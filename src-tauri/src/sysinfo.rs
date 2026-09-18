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
    Gpu,
}

impl Topic {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "cpu" | "processor" => Self::Cpu,
            "memory" | "ram" => Self::Memory,
            "disk" | "disks" | "storage" => Self::Disk,
            "gpu" | "video" | "vram" | "graphics" => Self::Gpu,
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
    /// Видеокарта — если Windows о ней рассказала.
    gpu: Option<Gpu>,
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

#[derive(Debug, Default)]
struct Gpu {
    name: String,
    /// Видеопамять всего и занято, ГБ.
    total: f64,
    used: f64,
    /// Загрузка, проценты.
    load: Option<f64>,
    /// Температура, градусы, — если её сообщает драйвер.
    temperature: Option<f64>,
    /// Кто больше всего занимает видеопамять; `memory` — мегабайты.
    by_memory: Vec<Process>,
}

/// Сколько длится замер загрузки процессора.
///
/// Загрузка — это разница двух отсчётов времени процессора. Короче полсекунды
/// в замер попадает случайный всплеск; длиннее — человек ждёт ответа дольше,
/// чем нужно.
#[cfg(target_os = "windows")]
const SAMPLE: std::time::Duration = std::time::Duration::from_millis(700);

#[cfg(target_os = "windows")]
const MB: f64 = 1024.0 * 1024.0;
#[cfg(target_os = "windows")]
const GB: f64 = MB * 1024.0;

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

        // Видеокарта меряется в ту же паузу, что и процессор.
        let names: HashMap<u32, String> =
            listed.iter().map(|(pid, name)| (*pid, program_of(name))).collect();
        let gpu_probe = gpu::start();

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
        let gpu = gpu_probe.map(|probe| probe.finish(&names));

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

            let program = program_of(&name);
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
            gpu,
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn snapshot() -> Result<Snapshot, String> {
    Err("сведения о системе есть только на Windows".into())
}

/// Имя программы без «.exe». Срез — по границе символа: имя может быть русским.
fn program_of(name: &str) -> String {
    match name.len().checked_sub(4) {
        Some(at) if at > 0 && name.get(at..).is_some_and(|tail| tail.eq_ignore_ascii_case(".exe")) => {
            name[..at].to_string()
        }
        _ => name.to_string(),
    }
}

/// Кто по-настоящему занимает видеопамять — для ответа вслух.
///
/// Счётчик Windows по процессам честен не для всех: оконный менеджер и оверлей
/// NVIDIA держат ссылки на кадры всех окон и показывают по восемь-десять
/// гигабайт — больше, чем занято на всей карте. Вживую так и было: 10,3 ГБ у
/// оверлея и 8,6 ГБ у оконного менеджера при 8,8 ГБ занятых. Такие строки
/// отбрасываются: по ним человек решил бы, что видеопамять съела сама Windows.
fn heavy_on_gpu(mut processes: Vec<Process>, used_gb: f64) -> Vec<Process> {
    const SYSTEM: &[&str] = &[
        "dwm", "nvidia overlay", "nvidia share", "nvcontainer", "csrss", "system",
        "memory compression",
    ];
    processes.retain(|process| {
        process.memory >= 50.0
            && process.memory / 1024.0 <= used_gb + 0.25
            && !SYSTEM.contains(&process.name.to_lowercase().as_str())
    });
    processes.sort_by(|a, b| b.memory.total_cmp(&a.memory));
    processes.truncate(5);
    processes
}

/// Видеокарта: видеопамять и загрузка — по счётчикам Windows, тем же, что в
/// диспетчере задач; название и объём — у DXGI; температура — у драйвера
/// NVIDIA, если он её сообщает.
///
/// Счётчики берутся по английским именам: на русской Windows их пути
/// переведены, и английский путь без этого молча ничего бы не нашёл.
#[cfg(target_os = "windows")]
mod gpu {
    use std::collections::HashMap;

    use windows::core::{w, PCWSTR};
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
        PDH_MORE_DATA,
    };

    use super::{heavy_on_gpu, Gpu, Process, GB, MB};

    /// Видеокарта, о которой отвечать: с самой большой своей памятью.
    struct Card {
        name: String,
        /// Метка адаптера в именах счётчиков: «luid_0x00000000_0x0000de7a».
        luid: String,
        total: f64,
        nvidia: bool,
    }

