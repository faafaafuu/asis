//! Что с компьютером: чем занят процессор, память, диски — и что за ошибка.
//!
//! Сведения берутся у самой Windows прямыми вызовами: список процессов, время
//! процессора каждого, частный рабочий набор памяти — тот же, что в столбце
//! «Память» диспетчера задач, — место на дисках. Через WMI первый такой вопрос
//! после запуска системы ждал десять секунд и больше, пока Windows готовила
//! классы производительности; прямые вызовы отвечают за время одного замера.
//!
//! Ответ о состоянии складывается из чисел без модели: «процессор загружен на
//! двадцать три процента, больше всего его занимает Chrome» — это факт, и
//! пересказывать его через модель значило бы рисковать, что она ошибётся в
//! цифре. Модель нужна там, где надо объяснить: что значит ошибка на экране и
//! что с ней делать.

use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// О чём спрашивают.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Overview,
    Cpu,
    Memory,
    Disk,
}

impl Topic {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "cpu" | "processor" => Self::Cpu,
            "memory" | "ram" => Self::Memory,
            "disk" | "disks" | "storage" => Self::Disk,
            _ => Self::Overview,
        }
    }
}

#[derive(Debug, Default)]
struct Snapshot {
    /// Загрузка процессора в целом, проценты.
    cpu: f64,
    /// Кто больше всего занимает процессор.
    by_cpu: Vec<Process>,
    /// Кто больше всего занимает память.
    by_memory: Vec<Process>,
    /// Всего памяти и свободно, ГБ.
    memory_total: f64,
    memory_free: f64,
    disks: Vec<Disk>,
    /// Сколько часов компьютер работает без перезагрузки.
    uptime_hours: f64,
}

#[derive(Debug, Default, Clone)]
struct Process {
    name: String,
    /// Проценты всего процессора.
    cpu: f64,
    /// Мегабайты.
    memory: f64,
}

#[derive(Debug, Default)]
struct Disk {
    letter: String,
    /// Гигабайты.
    size: f64,
    free: f64,
}

/// Сколько длится замер загрузки процессора.
///
/// Загрузка — это разница двух отсчётов времени процессора. Короче полсекунды
/// в замер попадает случайный всплеск; длиннее — человек ждёт ответа дольше,
/// чем нужно.
#[cfg(target_os = "windows")]
const SAMPLE: std::time::Duration = std::time::Duration::from_millis(700);

