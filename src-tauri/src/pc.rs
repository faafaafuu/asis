//! Компьютер голосом: открыть программу, окно, раздел параметров или игру,
//! закрыть программу, листать страницу, включить или выключить VPN, усыпить,
//! выключить, перезагрузить или заблокировать машину.
//!
//! Главная трудность не в запуске — запускать умеет оболочка, — а в том, чтобы
//! понять, что назвали. Человек говорит «открой телеграм», распознавание пишет
//! «телеграм», а программа называется Telegram; «клауде» и «клод» — это Claude;
//! «мортал шелл два» — папка «Mortal Shell II (2026)»; «квинчат» — Qwen Chat.
//! Поэтому названия сравниваются не буквами, а звучанием: кириллица переводится
//! в латиницу, у слов остаётся согласный остов, а римские цифры и числительные
//! сводятся к цифрам. См. `score`.
//!
//! Искать есть где, и порядок поиска — это порядок доверия. Системные окна и
//! разделы параметров — по таблицам: у «диспетчера устройств» и «параметров
//! Bluetooth» нет ярлыков, только имена. Ярлыки меню «Пуск» и рабочего стола —
//! ими программу запускает и сам человек, и запуск по ярлыку ведёт себя так же:
//! уже открытый Telegram, свёрнутый в трей, по ярлыку показывает окно. Игры
//! Steam — по его манифестам, потому что запускать их надо через Steam, а не
//! файлом. Приложения из магазина — из списка «Пуска». И папки с играми на
//! дисках: игры, поставленные мимо установщика, больше нигде не видны.
//!
//! Список собирается заранее, в фоне. Меню «Пуск» читается через PowerShell —
//! это секунды, — и платить их, пока человек ждёт ответа на «открой блокнот»,
//! нельзя.
//!
//! Выключение и перезагрузка идут с минутной отсрочкой. Распознавание ошибается,
//! а выключенный посреди работы компьютер — это потерянная работа; минута даёт
//! сказать «отмени выключение». Сон и блокировка ничего не теряют и происходят
//! сразу, как только прозвучит ответ.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Готовит всё заранее: собирает список программ и начинает следить, какое
/// окно впереди. Зовётся один раз при запуске.
pub fn start() {
    std::thread::Builder::new()
        .name("sufler-catalog".into())
        .spawn(refresh)
        .ok();
    track_foreground();
}

/* ── Питание ────────────────────────────────────────────────────────────── */

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
            Ok(_) => "Отменил. Компьютер остаётся включённым.".into(),
            Err(_) => "Выключение и не было назначено.".into(),
        },
    }
}

fn schedule(flag: &str) -> Result<(), String> {
    run_hidden(
        "shutdown",
        &[flag, "/t", SHUTDOWN_DELAY_SECS, "/c", "Суфлёр: по голосовой команде"],
    )
    .map(|_| ())
}

fn after(delay: Duration, action: fn()) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        action();
    });
}

/// Запускает системную программу без окна консоли, ждёт и отдаёт её вывод.
fn run_hidden(program: &str, args: &[&str]) -> Result<String, String> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|err| format!("не запустился {program}: {err}"))?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if output.status.success() {
        Ok(text)
    } else {
        Err(format!(
            "{program} ответил кодом {}",
            output.status.code().unwrap_or(-1)
        ))
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

/* ── VPN ────────────────────────────────────────────────────────────────── */

/// Включает или выключает VPN и отдаёт ответ вслух.
///
/// VPN здесь — туннель AmneziaVPN или WireGuard, то есть служба Windows
/// `AmneziaWGTunnel$…` или `WireGuardTunnel$…`. Приложение при подключении
/// заводит такую службу с настройками внутри и дальше просто запускает и
/// останавливает её. Мы делаем то же самое.
///
/// Запуск и остановка этих служб разрешены только администратору — так их
/// ставит сам VPN, и обходить это нельзя. Поэтому Windows спросит
/// подтверждение: окно UAC — не поломка, а то, ради чего права и заведены.
pub fn vpn(on: bool) -> String {
    let Some(service) = vpn_service() else {
        // Службы нет — приложение сейчас её не держит. Тогда лучшее, что можно
        // сделать, — открыть само приложение, где включают одной кнопкой.
        return match best_match("amnezia vpn", &catalog()) {
            Some(entry) if on => {
                let _ = launch_entry(entry);
                "Туннеля сейчас нет — открыл AmneziaVPN, подключитесь кнопкой.".into()
            }
            _ if on => "Не нашёл VPN, который умею включать.".into(),
            _ => "VPN и так выключен.".into(),
        };
    };

    let running = service_running(&service);
    match (running, on) {
        (true, true) => return "VPN уже включён.".into(),
        (false, false) => return "VPN уже выключен.".into(),
        _ => {}
    }

    let verb = if on { "start" } else { "stop" };
    match elevated("sc.exe", &format!("{verb} \"{service}\"")) {
        Ok(()) if on => "Включаю VPN — подтвердите запрос Windows.".into(),
        Ok(()) => "Выключаю VPN — подтвердите запрос Windows.".into(),
        Err(err) => format!("Не вышло: {err}."),
    }
}

/// Служба туннеля, если она есть.
fn vpn_service() -> Option<String> {
    let listing = match run_hidden(
        &sc_exe(),
        &["query", "type=", "service", "state=", "all", "bufsize=", "262144"],
    ) {
        Ok(listing) => listing,
        Err(err) => {
            log::warn!("список служб не прочитался: {err}");
            return None;
        }
    };
    tunnel_in(&listing)
}

/// Имя службы туннеля в выводе `sc query`.
///
/// Вывод `sc` переведён на язык системы: на русской Windows вместо
/// `SERVICE_NAME:` стоит «ИМЯ_СЛУЖБЫ:», да ещё в кодировке консоли. Поэтому
/// ключ строки не читается вовсе — берётся значение после двоеточия, а служба
/// узнаётся по своему имени, которое не переводится.
fn tunnel_in(listing: &str) -> Option<String> {
    listing
        .lines()
        .filter_map(|line| line.split_once(':').map(|(_, value)| value.trim()))
        .find(|name| name.starts_with("AmneziaWGTunnel$") || name.starts_with("WireGuardTunnel$"))
        .map(str::to_string)
}

/// Запущена ли служба, по выводу `sc query <имя>`.
///
/// Состояние ищется по коду, а не по слову: `4` — работает. Слово рядом с
/// кодом тоже может оказаться переведённым. Код выхода вида «4 (0x4)» не
/// путается с состоянием: после кода состояния идёт слово, а не скобка.
fn running_in(state: &str) -> bool {
    state.lines().any(|line| {
        line.split_once(':')
            .and_then(|(_, value)| value.trim().strip_prefix('4'))
            .map(|rest| rest.trim_start().chars().next().is_some_and(char::is_alphabetic))
            .unwrap_or(false)
    })
}

/// sc.exe по полному пути.
///
/// По одному имени программа ищется там, куда смотрит PATH запустившего
/// процесса, а он у каждого свой; системная утилита лежит в одном месте всегда.
fn sc_exe() -> String {
    std::env::var("SystemRoot")
        .map(|root| format!("{root}\\System32\\sc.exe"))
        .unwrap_or_else(|_| "sc".into())
}

fn service_running(name: &str) -> bool {
    run_hidden(&sc_exe(), &["query", name])
        .map(|state| running_in(&state))
        .unwrap_or(false)
}

/// Запускает программу от имени администратора — с подтверждением UAC.
#[cfg(target_os = "windows")]
fn elevated(file: &str, parameters: &str) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    shell_execute_verb("runas", file, parameters, None, SW_HIDE.0).map_err(|code| {
        // 5 — человек нажал «Нет» в окне подтверждения.
        if code == 5 {
            "запрос Windows отклонён".to_string()
        } else {
            format!("оболочка отказала (код {code})")
        }
    })
}

