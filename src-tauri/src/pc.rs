//! Компьютер голосом: открыть программу или окно, закрыть программу, усыпить,
//! выключить, перезагрузить или заблокировать машину.
//!
//! Главная трудность не в запуске — запускать умеет оболочка, — а в том, чтобы
//! понять, что назвали. Человек говорит «открой телеграм», распознавание пишет
//! «телеграм», а программа называется Telegram; «клауде» и «клод» — это Claude;
//! «мортал шелл два» — папка «Mortal Shell II (2026)». Поэтому названия
//! сравниваются не буквами, а звучанием: кириллица переводится в латиницу, у
//! слов остаётся согласный остов, а римские цифры и числительные сводятся к
//! цифрам. См. `score`.
//!
//! Искать есть где. Системные окна и оснастки — по таблице: у «диспетчера
//! устройств» нет ярлыка в меню «Пуск», только имя оснастки. Установленные
//! программы — по меню «Пуск», откуда их запускает и сам человек, включая
//! приложения из магазина. И папки с играми на дисках: игры, поставленные мимо
//! установщика, в меню «Пуск» не попадают вовсе.
//!
//! Выключение и перезагрузка идут с минутной отсрочкой. Распознавание ошибается,
//! а выключенный посреди работы компьютер — это потерянная работа; минута даёт
//! сказать «отмени выключение». Сон и блокировка ничего не теряют и происходят
//! сразу, как только прозвучит ответ.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Что сделать с самим компьютером.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Sleep,
    Shutdown,
    Restart,
    Lock,
    /// Отменить назначенное выключение или перезагрузку.
    Cancel,
}

impl Power {
    /// Действие по слову из разбора реплики.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "sleep" | "suspend" => Some(Self::Sleep),
            "shutdown" | "off" => Some(Self::Shutdown),
            "restart" | "reboot" => Some(Self::Restart),
            "lock" => Some(Self::Lock),
            "cancel" | "abort" => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// Отсрочка выключения и перезагрузки, в секундах.
const SHUTDOWN_DELAY_SECS: &str = "60";

/// Выполняет действие с компьютером и отдаёт то, что сказать вслух.
pub fn power(action: Power) -> String {
    match action {
        // Не сразу: сначала должен прозвучать ответ, иначе человек не поймёт,
        // услышали ли его или компьютер завис.
        Power::Sleep => {
            after(Duration::from_secs(5), suspend);
            "Усыпляю компьютер.".into()
        }
        Power::Lock => {
            after(Duration::from_secs(2), lock);
            "Блокирую экран.".into()
        }
        Power::Shutdown => match schedule("/s") {
            Ok(()) => "Выключу компьютер через минуту. Если передумали — скажите «отмени выключение».".into(),
            Err(err) => format!("Выключить не вышло: {err}."),
        },
        Power::Restart => match schedule("/r") {
            Ok(()) => "Перезагружу компьютер через минуту. Если передумали — скажите «отмени перезагрузку».".into(),
            Err(err) => format!("Перезагрузить не вышло: {err}."),
        },
        // `shutdown /a` отвечает ошибкой, когда отменять нечего, — это не сбой,
        // а просто нечего было отменять.
        Power::Cancel => match run_hidden("shutdown", &["/a"]) {
            Ok(()) => "Отменил. Компьютер остаётся включённым.".into(),
            Err(_) => "Выключение и не было назначено.".into(),
        },
    }
}

fn schedule(flag: &str) -> Result<(), String> {
    run_hidden(
        "shutdown",
        &[flag, "/t", SHUTDOWN_DELAY_SECS, "/c", "Суфлёр: по голосовой команде"],
    )
}

fn after(delay: Duration, action: fn()) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        action();
    });
}

/// Запускает системную программу без окна консоли и ждёт её ответа.
fn run_hidden(program: &str, args: &[&str]) -> Result<(), String> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let status = command
        .status()
        .map_err(|err| format!("не запустился {program}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} ответил кодом {}", status.code().unwrap_or(-1)))
    }
}

#[cfg(target_os = "windows")]
fn suspend() {
    // SAFETY: просьба к системе уснуть; ничего не читает и не пишет в память.
    // Первый аргумент false — именно сон, а не гибернация.
    unsafe {
        let _ = windows::Win32::System::Power::SetSuspendState(false, false, false);
    }
}