    fn card() -> Option<Card> {
        // SAFETY: только чтение описаний видеокарт.
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
            let mut best: Option<Card> = None;
            let mut index = 0;
            while let Ok(adapter) = factory.EnumAdapters1(index) {
                index += 1;
                let Ok(desc) = adapter.GetDesc1() else {
                    continue;
                };
                // Программная видеокарта Windows — не железо.
                if desc.Flags & 2 != 0 {
                    continue;
                }
                let total = desc.DedicatedVideoMemory as f64 / GB;
                if best.as_ref().is_some_and(|card| card.total >= total) {
                    continue;
                }
                let length = desc
                    .Description
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(desc.Description.len());
                best = Some(Card {
                    name: String::from_utf16_lossy(&desc.Description[..length]).trim().to_string(),
                    luid: format!(
                        "luid_0x{:08x}_0x{:08x}",
                        desc.AdapterLuid.HighPart as u32, desc.AdapterLuid.LowPart
                    ),
                    total,
                    nvidia: desc.VendorId == 0x10DE,
                });
            }
            best
        }
    }

    /// Замер: открывается до паузы замера процессора, читается после неё.
    pub(super) struct Probe {
        query: PDH_HQUERY,
        adapters: PDH_HCOUNTER,
        processes: PDH_HCOUNTER,
        engines: PDH_HCOUNTER,
        card: Card,
        temperature: Option<std::process::Child>,
    }

    pub(super) fn start() -> Option<Probe> {
        let card = card()?;
        // SAFETY: запрос счётчиков открывается здесь и закрывается в `finish`.
        unsafe {
            let mut query = PDH_HQUERY(std::ptr::null_mut());
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            let add = |path: PCWSTR| {
                let mut counter = PDH_HCOUNTER(std::ptr::null_mut());
                (PdhAddEnglishCounterW(query, path, 0, &mut counter) == 0).then_some(counter)
            };
            let counters = (
                add(w!("\\GPU Adapter Memory(*)\\Dedicated Usage")),
                add(w!("\\GPU Process Memory(*)\\Dedicated Usage")),
                add(w!("\\GPU Engine(*)\\Utilization Percentage")),
            );
            let (Some(adapters), Some(processes), Some(engines)) = counters else {
                let _ = PdhCloseQuery(query);
                return None;
            };
            let _ = PdhCollectQueryData(query);
            let temperature = if card.nvidia { nvidia_temperature() } else { None };
            Some(Probe {
                query,
                adapters,
                processes,
                engines,
                card,
                temperature,
            })
        }
    }

    impl Probe {
        pub(super) fn finish(self, names: &HashMap<u32, String>) -> Gpu {
            let Probe {
                query,
                adapters,
                processes,
                engines,
                card,
                temperature,
            } = self;
            // SAFETY: запрос открыт в `start` и закрывается здесь же, после чтения.
            let (adapters, processes, engines) = unsafe {
                let _ = PdhCollectQueryData(query);
                let read = (values(adapters), values(processes), values(engines));
                let _ = PdhCloseQuery(query);
                read
            };
            let ours = |instance: &str| instance.to_lowercase().contains(&card.luid);

            let used = adapters
                .iter()
                .filter(|(instance, _)| ours(instance))
                .map(|(_, bytes)| bytes)
                .sum::<f64>()
                / GB;

            let mut memory: HashMap<String, Process> = HashMap::new();
            for (instance, bytes) in processes.iter().filter(|(instance, _)| ours(instance)) {
                let Some(pid) = pid_of(instance) else {
                    continue;
                };
                let program = names.get(&pid).cloned().unwrap_or_else(|| format!("процесс {pid}"));
                let entry = memory.entry(program.to_lowercase()).or_insert_with(|| Process {
                    name: program,
                    ..Default::default()
                });
                entry.memory += bytes / MB;
            }
            let by_memory = heavy_on_gpu(memory.into_values().collect(), used);

            // Загрузка — как в диспетчере задач: по каждому движку сумма всех
            // процессов, и берётся самый занятый движок.
            let mut per_engine: HashMap<String, f64> = HashMap::new();
            for (instance, percent) in engines.iter().filter(|(instance, _)| ours(instance)) {
                let engine = instance
                    .split_once("_luid_")
                    .map(|(_, rest)| rest.to_string())
                    .unwrap_or_default();
                *per_engine.entry(engine).or_default() += percent;
            }
            let load = per_engine.values().copied().reduce(f64::max).map(|load| load.min(100.0));

            let temperature = temperature
                .and_then(|child| child.wait_with_output().ok())
                .and_then(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .next()
                        .and_then(|line| line.trim().parse::<f64>().ok())
                });

            Gpu {
                name: card.name,
                total: card.total,
                used,
                load,
                temperature,
                by_memory,
            }
        }
    }

    /// Значения счётчика по всем экземплярам: имя экземпляра и число.
    fn values(counter: PDH_HCOUNTER) -> Vec<(String, f64)> {
        // SAFETY: буфер под массив выделяется по размеру, который назвала сама
        // PDH; строки имён живут в том же буфере и копируются до его освобождения.
        unsafe {
            for _ in 0..3 {
                let (mut size, mut count) = (0u32, 0u32);
                let status =
                    PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
                if status != PDH_MORE_DATA || size == 0 {
                    return Vec::new();
                }
                let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
                let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                let status = PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_DOUBLE,
                    &mut size,
                    &mut count,
                    Some(items),
                );
                // Пока читали, появились новые экземпляры — ещё раз.
                if status == PDH_MORE_DATA {
                    continue;
                }
                if status != 0 {
                    return Vec::new();
                }
                return std::slice::from_raw_parts(items, count as usize)
                    .iter()
                    .filter(|item| item.FmtValue.CStatus <= 1)
                    .map(|item| {
                        (
                            item.szName.to_string().unwrap_or_default(),
                            item.FmtValue.Anonymous.doubleValue,
                        )
                    })
                    .collect();
            }
            Vec::new()
        }
    }

    /// Номер процесса из имени экземпляра: «pid_10416_luid_…».
    fn pid_of(instance: &str) -> Option<u32> {
        instance.strip_prefix("pid_")?.split('_').next()?.parse().ok()
    }

    /// Температура у драйвера NVIDIA: запускается сразу, читается после паузы.
    fn nvidia_temperature() -> Option<std::process::Child> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let tool = std::path::Path::new(&std::env::var("SystemRoot").ok()?)
            .join("System32")
            .join("nvidia-smi.exe");
        if !tool.exists() {
            return None;
        }
        std::process::Command::new(tool)
            .args(["--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .ok()
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn the_process_number_is_read_from_the_counter_name() {
            assert_eq!(super::pid_of("pid_10416_luid_0x00000000_0x0000DE7A_phys_0"), Some(10416));
            assert_eq!(super::pid_of("luid_0x00000000_0x0000DE7A_phys_0"), None);
        }
    }
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

    if matches!(topic, Topic::Overview | Topic::Gpu) {
        match &snapshot.gpu {
            Some(gpu) => parts.push(describe_gpu(gpu, topic == Topic::Gpu)),
            None if topic == Topic::Gpu => {
                parts.push("Сведений о видеокарте Windows не отдала.".into())
            }
            None => {}
        }
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

/// Строка о видеокарте. `full` — отдельный вопрос о ней: с названием и тем,
/// кто занимает видеопамять; в обзоре — одна короткая строка.
fn describe_gpu(gpu: &Gpu, full: bool) -> String {
    let named = match (full, gpu.name.is_empty()) {
        (true, false) => format!(" {}", gpu.name),
        _ => String::new(),
    };
    let mut line = format!(
        "Видеокарта{named}: видеопамяти занято {} из {:.0} ГБ",
        decimal(gpu.used),
        gpu.total
    );
    if let Some(load) = gpu.load {
        line.push_str(&format!(", загрузка {load:.0}%"));
    }
    if let Some(temperature) = gpu.temperature {
        line.push_str(&format!(", температура {temperature:.0} °C"));
    }
    line.push('.');
    if full {
        let heavy: Vec<String> = gpu
            .by_memory
            .iter()
            .take(3)
            .map(|p| format!("{} — {}", spoken_name(&p.name), gigabytes(p.memory)))
            .collect();
        if !heavy.is_empty() {
            line.push_str(&format!(" Больше всего видеопамяти у {}.", heavy.join(", ")));
        }
    }
    line
}

/* ── Что не так с компьютером ───────────────────────────────────────────── */

/// Скан системы для «что не так»: устройства с ошибками, драйверы основных
/// устройств с датами, падения программ и ошибки Windows за сутки, службы
/// автозапуска, которые стоят, ожидание перезагрузки, диски, сеть и звук.
///
/// Раньше смотрели только ошибки журнала за полчаса, и на «проверь драйверы»
/// Ноа отвечала загрузкой процессора: про драйверы ей было просто нечего
/// сказать.
const SCAN_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

'## Устройства с ошибками'
$bad = Get-PnpDevice -PresentOnly | Where-Object { $_.Status -ne 'OK' -and $_.Status -ne 'Unknown' }
if ($bad) { $bad | Select-Object -First 8 | ForEach-Object { '{0} [{1}]: состояние {2}, код {3}' -f $_.FriendlyName, $_.Class, $_.Status, $_.Problem } } else { 'нет' }

'## Драйверы основных устройств'
Get-CimInstance Win32_PnPSignedDriver |
  Where-Object { $_.DeviceClass -in 'DISPLAY','NET','MEDIA','BLUETOOTH','USB','HDC','SCSIADAPTER' -and $_.DriverProviderName -ne 'Microsoft' -and $_.DeviceName } |
  Sort-Object DeviceClass, DeviceName -Unique | Select-Object -First 14 |
  ForEach-Object {
    $date = if ($_.DriverDate) { $_.DriverDate.ToString('yyyy-MM-dd') } else { '?' }
    '{0} [{1}]: версия {2}, от {3}, {4}' -f $_.DeviceName, $_.DeviceClass, $_.DriverVersion, $date, $_.DriverProviderName
  }

'## Падения программ за сутки'
$crashes = Get-WinEvent -FilterHashtable @{ LogName = 'Application'; Id = 1000, 1002; StartTime = (Get-Date).AddDays(-1) } -MaxEvents 30
if ($crashes) {
  $crashes | ForEach-Object { ($_.Properties[0].Value) } | Group-Object | Sort-Object Count -Descending | Select-Object -First 6 |
    ForEach-Object { '{0}: {1} раз' -f $_.Name, $_.Count }
} else { 'нет' }

'## Ошибки Windows за сутки'
$errors = Get-WinEvent -FilterHashtable @{ LogName = 'System', 'Application'; Level = 1, 2; StartTime = (Get-Date).AddDays(-1) } -MaxEvents 200
if ($errors) {
  $errors | Group-Object ProviderName, Id | Sort-Object Count -Descending | Select-Object -First 8 |
    ForEach-Object {
      $first = $_.Group[0]
      $text = ($first.Message -replace '\s+', ' ')
      if ($text.Length -gt 180) { $text = $text.Substring(0, 180) }
      '{0} раз, последний {1:dd.MM HH:mm} — {2} (код {3}): {4}' -f $_.Count, $first.TimeCreated, $first.ProviderName, $first.Id, $text
    }
} else { 'нет' }

'## Службы автозапуска, которые стоят'
$stopped = Get-CimInstance Win32_Service | Where-Object { $_.StartMode -eq 'Auto' -and $_.State -ne 'Running' -and $_.ExitCode -ne 0 }
if ($stopped) { $stopped | Select-Object -First 6 | ForEach-Object { '{0} ({1}): код выхода {2}' -f $_.DisplayName, $_.Name, $_.ExitCode } } else { 'нет' }

'## Прочее'
$pending = (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') -or
  (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired')
if ($pending) { 'Windows ждёт перезагрузки после обновлений.' }
$os = Get-CimInstance Win32_OperatingSystem
'Windows: {0} {1}, без перезагрузки {2:N0} ч' -f $os.Caption, $os.BuildNumber, ((Get-Date) - $os.LastBootUpTime).TotalHours
Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | ForEach-Object {
  '{0} свободно {1:N0} из {2:N0} ГБ' -f $_.DeviceID, ($_.FreeSpace / 1GB), ($_.Size / 1GB)
}
Get-NetAdapter | Where-Object Status -eq 'Up' | Select-Object -First 3 | ForEach-Object { 'Сеть: {0} — {1}' -f $_.Name, $_.LinkSpeed }
$audio = Get-CimInstance Win32_SoundDevice | ForEach-Object { '{0} ({1})' -f $_.Name, $_.Status }
'Звук: ' + ($audio -join '; ')
"#;

/// Разбирается, что не так, и отдаёт ответ.
///
/// Смотрит туда же, куда смотрел бы мастер: окно, которое было впереди, —
/// чаще всего это и есть окно с ошибкой, — скан системы и загрузку. Модель
/// называет причину и предлагает, что сделать. Выдумывать ей запрещено.
pub async fn diagnose(app: &AppHandle, question: &str) -> String {
    let window = tauri::async_runtime::spawn_blocking(crate::pc::last_window_text)
        .await
        .unwrap_or_default();
    let scan = tauri::async_runtime::spawn_blocking(|| powershell(SCAN_SCRIPT))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let load = tauri::async_runtime::spawn_blocking(|| status(Topic::Overview))
        .await
        .unwrap_or_default();

    let name = app.state::<AppState>().wake_name();
    let rules = format!(
        "Ты — {name}, помощник на компьютере человека с Windows, и у тебя есть данные \
         с этого компьютера — ниже. Человек спрашивает о проблеме или просит проверить \
         систему. Ответь по-русски, как мастер другу: сначала главное — что не так, \
         одним-двумя предложениями, с конкретным названием устройства, программы или \
         драйвера из данных; потом, что с этим сделать, одним-двумя предложениями. \
         Если есть что поправить — закончи коротким предложением сделать это: \
         «Открыть очистку диска?», «Открыть центр обновления?». Отвечай ровно на \
         вопрос: спросили про драйверы — говори про драйверы, а не про процессор. \
         Старым драйвер считай, если ему больше двух лет. Если по данным всё в \
         порядке — так и скажи одной фразой. Не выдумывай того, чего в данных нет. \
         Без списков, без разметки, без извинений.\n\n\
         Окно впереди:\n{}\n\n\
         Скан системы:\n{}\n\n\
         Загрузка: {}",
        if window.trim().is_empty() { "(текста не видно)".to_string() } else { window },
        if scan.trim().is_empty() { "(скан не удался)".to_string() } else { scan },
        load
    );

    let provider = app.state::<AppState>().provider();
    match provider.advise(&rules, question).await {
        Ok(answer) if !answer.trim().is_empty() => answer.trim().to_string(),
        _ => "Не смогла разобраться: модель не ответила. Спросите ещё раз.".into(),
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
            gpu: Some(Gpu {
                name: "NVIDIA GeForce RTX 3080".into(),
                total: 10.0,
                used: 9.2,
                load: Some(50.0),
                temperature: Some(42.0),
                by_memory: vec![
                    Process { name: "llama-server".into(), cpu: 0.0, memory: 4700.0 },
                    Process { name: "Cyberpunk2077".into(), cpu: 0.0, memory: 2048.0 },
                ],
            }),
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
    fn the_video_card_is_its_own_answer() {
        let text = describe(&sample(), Topic::Gpu);
        assert!(
            text.starts_with(
                "Видеокарта NVIDIA GeForce RTX 3080: видеопамяти занято 9,2 из 10 ГБ, \
                 загрузка 50%, температура 42 °C."
            ),
            "{text}"
        );
        assert!(text.contains("llama-server — 4,6 ГБ"), "{text}");
        assert!(!text.contains("Процессор"), "{text}");
        assert_eq!(Topic::parse("gpu"), Topic::Gpu);
    }

    #[test]
    fn the_overview_mentions_the_video_card_briefly() {
        let text = describe(&sample(), Topic::Overview);
        assert!(text.contains("Видеокарта: видеопамяти занято 9,2 из 10 ГБ"), "{text}");
        assert!(!text.contains("llama-server — 4,6"), "{text}");
    }

    #[test]
    fn the_window_manager_does_not_eat_the_video_card() {
        // Цифры из живого замера: на карте занято 8,8 ГБ.
        let listed = vec![
            Process { name: "NVIDIA Overlay".into(), cpu: 0.0, memory: 10_547.0 },
            Process { name: "dwm".into(), cpu: 0.0, memory: 8_806.0 },
            Process { name: "llama-server".into(), cpu: 0.0, memory: 4_403.0 },
            Process { name: "chrome".into(), cpu: 0.0, memory: 20.0 },
        ];
        let heavy = heavy_on_gpu(listed, 8.8);
        let names: Vec<&str> = heavy.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["llama-server"]);
    }

    #[test]
    fn programs_are_named_without_exe() {
        assert_eq!(program_of("chrome.exe"), "chrome");
        assert_eq!(program_of("Яндекс Музыка.exe"), "Яндекс Музыка");
        assert_eq!(program_of("System"), "System");
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
            ("видеокарта", super::Topic::Gpu),
        ] {
            let started = std::time::Instant::now();
            let said = super::status(topic);
            println!("{label} ({} мс): {said}", started.elapsed().as_millis());
            assert!(!said.is_empty());
        }
        let scan = super::powershell(super::SCAN_SCRIPT);
        println!("скан: {:?}", scan.map(|text| text.lines().count()));
    }
}