#[cfg(not(target_os = "windows"))]
fn elevated(_file: &str, _parameters: &str) -> Result<(), String> {
    Err("умею только на Windows".into())
}

/* ── Листать ────────────────────────────────────────────────────────────── */

/// Что сделать со страницей в окне, где человек сейчас работает.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Down,
    Up,
    Top,
    Bottom,
    Back,
    Forward,
    CloseTab,
    Reload,
    /// Свернуть все окна — показать рабочий стол.
    Desktop,
}

impl Nav {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "down" | "scroll_down" => Some(Self::Down),
            "up" | "scroll_up" => Some(Self::Up),
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            "close_tab" | "closetab" => Some(Self::CloseTab),
            "reload" | "refresh" => Some(Self::Reload),
            "desktop" | "minimize_all" => Some(Self::Desktop),
            _ => None,
        }
    }
}

/// Окно чужой программы, которое было впереди последним.
///
/// Когда человек зовёт Ноа клавишей, вперёд выходит индикатор голоса, и
/// «листай вниз» надо отправить не ему, а браузеру, который был впереди до
/// этого. Поэтому переднее окно запоминается постоянно, а не в момент просьбы.
static LAST_EXTERNAL: AtomicIsize = AtomicIsize::new(0);

/// Листает, возвращается назад, закрывает вкладку — клавишами, как человек.
///
/// Клавиши, а не управление браузером изнутри: так это работает в любом
/// браузере и в любой программе с прокруткой, без расширений и настроек.
pub fn navigate(action: Nav) -> String {
    // Коды клавиш Windows.
    const CONTROL: u16 = 0x11;
    const ALT: u16 = 0x12;
    const PAGE_UP: u16 = 0x21;
    const PAGE_DOWN: u16 = 0x22;
    const END: u16 = 0x23;
    const HOME: u16 = 0x24;
    const LEFT: u16 = 0x25;
    const RIGHT: u16 = 0x27;
    const W: u16 = 0x57;
    const F5: u16 = 0x74;
    const WIN: u16 = 0x5B;
    const D: u16 = 0x44;

    let (keys, spoken): (&[u16], &str) = match action {
        Nav::Down => (&[PAGE_DOWN], "Листаю вниз."),
        Nav::Up => (&[PAGE_UP], "Листаю вверх."),
        Nav::Top => (&[HOME], "В начало."),
        Nav::Bottom => (&[END], "В конец."),
        Nav::Back => (&[ALT, LEFT], "Назад."),
        Nav::Forward => (&[ALT, RIGHT], "Вперёд."),
        Nav::CloseTab => (&[CONTROL, W], "Закрываю вкладку."),
        Nav::Reload => (&[F5], "Обновляю страницу."),
        Nav::Desktop => (&[WIN, D], "Сворачиваю все окна."),
    };

    let target = LAST_EXTERNAL.load(Ordering::Relaxed);
    if target == 0 {
        return "Не знаю, в каком окне это сделать.".into();
    }
    if !press(target, keys) {
        return "Не получилось нажать клавиши.".into();
    }
    spoken.into()
}

#[cfg(target_os = "windows")]
fn track_foreground() {
    std::thread::Builder::new()
        .name("sufler-foreground".into())
        .spawn(|| loop {
            if let Some(window) = external_foreground() {
                LAST_EXTERNAL.store(window, Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_millis(400));
        })
        .ok();
}

#[cfg(not(target_os = "windows"))]
fn track_foreground() {}

/// Переднее окно, если оно не наше.
#[cfg(target_os = "windows")]
fn external_foreground() -> Option<isize> {
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    // SAFETY: только чтение состояния системы.
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        (pid != 0 && pid != GetCurrentProcessId()).then_some(window.0 as isize)
    }
}

/// Выводит окно вперёд и нажимает в нём сочетание клавиш.
#[cfg(target_os = "windows")]
fn press(window: isize, keys: &[u16]) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let event = |vk: u16, up: bool| {
        // Стрелки и клавиши листания — «расширенные»: без флага часть программ
        // принимает их за клавиши цифрового блока.
        let mut flags = KEYBD_EVENT_FLAGS(0);
        if (0x21..=0x28).contains(&vk) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    };

    // Нажали по порядку, отпустили в обратном: Alt держится, пока жмут стрелку.
    let mut inputs: Vec<INPUT> = keys.iter().map(|&vk| event(vk, false)).collect();
    inputs.extend(keys.iter().rev().map(|&vk| event(vk, true)));

    // SAFETY: окно только выводится вперёд; события клавиатуры — массив,
    // живущий до конца вызова.
    unsafe {
        let _ = SetForegroundWindow(HWND(window as *mut core::ffi::c_void));
        std::thread::sleep(Duration::from_millis(150));
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        sent as usize == inputs.len()
    }
}