#[cfg(target_os = "windows")]
fn lock() {
    // SAFETY: без аргументов, только просьба к системе.
    unsafe {
        let _ = windows::Win32::System::Shutdown::LockWorkStation();
    }
}

#[cfg(not(target_os = "windows"))]
fn suspend() {
    let _ = run_hidden("systemctl", &["suspend"]);
}

#[cfg(not(target_os = "windows"))]
fn lock() {
    let _ = run_hidden("loginctl", &["lock-session"]);
}

/* ── Что можно открыть ──────────────────────────────────────────────────── */

/// Окна и оснастки Windows, у которых нет ярлыка в меню «Пуск».
///
/// Имя слева — как это называет человек; справа — что понимает оболочка.
/// Несколько имён на одно окно, потому что зовут его по-разному.
const SYSTEM: &[(&str, &str)] = &[
    ("диспетчер устройств", "devmgmt.msc"),
    ("устройства", "devmgmt.msc"),
    ("диспетчер задач", "taskmgr.exe"),
    ("управление дисками", "diskmgmt.msc"),
    ("управление компьютером", "compmgmt.msc"),
    ("службы", "services.msc"),
    ("просмотр событий", "eventvwr.msc"),
    ("монитор ресурсов", "resmon.exe"),
    ("редактор реестра", "regedit.exe"),
    ("панель управления", "control.exe"),
    ("параметры", "ms-settings:"),
    ("настройки windows", "ms-settings:"),
    ("программы и компоненты", "appwiz.cpl"),
    ("сетевые подключения", "ncpa.cpl"),
    ("звук", "mmsys.cpl"),
    ("проводник", "explorer.exe"),
    ("командная строка", "cmd.exe"),
    ("терминал", "wt.exe"),
    ("блокнот", "notepad.exe"),
    ("калькулятор", "calc.exe"),
    ("загрузки", "shell:Downloads"),
    ("документы", "shell:Personal"),
    ("корзина", "shell:RecycleBinFolder"),
];

#[derive(Clone, Debug)]
struct Entry {
    /// Как называется — это же имя звучит в ответе.
    name: String,
    target: Target,
}

#[derive(Clone, Debug)]
enum Target {
    /// Имя, которое оболочка найдёт сама: оснастка, системная программа, адрес.
    Shell(String),
    /// Приложение из меню «Пуск». `path` — исполняемый файл, если его удалось
    /// вывести из идентификатора: без него программу не запустить в песочнице.
    StartApp { id: String, path: Option<PathBuf> },
    /// Папка программы на диске; что в ней запускать, решается при запуске.
    Folder(PathBuf),
}

/// Сколько держим собранный список. Меню «Пуск» читается через PowerShell —
/// это секунда-другая, и платить её на каждую просьбу незачем. Но и навсегда
/// запоминать нельзя: только что поставленная программа должна находиться.
const CATALOG_TTL: Duration = Duration::from_secs(10 * 60);

static CATALOG: Mutex<Option<(Instant, Vec<Entry>)>> = Mutex::new(None);

fn catalog() -> Vec<Entry> {
    if let Some((at, entries)) = CATALOG.lock().unwrap_or_else(|err| err.into_inner()).as_ref() {
        if at.elapsed() < CATALOG_TTL {
            return entries.clone();
        }
    }

    let mut entries: Vec<Entry> = SYSTEM
        .iter()
        .map(|(name, command)| Entry {
            name: (*name).to_string(),
            target: Target::Shell((*command).to_string()),
        })
        .collect();
    entries.extend(start_apps());
    entries.extend(program_folders());

    *CATALOG.lock().unwrap_or_else(|err| err.into_inner()) = Some((Instant::now(), entries.clone()));
    entries
}