/// Снимок состояния: процессы, память, диски.
///
/// Процессы одной программы складываются: двадцать вкладок Chrome по два
/// процента — это Chrome на сорок процентов, а не двадцать мелочей.
#[cfg(target_os = "windows")]
fn snapshot() -> Result<Snapshot, String> {
    use std::collections::HashMap;

    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2};
    use windows::Win32::System::SystemInformation::{GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows::Win32::System::Threading::{
        GetProcessTimes, GetSystemTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const DRIVE_FIXED: u32 = 3;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = MB * 1024.0;

    fn ticks(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }

    /// Время всех процессоров всего и из него простой — в сотнях наносекунд.
    fn system_times() -> Option<(u64, u64)> {
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        // SAFETY: три указателя на локальные структуры, живущие до конца вызова.
        unsafe {
            GetSystemTimes(
                Some(&mut idle as *mut FILETIME),
                Some(&mut kernel as *mut FILETIME),
                Some(&mut user as *mut FILETIME),
            )
            .ok()?;
        }
        // Время ядра у Windows включает простой.
        Some((ticks(kernel) + ticks(user), ticks(idle)))
    }

    fn process_time(handle: HANDLE) -> Option<u64> {
        let mut times = [FILETIME::default(); 4];
        let [created, exited, kernel, user] = &mut times;
        // SAFETY: дескриптор открыт вызывающим; указатели на локальный массив.
        unsafe { GetProcessTimes(handle, created, exited, kernel, user).ok()? };
        Some(ticks(times[2]) + ticks(times[3]))
    }

    // SAFETY: снимок процессов читается и закрывается здесь же; дескрипторы
    // процессов открываются только на чтение и закрываются после замера.
    unsafe {
        let mut listed: Vec<(u32, String)> = Vec::new();
        let list = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|err| format!("список процессов не получен: {err}"))?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(list, &mut entry).is_ok() {
            loop {
                let length = entry.szExeFile.iter().position(|unit| *unit == 0).unwrap_or(entry.szExeFile.len());
                listed.push((entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..length])));
                if Process32NextW(list, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(list);

        let before = system_times().ok_or("время процессора не прочиталось")?;
        let mut open: Vec<(String, HANDLE, u64)> = Vec::new();
        for (pid, name) in listed {
            // Ноль — «бездействие системы»: это простой, а не программа.
            if pid == 0 {
                continue;
            }
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            match process_time(handle) {
                Some(time) => open.push((name, handle, time)),
                None => {
                    let _ = CloseHandle(handle);
                }
            }
        }

        std::thread::sleep(SAMPLE);

        let after = system_times().ok_or("время процессора не прочиталось")?;
        let total = after.0.saturating_sub(before.0).max(1);
        let idle = after.1.saturating_sub(before.1);
        let cpu = total.saturating_sub(idle) as f64 / total as f64 * 100.0;

        let mut grouped: HashMap<String, Process> = HashMap::new();
        for (name, handle, started) in open {
            let spent = process_time(handle).unwrap_or(started).saturating_sub(started);
            let mut counters = PROCESS_MEMORY_COUNTERS_EX2 {
                cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
                ..Default::default()
            };
            let memory = match GetProcessMemoryInfo(
                handle,
                (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2).cast(),
                counters.cb,
            ) {
                Ok(()) => counters.PrivateWorkingSetSize as f64 / MB,
                Err(_) => 0.0,
            };
            let _ = CloseHandle(handle);

            // Без «.exe». Срез — по границе символа: имя может быть русским.
            let stem = name
                .len()
                .checked_sub(4)
                .filter(|&at| at > 0 && name.get(at..).is_some_and(|tail| tail.eq_ignore_ascii_case(".exe")));
            let program = match stem {
                Some(at) => name[..at].to_string(),
                None => name,
            };
            let entry = grouped.entry(program.to_lowercase()).or_insert_with(|| Process {
                name: program,
                ..Default::default()
            });
            entry.cpu += spent as f64 / total as f64 * 100.0;
            entry.memory += memory;
        }
        let processes: Vec<Process> = grouped.into_values().collect();
        let top = |key: fn(&Process) -> f64| {
            let mut sorted = processes.clone();
            sorted.sort_by(|a, b| key(b).total_cmp(&key(a)));
            sorted.truncate(5);
            sorted
        };

        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        let _ = GlobalMemoryStatusEx(&mut status);

        let mut disks = Vec::new();
        let mask = GetLogicalDrives();
        for index in 0..26u8 {
            if mask & (1 << index) == 0 {
                continue;
            }
            let letter = format!("{}:", (b'A' + index) as char);
            let root = HSTRING::from(format!("{letter}\\"));
            if GetDriveTypeW(&root) != DRIVE_FIXED {
                continue;
            }
            let (mut size, mut free) = (0u64, 0u64);
            let read = GetDiskFreeSpaceExW(
                &root,
                None,
                Some(&mut size as *mut u64),
                Some(&mut free as *mut u64),
            );
            if read.is_ok() && size > 0 {
                disks.push(Disk {
                    letter,
                    size: (size as f64 / GB).round(),
                    free: (free as f64 / GB * 10.0).round() / 10.0,
                });
            }
        }

        Ok(Snapshot {
            cpu,
            by_cpu: top(|p| p.cpu),
            by_memory: top(|p| p.memory),
            memory_total: status.ullTotalPhys as f64 / GB,
            memory_free: status.ullAvailPhys as f64 / GB,
            disks,
            uptime_hours: GetTickCount64() as f64 / 3_600_000.0,
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn snapshot() -> Result<Snapshot, String> {
    Err("сведения о системе есть только на Windows".into())
}

/// Короткий ответ вслух о состоянии компьютера.
pub fn status(topic: Topic) -> String {
    let snapshot = match snapshot() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            log::warn!("состояние системы не прочиталось: {err}");
            return "Не смог прочитать состояние системы.".into();
        }
    };
    describe(&snapshot, topic)
}

/// Имя процесса, как его говорят: без «.exe» и служебных хвостов.
fn spoken_name(name: &str) -> String {
    let name = name.trim_end_matches(".exe");
    match name.to_lowercase().as_str() {
        "msedge" => "Edge".into(),
        "chrome" => "Chrome".into(),
        "firefox" => "Firefox".into(),
        "explorer" => "Проводник".into(),
        "dwm" => "оконный менеджер Windows".into(),
        "system" => "система".into(),
        "memory compression" => "сжатие памяти Windows".into(),
        _ => name.to_string(),
    }
}

/// Число с одним знаком после запятой, как его пишут по-русски: «20,2».
fn decimal(value: f64) -> String {
    format!("{value:.1}").replace('.', ",")
}

fn gigabytes(megabytes: f64) -> String {
    if megabytes >= 1024.0 {
        format!("{} ГБ", decimal(megabytes / 1024.0))
    } else {
        format!("{megabytes:.0} МБ")
    }
}

fn describe(snapshot: &Snapshot, topic: Topic) -> String {
    let mut parts: Vec<String> = Vec::new();

    if matches!(topic, Topic::Overview | Topic::Cpu) {
        let mut line = format!("Процессор загружен на {:.0}%.", snapshot.cpu);
        // Верхушку по процессору называем, только если она что-то значит:
        // «больше всего занимает Chrome — 0,3%» — это шум, а не ответ.
        let busy: Vec<&Process> = snapshot.by_cpu.iter().filter(|p| p.cpu >= 1.0).take(3).collect();
        if !busy.is_empty() {
            let named: Vec<String> = busy
                .iter()
                .map(|p| format!("{} — {:.0}%", spoken_name(&p.name), p.cpu))
                .collect();
            line.push_str(&format!(" Больше всего его занимают: {}.", named.join(", ")));
        }
        parts.push(line);
    }

    if matches!(topic, Topic::Overview | Topic::Memory) {
        let used = (snapshot.memory_total - snapshot.memory_free).max(0.0);
        let mut line = format!(
            "Память: занято {} из {:.0} ГБ.",
            decimal(used),
            snapshot.memory_total
        );
        let heavy: Vec<String> = snapshot
            .by_memory
            .iter()
            .take(3)
            .map(|p| format!("{} — {}", spoken_name(&p.name), gigabytes(p.memory)))
            .collect();
        if !heavy.is_empty() {
            line.push_str(&format!(" Больше всего у {}.", heavy.join(", ")));
        }
        parts.push(line);
    }

    if matches!(topic, Topic::Overview | Topic::Disk) {
        let disks: Vec<String> = snapshot
            .disks
            .iter()
            .map(|disk| {
                let tight = disk.size > 0.0 && disk.free / disk.size < 0.1;
                format!(
                    "{} — свободно {:.0} из {:.0} ГБ{}",
                    disk.letter,
                    disk.free,
                    disk.size,
                    if tight { ", места мало" } else { "" }
                )
            })
            .collect();
        if !disks.is_empty() {
            parts.push(format!("Диски: {}.", disks.join("; ")));
        }
    }

    if topic == Topic::Overview && snapshot.uptime_hours >= 72.0 {
        parts.push(format!(
            "Компьютер не перезагружался {:.0} дня — если что-то тормозит, перезагрузка часто помогает.",
            snapshot.uptime_hours / 24.0
        ));
    }

    parts.join(" ")
}

/* ── Что за ошибка ──────────────────────────────────────────────────────── */

/// Недавние ошибки из журнала Windows: последние полчаса, самые свежие.
///
/// Только «ошибка» и «критическая» — предупреждений в журнале сотни, и среди
/// них настоящая причина утонула бы.
const EVENTS_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Get-WinEvent -FilterHashtable @{ LogName = 'Application','System'; Level = 1,2; StartTime = (Get-Date).AddMinutes(-30) } -MaxEvents 8 |
  ForEach-Object {
    $text = ($_.Message -replace '\s+', ' ')
    if ($text.Length -gt 300) { $text = $text.Substring(0, 300) }
    '{0:HH:mm} {1} (код {2}): {3}' -f $_.TimeCreated, $_.ProviderName, $_.Id, $text
  }
"#;

/// Объясняет, что случилось, и отдаёт ответ.
///
/// Смотрит туда же, куда смотрит человек: текст окна, которое было впереди, —
/// чаще всего это и есть окно с ошибкой, — плюс недавние ошибки из журнала
/// Windows и загрузка системы. Модель по этим данным объясняет причину и
/// говорит, что сделать. Выдумывать ей запрещено: если по данным причина не
/// видна, она так и говорит и называет, что проверить.
pub async fn diagnose(app: &AppHandle, question: &str) -> String {
    let window = tauri::async_runtime::spawn_blocking(crate::pc::last_window_text)
        .await
        .unwrap_or_default();
    let events = tauri::async_runtime::spawn_blocking(|| powershell(EVENTS_SCRIPT))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let load = tauri::async_runtime::spawn_blocking(|| status(Topic::Overview))
        .await
        .unwrap_or_default();

    let rules = format!(
        "Ты — Ноа, голосовой помощник на компьютере с Windows. Человек спрашивает о \
         проблеме. Ниже — текст окна, которое у него сейчас впереди, недавние ошибки \
         из журнала Windows и состояние системы. Объясни по-русски, коротко и \
         простыми словами: что случилось — одним-двумя предложениями, — и что \
         сделать — двумя-четырьмя шагами. Опирайся только на данные ниже; если \
         причина по ним не видна, так и скажи и назови, что проверить. Не выдумывай \
         названия программ и ошибок, которых нет в данных. Обычный текст, без \
         разметки.\n\n\
         Окно впереди:\n{}\n\n\
         Ошибки в журнале за полчаса:\n{}\n\n\
         Состояние: {}",
        if window.trim().is_empty() { "(текста не видно)".to_string() } else { window },
        if events.trim().is_empty() { "(ошибок нет)".to_string() } else { events },
        load
    );

    let provider = app.state::<AppState>().provider();
    match provider.interpret(&rules, question).await {
        Ok(answer) if !answer.trim().is_empty() => answer.trim().to_string(),
        _ => "Не смог разобраться: модель не ответила. Посмотрите текст ошибки в окне — \
              и спросите ещё раз."
            .into(),
    }
}

/// Запускает PowerShell без окна и отдаёт вывод.
pub(crate) fn powershell(script: &str) -> Result<String, String> {
    let encoded = encoded(script);
    let mut command = std::process::Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", encoded.as_str()]);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|err| format!("PowerShell не запустился: {err}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Сценарий для `-EncodedCommand`: UTF-16 в base64.
///
/// Через `-Command` сценарий шёл бы строкой командной строки, и кавычки в нём
/// проходили бы два слоя экранирования — Rust и самого PowerShell. Запрос к
/// индексу поиска с кавычками в SQL на этом терялся молча: индекс отвечал, а
/// до него ничего не доходило. Закодированному сценарию экранирование не нужно.
fn encoded(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for at in 0..4 {
            if at <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * at)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Snapshot {
        Snapshot {
            cpu: 34.0,
            by_cpu: vec![
                Process { name: "MortalShell2-Win64-Shipping".into(), cpu: 21.4, memory: 5200.0 },
                Process { name: "chrome".into(), cpu: 6.0, memory: 3100.0 },
                Process { name: "svchost".into(), cpu: 0.4, memory: 800.0 },
            ],
            by_memory: vec![
                Process { name: "MortalShell2-Win64-Shipping".into(), cpu: 21.4, memory: 5200.0 },
                Process { name: "chrome".into(), cpu: 6.0, memory: 3100.0 },
            ],
            memory_total: 31.9,
            memory_free: 12.4,
            disks: vec![
                Disk { letter: "C:".into(), size: 419.0, free: 20.0 },
                Disk { letter: "D:".into(), size: 315.0, free: 120.0 },
            ],
            uptime_hours: 100.0,
        }
    }

    #[test]
    fn the_overview_names_the_heaviest() {
        let text = describe(&sample(), Topic::Overview);
        assert!(text.contains("Процессор загружен на 34%"), "{text}");
        assert!(text.contains("MortalShell2-Win64-Shipping — 21%"), "{text}");
        assert!(text.contains("Chrome"), "{text}");
        // Меньше процента — не ответ, а шум.
        assert!(!text.contains("svchost — 0%"), "{text}");
        assert!(text.contains("Память: занято 19,5 из 32 ГБ."), "{text}");
        assert!(text.contains("C: — свободно 20 из 419 ГБ, места мало"), "{text}");
        assert!(text.contains("не перезагружался"), "{text}");
    }

    #[test]
    fn a_topic_keeps_the_answer_short() {
        let disk = describe(&sample(), Topic::Disk);
        assert!(disk.starts_with("Диски:"), "{disk}");
        assert!(!disk.contains("Процессор"), "{disk}");
        assert_eq!(Topic::parse("memory"), Topic::Memory);
        assert_eq!(Topic::parse("что угодно"), Topic::Overview);
    }

    #[test]
    fn memory_is_spoken_in_gigabytes() {
        assert_eq!(gigabytes(3100.0), "3,0 ГБ");
        assert_eq!(gigabytes(512.0), "512 МБ");
    }

    #[test]
    fn scripts_are_encoded_as_powershell_expects() {
        // UTF-16 в base64 — так PowerShell ждёт `-EncodedCommand`.
        assert_eq!(encoded("A"), "QQA=");
        assert_eq!(encoded("hi"), "aABpAA==");
    }
}

#[cfg(test)]
mod live {
    /// `cargo test --lib sysinfo::live -- --ignored --nocapture`
    #[test]
    #[ignore = "читает состояние настоящей машины"]
    fn what_the_machine_says() {
        for (label, topic) in [
            ("обзор", super::Topic::Overview),
            ("процессор", super::Topic::Cpu),
            ("память", super::Topic::Memory),
            ("диски", super::Topic::Disk),
        ] {
            let started = std::time::Instant::now();
            let said = super::status(topic);
            println!("{label} ({} мс): {said}", started.elapsed().as_millis());
            assert!(!said.is_empty());
        }
        let events = super::powershell(super::EVENTS_SCRIPT);
        println!("журнал: {:?}", events.map(|text| text.lines().count()));
    }
}