#[cfg(not(target_os = "windows"))]
fn press(_window: isize, _keys: &[u16]) -> bool {
    false
}

/* ── Что можно открыть ──────────────────────────────────────────────────── */

/// Окна и оснастки Windows, у которых нет ярлыка в меню «Пуск».
///
/// Имя слева — как это называет человек; справа — что понимает оболочка.
/// Несколько имён на одно окно, потому что зовут его по-разному.
const SYSTEM: &[(&str, &str)] = &[
    ("диспетчер устройств", "devmgmt.msc"),
    ("диспетчер задач", "taskmgr.exe"),
    ("управление дисками", "diskmgmt.msc"),
    ("управление компьютером", "compmgmt.msc"),
    ("службы", "services.msc"),
    ("просмотр событий", "eventvwr.msc"),
    ("монитор ресурсов", "resmon.exe"),
    ("редактор реестра", "regedit.exe"),
    ("панель управления", "control.exe"),
    ("программы и компоненты", "appwiz.cpl"),
    ("сетевые подключения", "ncpa.cpl"),
    ("панель звука", "mmsys.cpl"),
    ("проводник", "explorer.exe"),
    ("командная строка", "cmd.exe"),
    ("терминал", "wt.exe"),
    ("блокнот", "notepad.exe"),
    ("калькулятор", "calc.exe"),
    ("загрузки", "shell:Downloads"),
    ("документы", "shell:Personal"),
    ("корзина", "shell:RecycleBinFolder"),
];

/// Разделы «Параметров» Windows.
///
/// Раздел открывается адресом `ms-settings:…` — прямо на нужной странице, а не
/// на главной, откуда до неё ещё три щелчка.
const SETTINGS: &[(&str, &str)] = &[
    ("параметры", "ms-settings:"),
    ("bluetooth", "ms-settings:bluetooth"),
    ("блютуз", "ms-settings:bluetooth"),
    ("bluetooth и устройства", "ms-settings:bluetooth"),
    ("wi-fi", "ms-settings:network-wifi"),
    ("вай фай", "ms-settings:network-wifi"),
    ("сеть и интернет", "ms-settings:network"),
    ("vpn", "ms-settings:network-vpn"),
    ("прокси", "ms-settings:network-proxy"),
    ("мобильная точка доступа", "ms-settings:network-mobilehotspot"),
    ("режим в самолёте", "ms-settings:network-airplanemode"),
    ("экран", "ms-settings:display"),
    ("дисплей", "ms-settings:display"),
    ("ночной свет", "ms-settings:nightlight"),
    ("звук", "ms-settings:sound"),
    ("уведомления", "ms-settings:notifications"),
    ("не беспокоить", "ms-settings:quiethours"),
    ("электропитание", "ms-settings:powersleep"),
    ("питание и батарея", "ms-settings:batterysaver"),
    ("память", "ms-settings:storagesense"),
    ("хранилище", "ms-settings:storagesense"),
    ("буфер обмена", "ms-settings:clipboard"),
    ("о системе", "ms-settings:about"),
    ("восстановление", "ms-settings:recovery"),
    ("активация", "ms-settings:activation"),
    ("установка и удаление программ", "ms-settings:appsfeatures"),
    ("установленные приложения", "ms-settings:appsfeatures"),
    ("приложения по умолчанию", "ms-settings:defaultapps"),
    ("автозагрузка", "ms-settings:startupapps"),
    ("персонализация", "ms-settings:personalization"),
    ("обои", "ms-settings:personalization-background"),
    ("фон рабочего стола", "ms-settings:personalization-background"),
    ("темы", "ms-settings:themes"),
    ("цвета", "ms-settings:colors"),
    ("тёмная тема", "ms-settings:colors"),
    ("экран блокировки", "ms-settings:lockscreen"),
    ("панель задач", "ms-settings:taskbar"),
    ("мышь", "ms-settings:mousetouchpad"),
    ("тачпад", "ms-settings:devices-touchpad"),
    ("принтеры", "ms-settings:printers"),
    ("клавиатура", "ms-settings:typing"),
    ("язык и раскладка", "ms-settings:regionlanguage"),
    ("дата и время", "ms-settings:dateandtime"),
    ("учётные записи", "ms-settings:accounts"),
    ("параметры входа", "ms-settings:signinoptions"),
    ("игровой режим", "ms-settings:gaming-gamemode"),
    ("специальные возможности", "ms-settings:easeofaccess"),
    ("конфиденциальность", "ms-settings:privacy"),
    ("микрофон", "ms-settings:privacy-microphone"),
    ("камера", "ms-settings:privacy-webcam"),
    ("местоположение", "ms-settings:privacy-location"),
    ("обновления windows", "ms-settings:windowsupdate"),
    ("центр обновления", "ms-settings:windowsupdate"),
    ("безопасность windows", "windowsdefender:"),
    ("защитник", "windowsdefender:"),
    ("проецирование", "ms-settings:project"),
];

#[derive(Clone, Debug)]
struct Entry {
    /// Как называется — это же имя звучит в ответе.
    name: String,
    target: Target,
}

#[derive(Clone, Debug)]
enum Target {
    /// Имя, которое оболочка найдёт сама: оснастка, программа, раздел, адрес.
    Shell(String),
    /// Ярлык из меню «Пуск» или с рабочего стола.
    Shortcut(PathBuf),
    /// Игра Steam: запускается через сам Steam, папка — для песочницы.
    Steam { app_id: String, dir: PathBuf },
    /// Приложение из меню «Пуск» без ярлыка. `path` — исполняемый файл, если
    /// его удалось вывести из идентификатора: без него не запустить в песочнице.
    StartApp { id: String, path: Option<PathBuf> },
    /// Папка программы на диске; что в ней запускать, решается при запуске.
    Folder(PathBuf),
}

/// Сколько держим собранный список, прежде чем пересобрать его в фоне.
const CATALOG_TTL: Duration = Duration::from_secs(10 * 60);

static CATALOG: Mutex<Option<(Instant, Vec<Entry>)>> = Mutex::new(None);
static REFRESHING: AtomicBool = AtomicBool::new(false);