/// Приложения из меню «Пуск» — тем же списком, что видит сам человек.
///
/// `Get-StartApps` знает и обычные программы, и приложения из магазина, и
/// у каждого даёт идентификатор, по которому оболочка запускает его одинаково.
#[cfg(target_os = "windows")]
fn start_apps() -> Vec<Entry> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // Кодировку вывода задаём явно: в канал PowerShell пишет в кодовой
    // странице консоли, и русские названия приходили бы кракозябрами.
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let Ok(output) = output else {
        log::warn!("меню «Пуск» не прочиталось: PowerShell не запустился");
        return Vec::new();
    };
    let raw = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(raw.trim()).unwrap_or_default();
    // Одно приложение PowerShell отдаёт объектом, а не массивом из одного.
    let items = match parsed {
        serde_json::Value::Array(items) => items,
        single @ serde_json::Value::Object(_) => vec![single],
        _ => Vec::new(),
    };

    items
        .iter()
        .filter_map(|item| {
            let name = item["Name"].as_str()?.trim().to_string();
            let id = item["AppID"].as_str()?.trim().to_string();
            // Ссылки на сайты поддержки в меню «Пуск» — не программы.
            if name.is_empty() || id.starts_with("http") {
                return None;
            }
            let path = executable_of(&id);
            Some(Entry {
                name,
                target: Target::StartApp { id, path },
            })
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn start_apps() -> Vec<Entry> {
    Vec::new()
}

/// Исполняемый файл по идентификатору приложения, если он из него выводится.
///
/// Обычные программы меню «Пуск» идентифицируются путём от известной папки:
/// `{7C5A40EF-…}\Steam\Steam.exe` — это `Program Files (x86)\Steam\Steam.exe`.
/// У приложений из магазина пути нет вовсе.
fn executable_of(id: &str) -> Option<PathBuf> {
    const KNOWN: &[(&str, &str, &str)] = &[
        ("{6D809377-6AF0-444B-8957-A3773F02200E}", "ProgramW6432", ""),
        ("{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}", "ProgramFiles(x86)", ""),
        ("{F38BF404-1D43-42F2-9305-67DE0B28FC23}", "SystemRoot", ""),
        ("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}", "SystemRoot", "System32"),
    ];

    if !id.to_lowercase().ends_with(".exe") {
        return None;
    }
    for (guid, variable, inner) in KNOWN {
        if let Some(rest) = id.strip_prefix(guid) {
            let base = std::env::var(variable).ok()?;
            let path = Path::new(&base)
                .join(inner)
                .join(rest.trim_start_matches('\\'));
            return path.exists().then_some(path);
        }
    }
    let path = PathBuf::from(id);
    (path.is_absolute() && path.exists()).then_some(path)
}

/// Папки программ и игр на дисках.
///
/// Смотрим туда, куда игры ставят обычно: корни несистемных дисков, папки
/// Games, библиотеки Steam и Epic. Системный диск целиком не обходим — в его
/// корне и так только системные папки, а игры на нём живут в перечисленных.
fn program_folders() -> Vec<Entry> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for letter in 'C'..='Z' {
        let drive = PathBuf::from(format!("{letter}:\\"));
        if !drive.exists() {
            continue;
        }
        if letter != 'C' {
            roots.push(drive.clone());
        }
        for inner in [
            "Games",
            "SteamLibrary\\steamapps\\common",
            "Steam\\steamapps\\common",
            "Program Files (x86)\\Steam\\steamapps\\common",
            "Epic Games",
        ] {
            roots.push(drive.join(inner));
        }
    }

    let mut entries = Vec::new();
    for root in roots {
        let Ok(children) = std::fs::read_dir(&root) else { continue };
        for child in children.flatten() {
            let path = child.path();
            if !path.is_dir() {
                continue;
            }
            let name = child.file_name().to_string_lossy().to_string();
            // Служебные папки дисков: они никогда не то, что просят открыть.
            if name.starts_with('$') || name.starts_with('.') {
                continue;
            }
            entries.push(Entry {
                name,
                target: Target::Folder(path),
            });
        }
    }
    entries
}

/// Что запускать в папке программы.
///
/// Ближайший к корню исполняемый файл, а среди равных — самый большой. В
/// игре на Unreal в корне лежит маленький загрузчик, а настоящий файл —
/// глубоко в `Binaries\Win64`; запускать надо загрузчик, он передаёт игре
/// нужные параметры. Установщики, отчёты о сбоях и помощники отсеиваются по
/// имени.
fn main_exe(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<(usize, u64, PathBuf)> = Vec::new();
    let mut budget = 20_000usize;
    collect_exes(dir, 0, &mut found, &mut budget);
    found.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    found.into_iter().next().map(|(_, _, path)| path)
}

fn collect_exes(dir: &Path, depth: usize, found: &mut Vec<(usize, u64, PathBuf)>, budget: &mut usize) {
    const JUNK: &[&str] = &[
        "crash", "unins", "setup", "redist", "helper", "report", "prereq", "dotnet", "install",
        "update", "cef", "vc_", "dxweb",
    ];
    if depth > 5 {
        return;
    }
    let Ok(children) = std::fs::read_dir(dir) else { return };
    for child in children.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = child.path();
        if path.is_dir() {
            collect_exes(&path, depth + 1, found, budget);
            continue;
        }
        let name = child.file_name().to_string_lossy().to_lowercase();
        if !name.ends_with(".exe") || JUNK.iter().any(|junk| name.contains(junk)) {
            continue;
        }
        let size = child.metadata().map(|meta| meta.len()).unwrap_or(0);
        found.push((depth, size, path));
    }
}

/* ── Открыть и закрыть ──────────────────────────────────────────────────── */

/// Насколько название должно совпасть с услышанным, чтобы считаться им.
const MATCH_THRESHOLD: f32 = 0.6;

fn best_match<'a>(spoken: &str, entries: &'a [Entry]) -> Option<&'a Entry> {
    let mut best: Option<(&Entry, f32)> = None;
    for entry in entries {
        let value = score(spoken, &entry.name);
        // Строго больше: при равенстве остаётся первое, а таблица системных
        // окон стоит первой — «проводник» это окно, а не папка с таким именем.
        if value >= MATCH_THRESHOLD && best.map_or(true, |(_, top)| value > top) {
            best = Some((entry, value));
        }
    }
    best.map(|(entry, _)| entry)
}

/// Открывает программу, окно или игру по названию и отдаёт ответ вслух.
///
/// `sandbox` — запустить в песочнице Sandboxie; `sandbox_box` — её имя, пустое
/// значит песочницу по умолчанию.
pub fn launch(spoken: &str, sandbox: bool, sandbox_box: &str) -> String {
    let entries = catalog();
    let Some(entry) = best_match(spoken, &entries) else {
        return format!("Не нашёл, что открыть по «{spoken}».");
    };
    log::info!("открываю «{}» по «{spoken}»", entry.name);

    if sandbox {
        return launch_sandboxed(entry, sandbox_box);
    }

    let result = match &entry.target {
        Target::Shell(command) => open(command),
        // Идентификатор-адрес (steam://…) открывается сам, остальные — через
        // папку приложений оболочки, одинаково для программ и магазина.
        Target::StartApp { id, .. } if id.contains("://") => open(id),
        Target::StartApp { id, .. } => open(&format!("shell:AppsFolder\\{id}")),
        Target::Folder(dir) => match main_exe(dir) {
            Some(exe) => open_program(&exe, ""),
            None => Err("в папке нет запускаемого файла".into()),
        },
    };

    match result {
        Ok(()) => format!("Открываю {}.", entry.name),
        Err(err) => format!("Открыть {} не вышло: {err}.", entry.name),
    }
}

fn launch_sandboxed(entry: &Entry, sandbox_box: &str) -> String {
    let Some(start) = sandboxie() else {
        return "Sandboxie не установлена — запускать в песочнице нечем.".into();
    };

    let exe = match &entry.target {
        Target::Folder(dir) => main_exe(dir),
        Target::StartApp { path, .. } => path.clone(),
        Target::Shell(command) if command.ends_with(".exe") => Some(PathBuf::from(command)),
        Target::Shell(_) => None,
    };
    let Some(exe) = exe else {
        // Приложения из магазина и системные оснастки Sandboxie не запускает:
        // ей нужен исполняемый файл, а у них его нет в привычном смысле.
        return format!("{} в песочнице не запустить: у неё нет файла программы.", entry.name);
    };

    // Имя песочницы уходит в командную строку — пропускаем только то, из чего
    // имена песочниц и состоят, чтобы кавычка не превратилась в параметр.
    let name: String = sandbox_box
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    let name = if name.is_empty() { "DefaultBox".to_string() } else { name };

    match open_program(&start, &format!("/box:{name} \"{}\"", exe.display())) {
        Ok(()) => format!("Запускаю {} в песочнице {name}.", entry.name),
        Err(err) => format!("Запустить в песочнице не вышло: {err}."),
    }
}

/// Где лежит запускалка Sandboxie.
fn sandboxie() -> Option<PathBuf> {
    ["Sandboxie-Plus", "Sandboxie"]
        .iter()
        .filter_map(|folder| {
            let base = std::env::var("ProgramW6432")
                .or_else(|_| std::env::var("ProgramFiles"))
                .ok()?;
            Some(Path::new(&base).join(folder).join("Start.exe"))
        })
        .find(|path| path.exists())
}