/// Список того, что можно открыть.
///
/// Никогда не ждёт PowerShell. Устаревший список отдаётся как есть, а
/// пересобирается в фоне. Если списка ещё нет вовсе — только что запустились —
/// собирается быстрая часть без «Пуска» из PowerShell: таблицы, ярлыки, Steam
/// и папки читаются с диска за доли секунды.
fn catalog() -> Vec<Entry> {
    let cached = CATALOG.lock().unwrap_or_else(|err| err.into_inner()).clone();
    match cached {
        Some((at, entries)) => {
            if at.elapsed() > CATALOG_TTL {
                std::thread::spawn(refresh);
            }
            entries
        }
        None => {
            std::thread::spawn(refresh);
            build(false)
        }
    }
}

/// Пересобирает список целиком. Два потока разом его не собирают.
fn refresh() {
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let started = Instant::now();
    let entries = build(true);
    log::info!(
        "список программ собран: {} за {} мс",
        entries.len(),
        started.elapsed().as_millis()
    );
    *CATALOG.lock().unwrap_or_else(|err| err.into_inner()) = Some((Instant::now(), entries));
    REFRESHING.store(false, Ordering::SeqCst);
}

fn build(with_start_apps: bool) -> Vec<Entry> {
    let table = |rows: &[(&str, &str)]| -> Vec<Entry> {
        rows.iter()
            .map(|(name, command)| Entry {
                name: (*name).to_string(),
                target: Target::Shell((*command).to_string()),
            })
            .collect()
    };

    let mut entries = table(SYSTEM);
    entries.extend(table(SETTINGS));
    entries.extend(shortcuts());
    entries.extend(steam_games());
    if with_start_apps {
        // Из «Пуска» берём только то, чего нет среди ярлыков: у приложений из
        // магазина ярлыков нет, а обычные программы уже нашлись надёжнее.
        let known: std::collections::HashSet<String> =
            entries.iter().map(|entry| entry.name.to_lowercase()).collect();
        entries.extend(
            start_apps()
                .into_iter()
                .filter(|entry| !known.contains(&entry.name.to_lowercase())),
        );
    }
    entries.extend(program_folders());
    entries
}

/// Слова в названиях ярлыков, по которым их запускать нельзя.
///
/// Деинсталлятор назван так же, как программа, — «Деинсталлировать
/// Telegram», — и по «закрой»/«открой телеграм» мог бы оказаться первым.
/// Запустить удаление программы голосом по ошибке распознавания недопустимо.
const NOT_LAUNCHERS: &[&str] = &[
    "unins", "uninstall", "деинстал", "удалить", "удаление", "remove", "readme", "help",
    "справка", "manual", "руководство", "license", "лиценз", "website", "веб-сайт",
];

fn is_launcher(name: &str) -> bool {
    let name = name.to_lowercase();
    !NOT_LAUNCHERS.iter().any(|word| name.contains(word))
}

/// Ярлыки меню «Пуск» и рабочих столов.
fn shortcuts() -> Vec<Entry> {
    let env = |name: &str| std::env::var(name).ok().map(PathBuf::from);
    let start_menu = Path::new("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = env("ProgramData") {
        roots.push(dir.join(&start_menu));
    }
    if let Some(dir) = env("APPDATA") {
        roots.push(dir.join(&start_menu));
    }
    if let Some(home) = env("USERPROFILE") {
        roots.push(home.join("Desktop"));
        roots.push(home.join("OneDrive").join("Desktop"));
        roots.push(home.join("OneDrive").join("Рабочий стол"));
    }
    if let Some(public) = env("PUBLIC") {
        roots.push(public.join("Desktop"));
    }

    let mut found = Vec::new();
    for root in roots {
        collect_shortcuts(&root, 0, &mut found);
    }
    found
}

fn collect_shortcuts(dir: &Path, depth: usize, found: &mut Vec<Entry>) {
    if depth > 4 {
        return;
    }
    let Ok(children) = std::fs::read_dir(dir) else { return };
    for child in children.flatten() {
        let path = child.path();
        if path.is_dir() {
            collect_shortcuts(&path, depth + 1, found);
            continue;
        }
        let is_shortcut = path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("lnk") || ext.eq_ignore_ascii_case("url"))
            .unwrap_or(false);
        let Some(name) = path.file_stem().map(|stem| stem.to_string_lossy().to_string()) else {
            continue;
        };
        if is_shortcut && is_launcher(&name) {
            found.push(Entry {
                name,
                target: Target::Shortcut(path),
            });
        }
    }
}

/// Игры Steam — по его манифестам во всех библиотеках.
fn steam_games() -> Vec<Entry> {
    let Some(steam) = std::env::var("ProgramFiles(x86)")
        .ok()
        .map(|base| Path::new(&base).join("Steam"))
    else {
        return Vec::new();
    };

    // Библиотеки перечислены в libraryfolders.vdf; основная — сам Steam.
    let mut libraries = vec![steam.clone()];
    if let Ok(vdf) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        libraries.extend(
            vdf_values(&vdf, "path")
                .into_iter()
                .map(|path| PathBuf::from(path.replace("\\\\", "\\"))),
        );
    }
    libraries.sort();
    libraries.dedup();

    let mut games = Vec::new();
    for library in libraries {
        let apps = library.join("steamapps");
        let Ok(children) = std::fs::read_dir(&apps) else { continue };
        for child in children.flatten() {
            let file = child.file_name().to_string_lossy().to_string();
            let Some(app_id) = file
                .strip_prefix("appmanifest_")
                .and_then(|rest| rest.strip_suffix(".acf"))
            else {
                continue;
            };
            let Ok(manifest) = std::fs::read_to_string(child.path()) else { continue };
            let name = vdf_values(&manifest, "name").into_iter().next().unwrap_or_default();
            let dir = vdf_values(&manifest, "installdir").into_iter().next().unwrap_or_default();
            // Служебные пакеты Steam — не игры, открывать их незачем.
            let service = ["Redistributable", "Steamworks", "Proton", "Runtime", "SDK"]
                .iter()
                .any(|word| name.contains(word));
            if name.is_empty() || service {
                continue;
            }
            games.push(Entry {
                name,
                target: Target::Steam {
                    app_id: app_id.to_string(),
                    dir: apps.join("common").join(dir),
                },
            });
        }
    }
    games
}

/// Значения ключа из текстового формата Valve: строки вида `"key"  "value"`.
fn vdf_values(text: &str, key: &str) -> Vec<String> {
    let quoted = format!("\"{key}\"");
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix(&quoted)?;
            let value = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
            Some(value.to_string())
        })
        .collect()
}