/// Закрывает программу по названию и отдаёт ответ вслух.
///
/// Закрывает так же, как крестик в углу окна: программа получает просьбу
/// закрыться и может спросить про несохранённое. Снимать процесс силой
/// нельзя — это потеря того, что человек не успел сохранить.
pub fn close(spoken: &str) -> String {
    let windows = open_windows();

    // Лучшая программа — по лучшему из её окон: сравниваем и с именем файла,
    // и с заголовком, потому что «закрой телеграм» совпадает с Telegram.exe,
    // а «закрой диспетчер задач» — только с заголовком окна.
    let mut best: Option<(String, f32)> = None;
    for window in &windows {
        let value = score(spoken, &window.exe).max(score(spoken, &window.title));
        if value >= MATCH_THRESHOLD && best.as_ref().map_or(true, |(_, top)| value > *top) {
            best = Some((window.exe.clone(), value));
        }
    }
    let Some((exe, _)) = best else {
        return format!("Не нашёл открытого окна «{spoken}».");
    };

    let mut closed = 0;
    for window in windows.iter().filter(|window| window.exe == exe) {
        if close_window(window.handle) {
            closed += 1;
        }
    }
    log::info!("закрываю {exe}: окон {closed}");

    match closed {
        0 => format!("{exe} не закрылся."),
        _ => format!("Закрываю {exe}."),
    }
}

struct OpenWindow {
    handle: isize,
    /// Имя исполняемого файла без расширения: Telegram, chrome.
    exe: String,
    title: String,
}

/// Видимые окна верхнего уровня других программ.
#[cfg(target_os = "windows")]
fn open_windows() -> Vec<OpenWindow> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    let own = unsafe { GetCurrentProcessId() };
    let mut found = Vec::new();
    let mut previous = None;

    // SAFETY: только чтение сведений об окнах и процессах; дескриптор процесса
    // закрывается сразу после чтения имени.
    unsafe {
        while let Ok(window) = FindWindowExW(None, previous, PCWSTR::null(), PCWSTR::null()) {
            if window.0.is_null() {
                break;
            }
            previous = Some(window);

            if !IsWindowVisible(window).as_bool() {
                continue;
            }
            let mut title = [0u16; 256];
            let length = GetWindowTextW(window, &mut title);
            if length <= 0 {
                continue;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid == 0 || pid == own {
                continue;
            }

            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            let mut path = [0u16; 1024];
            let mut size = path.len() as u32;
            let named = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(process);
            if named.is_err() {
                continue;
            }

            let path = String::from_utf16_lossy(&path[..size as usize]);
            let exe = Path::new(&path)
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            found.push(OpenWindow {
                handle: window.0 as isize,
                exe,
                title: String::from_utf16_lossy(&title[..length as usize]),
            });
        }
    }
    found
}

#[cfg(not(target_os = "windows"))]
fn open_windows() -> Vec<OpenWindow> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn close_window(handle: isize) -> bool {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    // SAFETY: просьба окну закрыться — то же, что нажатие на крестик.
    unsafe {
        PostMessageW(
            Some(HWND(handle as *mut core::ffi::c_void)),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        )
        .is_ok()
    }
}

#[cfg(not(target_os = "windows"))]
fn close_window(_handle: isize) -> bool {
    false
}

/// Открывает адрес, файл, оснастку или приложение так, как это сделала бы
/// оболочка по двойному щелчку.
pub fn open(target: &str) -> Result<(), String> {
    shell_execute(target, "", None)
}

/// Запускает программу с параметрами из её собственной папки.
///
/// Папка важна для игр: они ищут свои файлы рядом с собой, а запущенные из
/// чужой рабочей папки падают на старте.
fn open_program(exe: &Path, parameters: &str) -> Result<(), String> {
    shell_execute(&exe.to_string_lossy(), parameters, exe.parent())
}

#[cfg(target_os = "windows")]
fn shell_execute(file: &str, parameters: &str, directory: Option<&Path>) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let verb = wide("open");
    let file_w = wide(file);
    let parameters_w = wide(parameters);
    let directory_w = directory.map(|dir| wide(&dir.to_string_lossy()));

    // SAFETY: все строки живут до конца вызова и оканчиваются нулём.
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file_w.as_ptr()),
            if parameters.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(parameters_w.as_ptr())
            },
            directory_w
                .as_ref()
                .map_or(PCWSTR::null(), |dir| PCWSTR(dir.as_ptr())),
            SW_SHOWNORMAL,
        )
    };

    // Больше 32 — успех; меньше — код ошибки оболочки.
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(format!("оболочка отказала (код {code})"))
    }
}

#[cfg(not(target_os = "windows"))]
fn shell_execute(file: &str, parameters: &str, _directory: Option<&Path>) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let mut command = std::process::Command::new(opener);
    command.arg(file);
    if !parameters.is_empty() {
        command.args(parameters.split_whitespace());
    }
    command.spawn().map(|_| ()).map_err(|err| err.to_string())
}

/* ── Сравнение названий на слух ─────────────────────────────────────────── */

/// Насколько название похоже на услышанное: от 0 до 1.
///
/// Сравниваются слова по звучанию (см. `words`). Доля совпавших слов запроса —
/// основа оценки; каждое лишнее слово в названии немного её снижает, чтобы
/// «стим» выбирал Steam, а не Steam Support Center.
fn score(spoken: &str, name: &str) -> f32 {
    let asked = words(spoken);
    let named = words(name);
    if asked.is_empty() || named.is_empty() {
        return 0.0;
    }

    let matched = asked
        .iter()
        .filter(|word| named.iter().any(|other| same_word(word, other)))
        .count();
    let extra = named.len().saturating_sub(matched).min(4) as f32;

    matched as f32 / asked.len() as f32 - 0.05 * extra
}

fn same_word(asked: &str, named: &str) -> bool {
    asked == named
        || (asked.chars().count() >= 3 && named.starts_with(asked))
        || (asked.chars().count() >= 4 && distance(asked, named) <= 1)
}

/// Слова названия в виде, в котором «телеграм» и Telegram совпадают.
fn words(text: &str) -> Vec<String> {
    translit(&text.to_lowercase())
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| match number(word) {
            Some(digit) => digit.to_string(),
            None => skeleton(word),
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// Римские цифры и числительные — к цифре: «Mortal Shell II» и «мортал шелл
/// два» должны встретиться на «2».
fn number(word: &str) -> Option<&'static str> {
    match word {
        "i" | "odin" | "one" => Some("1"),
        "ii" | "dva" | "two" => Some("2"),
        "iii" | "tri" | "three" => Some("3"),
        "iv" | "chetire" | "four" => Some("4"),
        "v" | "piat" | "five" => Some("5"),
        _ => None,
    }
}

/// Кириллица в латиницу — так, как её произносят, а не как пишут в паспорте.
fn translit(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let piece = match ch {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' | 'ё' | 'э' => "e",
            'ж' => "zh",
            'з' => "z",
            'и' | 'й' | 'ы' => "i",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "h",
            'ц' => "ts",
            'ч' => "ch",
            'ш' | 'щ' => "sh",
            'ъ' | 'ь' => "",
            'ю' => "iu",
            'я' => "ia",
            other => {
                out.push(other);
                continue;
            }
        };
        out.push_str(piece);
    }
    out
}

/// Согласный остов слова.
///
/// Гласные — то, в чём распознавание и транслитерация расходятся чаще всего:
/// «клауде», «клод» и Claude отличаются именно ими. Согласные же совпадают:
/// у всех трёх остов «kld». Первая буква остаётся любой, иначе слова из одних
/// гласных исчезали бы целиком. Сдвоенные буквы схлопываются: «телеграмм».
fn skeleton(word: &str) -> String {
    let word = word
        .replace("ph", "f")
        .replace("th", "t")
        .replace("ck", "k")
        .replace("qu", "kv")
        .replace('q', "k")
        .replace('x', "ks")
        .replace('w', "v")
        .replace('y', "i")
        .replace('j', "dzh");

    // «c» читается «с» перед e и i, «к» — в остальных случаях; «ch» остаётся.
    let chars: Vec<char> = word.chars().collect();
    let mut spoken = String::with_capacity(word.len());
    for (at, &ch) in chars.iter().enumerate() {
        let next = chars.get(at + 1).copied();
        if ch == 'c' && next != Some('h') {
            spoken.push(if matches!(next, Some('e' | 'i')) { 's' } else { 'k' });
        } else {
            spoken.push(ch);
        }
    }

    let mut out = String::with_capacity(spoken.len());
    for (at, ch) in spoken.chars().enumerate() {
        if at > 0 && matches!(ch, 'a' | 'e' | 'i' | 'o' | 'u') {
            continue;
        }
        if out.ends_with(ch) {
            continue;
        }
        out.push(ch);
    }
    out
}