/// Приложения из меню «Пуск» — тем же списком, что видит сам человек.
///
/// `Get-StartApps` знает и приложения из магазина, у которых ярлыков нет.
#[cfg(target_os = "windows")]
fn start_apps() -> Vec<Entry> {
    // Кодировку вывода задаём явно: в канал PowerShell пишет в кодовой
    // странице консоли, и русские названия приходили бы кракозябрами.
    let raw = match run_hidden(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress",
        ],
    ) {
        Ok(raw) => raw,
        Err(err) => {
            log::warn!("меню «Пуск» не прочиталось: {err}");
            return Vec::new();
        }
    };
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
            if name.is_empty() || id.starts_with("http") || !is_launcher(&name) {
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

/// Несъёмные диски.
///
/// Только они: обращение к отключённому сетевому диску или пустому кардридеру
/// висит секундами, а список собирается, пока человек, может быть, ждёт.
#[cfg(target_os = "windows")]
fn fixed_drives() -> Vec<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    const DRIVE_FIXED: u32 = 3;

    // SAFETY: обе функции только читают список дисков.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|bit| mask & (1 << bit) != 0)
        .filter_map(|bit| {
            let root = format!("{}:\\", (b'A' + bit) as char);
            let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            let kind = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
            (kind == DRIVE_FIXED).then(|| PathBuf::from(root))
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn fixed_drives() -> Vec<PathBuf> {
    Vec::new()
}

/// Папки программ и игр на дисках.
///
/// Смотрим туда, куда игры ставят обычно: корни несистемных дисков, папки
/// Games, библиотеки Steam и Epic. Системный диск целиком не обходим — в его
/// корне и так только системные папки, а игры на нём живут в перечисленных.
fn program_folders() -> Vec<Entry> {
    let system = std::env::var("SystemDrive")
        .map(|drive| format!("{drive}\\").to_uppercase())
        .unwrap_or_else(|_| "C:\\".into());

    let mut roots: Vec<PathBuf> = Vec::new();
    for drive in fixed_drives() {
        if drive.to_string_lossy().to_uppercase() != system {
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
            if name.starts_with('$') || name.starts_with('.') || is_system_folder(&name) {
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

/// Системная папка в корне диска.
///
/// Они никогда не то, что просят открыть, а звучат как обычные слова:
/// «программа сейчас запущена» открывала папку ProgramData.
fn is_system_folder(name: &str) -> bool {
    const SYSTEM_FOLDERS: &[&str] = &[
        "programdata",
        "program files",
        "program files (x86)",
        "windows",
        "windows.old",
        "users",
        "perflogs",
        "recovery",
        "system volume information",
        "intel",
        "amd",
        "nvidia",
        "drivers",
        "msocache",
        "config.msi",
        "onedrivetemp",
        "boot",
        "documents and settings",
    ];
    SYSTEM_FOLDERS.contains(&name.to_lowercase().as_str())
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
    let mut best: Option<(&Entry, f32, f32)> = None;
    for entry in entries {
        let value = score(spoken, &entry.name);
        if value < MATCH_THRESHOLD {
            continue;
        }
        // При равной оценке решает близость с гласными: у «стим» и «о системе»
        // согласный остов один — «stm», — но «стим» куда ближе к Steam. Если
        // равны и по ней, остаётся первое, а таблицы стоят первыми: «проводник»
        // это окно, а не папка с таким именем.
        let near = closeness(spoken, &entry.name);
        let better = best.map_or(true, |(_, top, top_near)| {
            value > top + f32::EPSILON || ((value - top).abs() <= f32::EPSILON && near > top_near)
        });
        if better {
            best = Some((entry, value, near));
        }
    }
    best.map(|(entry, _, _)| entry)
}

/// Близость по полному звучанию, с гласными: от 0 до 1.
///
/// Нужна только чтобы развести названия, равные по согласному остову.
fn closeness(spoken: &str, name: &str) -> f32 {
    let asked = asked_words_raw(spoken).concat();
    let named = raw_words(name).concat();
    let longest = asked.chars().count().max(named.chars().count()).max(1);
    1.0 - distance(&asked, &named) as f32 / longest as f32
}

/// Открывает программу, окно, раздел параметров или игру по названию и
/// отдаёт ответ вслух.
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

    match launch_entry(entry) {
        Ok(()) => format!("Открываю {}.", entry.name),
        Err(err) => format!("Открыть {} не вышло: {err}.", entry.name),
    }
}

fn launch_entry(entry: &Entry) -> Result<(), String> {
    match &entry.target {
        Target::Shell(command) => open(command),
        Target::Shortcut(path) => shell_execute(&path.to_string_lossy(), "", None),
        Target::Steam { app_id, .. } => open(&format!("steam://rungameid/{app_id}")),
        // Идентификатор-адрес (steam://…) открывается сам, остальные — через
        // папку приложений оболочки, одинаково для программ и магазина.
        Target::StartApp { id, .. } if id.contains("://") => open(id),
        Target::StartApp { id, .. } => open(&format!("shell:AppsFolder\\{id}")),
        Target::Folder(dir) => match main_exe(dir) {
            Some(exe) => open_program(&exe, ""),
            None => Err("в папке нет запускаемого файла".into()),
        },
    }
}

fn launch_sandboxed(entry: &Entry, sandbox_box: &str) -> String {
    let Some(start) = sandboxie() else {
        return "Sandboxie не установлена — запускать в песочнице нечем.".into();
    };

    let program = match &entry.target {
        Target::Folder(dir) | Target::Steam { dir, .. } => main_exe(dir),
        Target::Shortcut(path) => Some(path.clone()),
        Target::StartApp { path, .. } => path.clone(),
        Target::Shell(command) if command.ends_with(".exe") => Some(PathBuf::from(command)),
        Target::Shell(_) => None,
    };
    let Some(program) = program else {
        // Приложения из магазина и разделы параметров Sandboxie не запускает:
        // ей нужен файл программы, а у них его нет в привычном смысле.
        return format!("{} в песочнице не запустить: у неё нет файла программы.", entry.name);
    };

    // Имя песочницы уходит в командную строку — пропускаем только то, из чего
    // имена песочниц и состоят, чтобы кавычка не превратилась в параметр.
    let name: String = sandbox_box
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    let name = if name.is_empty() { "DefaultBox".to_string() } else { name };

    match open_program(&start, &format!("/box:{name} \"{}\"", program.display())) {
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
/// Сначала — как крестиком в углу окна: программа получает просьбу закрыться
/// и может спросить про несохранённое. Снимать силой программу с открытыми
/// окнами нельзя — это потеря того, что человек не успел сохранить.
///
/// Но у программы, свёрнутой в трей, — Telegram, Яндекс Музыки — видимых окон
/// нет вовсе, а значит, нет и крестика. Раньше на неё отвечалось «такого окна
/// нет», хотя значок висел в трее. Для такой программы «закрой» значит
/// завершить процесс: открытых окон с несохранённым у неё нет.
pub fn close(spoken: &str) -> String {
    let windows = open_windows();

    // Лучшее окно — по заголовку или по имени программы: «закрой телеграм»
    // совпадает с Telegram.exe, «закрой диспетчер задач» — только с
    // заголовком окна.
    let mut best: Option<(usize, f32, bool)> = None;
    for (at, window) in windows.iter().enumerate() {
        let by_exe = score(spoken, &window.exe);
        let by_title = score(spoken, &window.title);
        let value = by_exe.max(by_title);
        if value >= MATCH_THRESHOLD && best.map_or(true, |(_, top, _)| value > top) {
            best = Some((at, value, by_exe >= by_title));
        }
    }

    if let Some((at, _, by_exe)) = best {
        let chosen = &windows[at];
        // Общий процесс-хозяин держит окна разных программ: ApplicationFrameHost —
        // все приложения из магазина. Закрыть «его» значит закрыть их все, и
        // «закрой Яндекс Музыку» закрывало заодно чужие окна. У хозяина
        // закрываются только окна с совпавшим заголовком.
        let shared = SHARED_HOSTS
            .iter()
            .any(|host| chosen.exe.eq_ignore_ascii_case(host));
        let targets: Vec<&OpenWindow> = windows
            .iter()
            .filter(|window| window.exe == chosen.exe)
            .filter(|window| {
                (by_exe && !shared) || score(spoken, &window.title) >= MATCH_THRESHOLD
            })
            .collect();
        let name = if shared {
            chosen.title.clone()
        } else {
            readable(&chosen.exe)
        };
        let closed = targets
            .iter()
            .filter(|window| close_window(window.handle))
            .count();
        log::info!("закрываю {name}: окон {closed}");
        return match closed {
            0 => format!("{name} не закрылся."),
            _ => format!("Закрываю {name}."),
        };
    }

    let Some((name, pids)) = running_process(spoken) else {
        return format!("Не нашёл открытой программы «{spoken}».");
    };
    let ended = pids.iter().filter(|pid| terminate(**pid)).count();
    log::info!("{name} без окон — завершаю процессов: {ended} из {}", pids.len());
    match ended {
        0 => format!("{name} не закрылся — Windows не дала его завершить."),
        _ => format!("Закрываю {name}."),
    }
}

/// Процессы, которые держат окна чужих программ.
const SHARED_HOSTS: &[&str] = &["ApplicationFrameHost"];

/// Процессы, которые нельзя завершать ни по какой просьбе: без них не
/// работает сама Windows, а распознавание может ослышаться.
const NEVER_END: &[&str] = &[
    "explorer",
    "csrss",
    "winlogon",
    "wininit",
    "lsass",
    "services",
    "svchost",
    "smss",
    "dwm",
    "system",
    "registry",
    "sihost",
    "ctfmon",
    "fontdrvhost",
    "conhost",
    "audiodg",
    "spoolsv",
    "runtimebroker",
    "searchhost",
    "startmenuexperiencehost",
    "shellexperiencehost",
    "textinputhost",
    "lockapp",
    "msmpeng",
    "securityhealthservice",
    "applicationframehost",
    "sufler",
];

/// Имя программы, как его говорят: WindowsTerminal — «Windows Terminal».
fn readable(exe: &str) -> String {
    split_camel(exe).replace(['_', '-'], " ")
}

/// Запущенная программа без видимых окон: её имя и все её процессы.
fn running_process(spoken: &str) -> Option<(String, Vec<u32>)> {
    let own = std::process::id();
    let all = processes();
    let stem_of = |exe: &str| {
        let lower = exe.to_lowercase();
        lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
    };

    let mut best: Option<(String, f32)> = None;
    for (pid, exe) in &all {
        let stem = stem_of(exe);
        if *pid == own || NEVER_END.contains(&stem.as_str()) {
            continue;
        }
        let shown = exe.strip_suffix(".exe").unwrap_or(exe);
        let value = score(spoken, shown);
        if value >= MATCH_THRESHOLD && best.as_ref().map_or(true, |(_, top)| value > *top) {
            best = Some((shown.to_string(), value));
        }
    }

    let (name, _) = best?;
    let wanted = stem_of(&name);
    let pids = all
        .iter()
        .filter(|(pid, exe)| *pid != own && stem_of(exe) == wanted)
        .map(|(pid, _)| *pid)
        .collect();
    Some((readable(&name), pids))
}

/// Все процессы: номер и имя исполняемого файла.
#[cfg(target_os = "windows")]
fn processes() -> Vec<(u32, String)> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut found = Vec::new();
    // SAFETY: снимок только читается; структура с верным размером живёт до
    // конца обхода, дескриптор снимка закрывается в конце.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return found;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let length = entry
                    .szExeFile
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(entry.szExeFile.len());
                found.push((
                    entry.th32ProcessID,
                    String::from_utf16_lossy(&entry.szExeFile[..length]),
                ));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    found
}

#[cfg(not(target_os = "windows"))]
fn processes() -> Vec<(u32, String)> {
    Vec::new()
}

/// Завершает процесс.
#[cfg(target_os = "windows")]
fn terminate(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    // SAFETY: дескриптор открывается ровно на завершение и сразу закрывается.
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_TERMINATE, false, pid) else {
            return false;
        };
        let done = TerminateProcess(process, 0).is_ok();
        let _ = CloseHandle(process);
        done
    }
}

#[cfg(not(target_os = "windows"))]
fn terminate(_pid: u32) -> bool {
    false
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
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    shell_execute_verb("open", file, parameters, directory, SW_SHOWNORMAL.0)
        .map_err(|code| format!("оболочка отказала (код {code})"))
}

/// ShellExecuteW с глаголом. Ошибка — код оболочки (не больше 32).
#[cfg(target_os = "windows")]
fn shell_execute_verb(
    verb: &str,
    file: &str,
    parameters: &str,
    directory: Option<&Path>,
    show: i32,
) -> Result<(), isize> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD;

    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let verb_w = wide(verb);
    let file_w = wide(file);
    let parameters_w = wide(parameters);
    let directory_w = directory.map(|dir| wide(&dir.to_string_lossy()));

    // SAFETY: все строки живут до конца вызова и оканчиваются нулём.
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(file_w.as_ptr()),
            if parameters.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(parameters_w.as_ptr())
            },
            directory_w
                .as_ref()
                .map_or(PCWSTR::null(), |dir| PCWSTR(dir.as_ptr())),
            SHOW_WINDOW_CMD(show),
        )
    };

    // Больше 32 — успех; меньше — код ошибки оболочки.
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(code)
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
/// «стим» выбирал Steam, а не Steam Support Center. Отдельно сравнивается
/// название, сказанное слитно: «квинчат» — это Qwen Chat, два слова на экране
/// и одно в речи.
pub(crate) fn score(spoken: &str, name: &str) -> f32 {
    let asked = asked_words(spoken);
    let named = words(name);
    if asked.is_empty() || named.is_empty() {
        return 0.0;
    }

    let matched = asked
        .iter()
        .filter(|word| named.iter().any(|other| same_word(word, other)))
        .count();
    let extra = named.len().saturating_sub(matched).min(4) as f32;
    let by_words = matched as f32 / asked.len() as f32 - 0.05 * extra;

    let joined_asked = skeleton(&asked_words_raw(spoken).concat());
    let joined_named = skeleton(&raw_words(name).concat());
    let by_whole = if joined_asked.chars().count() >= 3 && joined_asked == joined_named {
        1.0
    } else {
        0.0
    };

    by_words.max(by_whole)
}

fn same_word(asked: &str, named: &str) -> bool {
    asked == named
        || (asked.chars().count() >= 3 && named.starts_with(asked))
        || (named.chars().count() >= 3 && asked.starts_with(named))
        || (asked.chars().count() >= 4 && distance(asked, named) <= 1)
}

/// Слова-обёртки из просьбы: «открой параметры Bluetooth» — это Bluetooth, а
/// не «параметры»; «закрой программу Telegram» — это Telegram.
const FILLER: &[&str] = &[
    "parametri", "parametr", "nastroiki", "nastroika", "nastroek", "okno", "okna", "programma",
    "programmu", "programmi", "prilozhenie", "prilozhenia", "menu", "razdel", "stranitsu",
    "windows", "vindovs", "igru", "igra", "sait",
];

/// Слова просьбы без обёрток. Если без них не остаётся ничего — «открой
/// параметры» — обёртка и есть то, что просят.
fn asked_words(spoken: &str) -> Vec<String> {
    to_sounds(asked_words_raw(spoken))
}

fn asked_words_raw(spoken: &str) -> Vec<String> {
    let all = raw_words(spoken);
    let meaningful: Vec<String> = all
        .iter()
        .filter(|word| !FILLER.contains(&word.as_str()))
        .cloned()
        .collect();
    if meaningful.is_empty() {
        all
    } else {
        meaningful
    }
}

/// Слова названия в виде, в котором «телеграм» и Telegram совпадают.
fn words(text: &str) -> Vec<String> {
    to_sounds(raw_words(text))
}

/// Слова в латинице, без однобуквенных: «и», «в», «с» в названиях не
/// различают ничего, а в числа превращались бы по ошибке.
fn raw_words(text: &str) -> Vec<String> {
    translit(&split_camel(text).to_lowercase())
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| word.len() > 1 || word.chars().all(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
        .collect()
}

/// Разрезает слитные имена по заглавным буквам: WindowsTerminal — «Windows
/// Terminal», ProgramData — «Program Data». Иначе «терминал» не узнавался бы в
/// имени процесса, записанном одним словом.
fn split_camel(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    let mut previous: Option<char> = None;
    for ch in text.chars() {
        if ch.is_uppercase() && previous.is_some_and(char::is_lowercase) {
            out.push(' ');
        }
        out.push(ch);
        previous = Some(ch);
    }
    out
}

fn to_sounds(raw: Vec<String>) -> Vec<String> {
    raw.iter()
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
        "odin" | "one" => Some("1"),
        "ii" | "dva" | "two" => Some("2"),
        "iii" | "tri" | "three" => Some("3"),
        "iv" | "chetire" | "four" => Some("4"),
        "piat" | "five" => Some("5"),
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

    fn pick<'a>(spoken: &str, names: &[&'a str]) -> Option<&'a str> {
        let entries: Vec<Entry> = names
            .iter()
            .map(|name| Entry {
                name: (*name).to_string(),
                target: Target::Shell(String::new()),
            })
            .collect();
        let chosen = best_match(spoken, &entries)?.name.clone();
        names.iter().copied().find(|name| *name == chosen)
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
    fn a_name_said_in_one_word_is_found() {
        // Qwen Chat на экране — два слова, в речи — одно.
        assert!(heard("квинчат", "Qwen Chat"));
        assert!(heard("квин", "Qwen"));
    }

    #[test]
    fn a_game_is_found_by_its_folder() {
        assert!(heard("мортал шелл", "Mortal Shell II (2026)"));
        // Римская цифра и числительное встречаются на одной цифре.
        assert!(heard("мортал шелл два", "Mortal Shell II (2026)"));
        assert!(heard("мортал шелл 2", "Mortal Shell II (2026)"));
        assert!(heard("киберпанк", "Cyberpunk 2077"));
    }

    #[test]
    fn similar_system_windows_are_told_apart() {
        assert!(heard("диспетчер устройств", "диспетчер устройств"));
        assert!(!heard("диспетчер устройств", "Диспетчер задач"));
    }

    #[test]
    fn settings_pages_are_found_through_the_wrapping_words() {
        let names = ["параметры", "bluetooth", "звук", "установка и удаление программ"];
        assert_eq!(pick("параметры bluetooth", &names), Some("bluetooth"));
        assert_eq!(pick("настройки блютуз", &names), Some("bluetooth"));
        assert_eq!(pick("параметры", &names), Some("параметры"));
        assert_eq!(
            pick("установка и удаление программ", &names),
            Some("установка и удаление программ")
        );
    }

    #[test]
    fn little_words_are_not_numbers() {
        // «и» и «в» — не римская единица и не пятёрка.
        assert!(!words("установка и удаление").contains(&"1".to_string()));
        assert!(!words("запусти в песочнице").contains(&"5".to_string()));
    }

    #[test]
    fn a_translated_service_listing_is_read() {
        // Так `sc query` отвечает на русской Windows.
        let listing = "ИМЯ_СЛУЖБЫ: AmneziaVPN-service\n\
                       ВЫВОДИМОЕ_ИМЯ: AmneziaVPN-service\n\
                       \x20       СОСТОЯНИЕ          : 4  RUNNING\n\
                       ИМЯ_СЛУЖБЫ: AmneziaWGTunnel$AmneziaVPN\n\
                       ВЫВОДИМОЕ_ИМЯ: Amnezia VPN (tunnel)\n";
        assert_eq!(tunnel_in(listing).as_deref(), Some("AmneziaWGTunnel$AmneziaVPN"));
        assert_eq!(tunnel_in("SERVICE_NAME: Spooler\n"), None);
    }

    #[test]
    fn service_state_is_read_by_code() {
        assert!(running_in("        СОСТОЯНИЕ          : 4  RUNNING\n"));
        assert!(running_in("        STATE              : 4  ВЫПОЛНЯЕТСЯ\n"));
        // Остановлена, а код выхода 4 — это не «работает».
        assert!(!running_in(
            "        СОСТОЯНИЕ          : 1  STOPPED\n        КОД_ВЫХОДА_WIN32   : 4  (0x4)\n"
        ));
    }

    #[test]
    fn a_tie_goes_to_the_closer_sound() {
        // Согласный остов у обоих «stm».
        assert_eq!(pick("стим", &["о системе", "Steam"]), Some("Steam"));
    }

    #[test]
    fn the_closer_name_wins() {
        assert_eq!(pick("стим", &["Steam Support Center", "Steam"]), Some("Steam"));
    }

    #[test]
    fn names_written_together_are_split() {
        assert!(heard("терминал", "WindowsTerminal"));
        assert_eq!(readable("WindowsTerminal"), "Windows Terminal");
    }

    #[test]
    fn system_folders_are_not_programs() {
        assert!(is_system_folder("ProgramData"));
        assert!(is_system_folder("Program Files (x86)"));
        assert!(!is_system_folder("Mortal Shell II (2026)"));
    }

    #[test]
    fn the_shell_and_itself_are_never_ended() {
        assert!(NEVER_END.contains(&"explorer"));
        assert!(NEVER_END.contains(&"sufler"));
        assert_eq!(Nav::parse("desktop"), Some(Nav::Desktop));
    }

    #[test]
    fn uninstallers_are_never_launchers() {
        assert!(!is_launcher("Деинсталлировать Telegram"));
        assert!(!is_launcher("Uninstall Steam"));
        assert!(is_launcher("Telegram"));
    }

    #[test]
    fn unrelated_names_do_not_match() {
        assert!(!heard("фотошоп", "Telegram"));
        assert!(!heard("блокнот", "Claude"));
        assert!(!heard("", "Telegram"));
    }

    #[test]
    fn valve_manifests_are_read() {
        let manifest = "\"AppState\"\n{\n\t\"appid\"\t\t\"1091500\"\n\t\"name\"\t\t\"Cyberpunk 2077\"\n\t\"installdir\"\t\t\"Cyberpunk 2077\"\n}";
        assert_eq!(vdf_values(manifest, "name"), vec!["Cyberpunk 2077".to_string()]);
        assert_eq!(vdf_values(manifest, "installdir"), vec!["Cyberpunk 2077".to_string()]);
        let folders = "\"0\"\n{\n\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n}";
        assert_eq!(vdf_values(folders, "path"), vec!["D:\\\\SteamLibrary".to_string()]);
    }

    #[test]
    fn spoken_actions_are_understood() {
        assert_eq!(Power::parse("sleep"), Some(Power::Sleep));
        assert_eq!(Power::parse(" Shutdown "), Some(Power::Shutdown));
        assert_eq!(Power::parse("reboot"), Some(Power::Restart));
        assert_eq!(Power::parse("cancel"), Some(Power::Cancel));
        assert_eq!(Power::parse("launch"), None);
        assert_eq!(Nav::parse("down"), Some(Nav::Down));
        assert_eq!(Nav::parse("close_tab"), Some(Nav::CloseTab));
        assert_eq!(Nav::parse("dance"), None);
    }

    #[test]
    fn start_menu_ids_become_paths_only_when_they_are_paths() {
        // У приложения из магазина пути нет — в песочнице его не запустить.
        assert_eq!(executable_of("Telegram.TelegramDesktop"), None);
        assert_eq!(executable_of("Claude_pzs8sxrjxfjjc!Claude"), None);
    }

    /// Что нашлось бы на этой машине — ничего не запуская.
    ///
    /// `cargo test pc::tests::what_would_open -- --ignored --nocapture`
    #[test]
    #[ignore = "читает меню «Пуск» и диски настоящей машины"]
    fn what_would_open() {
        let started = Instant::now();
        let fast = build(false);
        println!("быстрая часть: {} за {} мс", fast.len(), started.elapsed().as_millis());
        let started = Instant::now();
        let entries = build(true);
        println!("целиком: {} за {} мс", entries.len(), started.elapsed().as_millis());
        for spoken in [
            "телеграм",
            "клауде",
            "мортал шелл два",
            "киберпанк",
            "диспетчер устройств",
            "параметры bluetooth",
            "установка и удаление программ",
            "блокнот",
            "стим",
            "amnezia vpn",
        ] {
            match best_match(spoken, &entries) {
                Some(entry) => println!("{spoken:>30} → {} ({:?})", entry.name, entry.target),
                None => println!("{spoken:>30} → не нашлось"),
            }
        }
        println!("служба VPN: {:?}", vpn_service());
        let listed = run_hidden(
            &sc_exe(),
            &["query", "type=", "service", "state=", "all", "bufsize=", "262144"],
        );
        match &listed {
            Ok(out) => println!(
                "sc.exe: строк {}, про Amnezia: {:?}",
                out.lines().count(),
                out.lines().filter(|line| line.contains("Amnezia")).collect::<Vec<_>>()
            ),
            Err(err) => println!("sc.exe: ошибка {err}"),
        }
    }
}