/// Расстояние Левенштейна: сколько букв поменять, чтобы из одного слова вышло другое.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = if ca == cb {
                previous
            } else {
                1 + previous.min(row[j]).min(row[j + 1])
            };
            previous = current;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(spoken: &str, name: &str) -> bool {
        score(spoken, name) >= MATCH_THRESHOLD
    }

    #[test]
    fn russian_speech_finds_latin_names() {
        assert!(heard("телеграм", "Telegram"));
        assert!(heard("телеграмм", "Telegram"));
        assert!(heard("стим", "Steam"));
        assert!(heard("клауде", "Claude"));
        assert!(heard("клод", "Claude"));
    }

    #[test]
    fn a_game_is_found_by_its_folder() {
        assert!(heard("мортал шелл", "Mortal Shell II (2026)"));
        // Римская цифра и числительное встречаются на одной цифре.
        assert!(heard("мортал шелл два", "Mortal Shell II (2026)"));
        assert!(heard("мортал шелл 2", "Mortal Shell II (2026)"));
    }

    #[test]
    fn similar_system_windows_are_told_apart() {
        assert!(heard("диспетчер устройств", "диспетчер устройств"));
        assert!(!heard("диспетчер устройств", "Диспетчер задач"));
    }

    #[test]
    fn the_closer_name_wins() {
        let entries = vec![
            Entry {
                name: "Steam Support Center".into(),
                target: Target::Shell(String::new()),
            },
            Entry {
                name: "Steam".into(),
                target: Target::Shell(String::new()),
            },
        ];
        assert_eq!(best_match("стим", &entries).map(|e| e.name.as_str()), Some("Steam"));
    }

    #[test]
    fn unrelated_names_do_not_match() {
        assert!(!heard("фотошоп", "Telegram"));
        assert!(!heard("блокнот", "Claude"));
        assert!(!heard("", "Telegram"));
    }

    #[test]
    fn power_words_are_understood() {
        assert_eq!(Power::parse("sleep"), Some(Power::Sleep));
        assert_eq!(Power::parse(" Shutdown "), Some(Power::Shutdown));
        assert_eq!(Power::parse("reboot"), Some(Power::Restart));
        assert_eq!(Power::parse("cancel"), Some(Power::Cancel));
        assert_eq!(Power::parse("launch"), None);
    }

    /// Что нашлось бы на этой машине — ничего не запуская.
    ///
    /// `cargo test pc::tests::what_would_open -- --ignored --nocapture`
    #[test]
    #[ignore = "читает меню «Пуск» и диски настоящей машины"]
    fn what_would_open() {
        let entries = catalog();
        println!("в каталоге: {}", entries.len());
        for spoken in [
            "телеграм",
            "клауде",
            "клод",
            "мортал шелл",
            "мортал шелл два",
            "диспетчер устройств",
            "диспетчер задач",
            "стим",
        ] {
            match best_match(spoken, &entries) {
                Some(entry) => {
                    let what = match &entry.target {
                        Target::Folder(dir) => format!("{:?}", main_exe(dir)),
                        Target::StartApp { id, path } => format!("{id} | {path:?}"),
                        Target::Shell(command) => command.clone(),
                    };
                    println!("{spoken:>22} → {} ({what})", entry.name);
                }
                None => println!("{spoken:>22} → не нашлось"),
            }
        }
        let mut running: Vec<String> = open_windows().into_iter().map(|w| w.exe).collect();
        running.sort();
        running.dedup();
        println!("программы с окнами: {}", running.join(", "));
        println!("sandboxie: {:?}", sandboxie());
    }

    #[test]
    fn start_menu_ids_become_paths_only_when_they_are_paths() {
        // У приложения из магазина пути нет — в песочнице его не запустить.
        assert_eq!(executable_of("Telegram.TelegramDesktop"), None);
        assert_eq!(executable_of("Claude_pzs8sxrjxfjjc!Claude"), None);
    }
}
