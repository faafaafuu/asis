//! Практика в терминале: человек работает руками, Ноа идёт рядом.
//!
//! Читать уроки получается не у всех: текст не держится, внимание уходит.
//! Здесь наоборот — задача настоящая (поднять кластер, настроить веб-сервер),
//! и делает её сам человек в настоящем терминале на своём сервере. Ноа видит
//! терминал: что набрано и что ответил сервер. Она объясняет происходящее,
//! замечает ошибки, подсказывает следующий шаг и время от времени
//! показывает общую картину — что уже построено и как части связаны.
//!
//! Терминал — псевдотерминал с оболочкой системы (на Windows — PowerShell
//! через ConPTY). Окно практики рисует его xterm.js, а всё, что идёт с
//! сервера, проходит здесь: так Ноа видит ровно то же, что человек, и не
//! зависит от того, каким терминалом он пользуется обычно.
//!
//! Вывод терминала уходит модели человека, поэтому перед отправкой похожее
//! на пароли, токены и ключи закрывается, а глаз в окне выключает
//! наблюдение совсем.

use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::ai_client::ThreadItem;

/// Окно практики.
pub const LABEL: &str = "practice";

/* ── Сценарий и ход практики ───────────────────────────────────────────── */

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub title: String,
    /// Что сделать и что должно получиться.
    #[serde(default)]
    pub goal: String,
    /// Зачем этот шаг в общей картине.
    #[serde(default)]
    pub why: String,
    /// Как убедиться, что получилось.
    #[serde(default)]
    pub check: String,
    /// Блок общей картины, к которому относится шаг.
    #[serde(default)]
    pub node: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub title: String,
    #[serde(default)]
    pub goal: String,
    /// Что нужно заранее: серверы, система, доступ.
    #[serde(default)]
    pub setup: String,
    /// Общая картина — схема в формате scheme.js.
    #[serde(default)]
    pub picture: serde_json::Value,
    pub steps: Vec<Step>,
}

/// Реплика в ленте практики.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    /// `noa` или `me`.
    pub who: String,
    pub text: String,
    /// `comment`, `error`, `step`, `hint`, `overview`, `answer`.
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Practice {
    pub scenario: Option<Scenario>,
    /// Текущий шаг сценария.
    #[serde(default)]
    pub step: usize,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub feed: Vec<Line>,
    /// Где в общей картине сейчас работа — id блока.
    #[serde(default)]
    pub here: String,
    /// Курс и тема, из которых открыли практику.
    #[serde(default)]
    pub course: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
    /// Смотрит ли Ноа в терминал. Выключили — вывод никуда не уходит.
    #[serde(default = "yes")]
    pub watching: bool,
}

fn yes() -> bool {
    true
}

struct Store {
    loaded: bool,
    practice: Practice,
    /// Последние замечания Ноа — чтобы не повторялась.
    thread: Vec<ThreadItem>,
    /// Ответ голосом идёт: фразы разговора без рук — вопросы практике.
    voice: bool,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    loaded: false,
    practice: Practice {
        scenario: None,
        step: 0,
        done: false,
        feed: Vec::new(),
        here: String::new(),
        course: None,
        topic: None,
        watching: true,
    },
    thread: Vec::new(),
    voice: false,
});

const MAX_FEED: usize = 80;
const DEPTH: usize = 4;

fn path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("practice.json"))
}

/// Первый доступ — с диска.
fn load(store: &mut Store, app: &AppHandle) {
    if store.loaded {
        return;
    }
    store.loaded = true;
    if let Some(saved) = path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Practice>(&text).ok())
    {
        store.practice = saved;
    }
}

/// Взглянуть на ход практики, ничего не меняя.
fn peek<R>(app: &AppHandle, look: impl FnOnce(&Practice) -> R) -> R {
    let mut store = STORE.lock().unwrap_or_else(|err| err.into_inner());
    load(&mut store, app);
    look(&store.practice)
}

/// Поменять ход практики — и сразу на диск.
fn with<R>(app: &AppHandle, change: impl FnOnce(&mut Practice) -> R) -> R {
    let mut store = STORE.lock().unwrap_or_else(|err| err.into_inner());
    load(&mut store, app);
    let out = change(&mut store.practice);
    let excess = store.practice.feed.len().saturating_sub(MAX_FEED);
    store.practice.feed.drain(..excess);
    if let (Some(path), Ok(json)) = (path(app), serde_json::to_string(&store.practice)) {
        if let Err(err) = std::fs::write(path, json) {
            log::warn!("практика не сохранилась: {err}");
        }
    }
    out
}

pub fn state(app: &AppHandle) -> Practice {
    peek(app, |p| p.clone())
}

/// Окну — свежий ход практики и что сказать вслух. `spoken` — уже звучит
/// из программы (ответ голосом), окну читать не надо.
fn publish(app: &AppHandle, say: &str, spoken: bool) {
    let practice = state(app);
    let _ = app.emit_to(
        LABEL,
        "practice:state",
        serde_json::json!({ "practice": practice, "say": say, "spoken": spoken }),
    );
}

fn note(p: &mut Practice, who: &str, kind: &str, text: &str) {
    if !text.trim().is_empty() {
        p.feed.push(Line {
            who: who.into(),
            kind: kind.into(),
            text: text.trim().to_string(),
        });
    }
}

/// Открыли практику из темы курса — запоминаем тему для сценария.
pub fn set_context(app: &AppHandle, course: Option<String>, topic: Option<String>) {
    if course.is_none() {
        return;
    }
    with(app, |p| {
        if p.scenario.is_none() {
            p.course = course;
            p.topic = topic;
        }
    });
}

/// Начать заново: сценарий и лента — прочь, терминал остаётся.
pub fn reset(app: &AppHandle) {
    with(app, |p| {
        let watching = p.watching;
        *p = Practice { watching, ..Practice::default() };
    });
    STORE.lock().unwrap_or_else(|err| err.into_inner()).thread.clear();
    publish(app, "", false);
}

pub fn set_watching(app: &AppHandle, on: bool) {
    with(app, |p| p.watching = on);
    if !on {
        let mut watch = WATCH.lock().unwrap_or_else(|err| err.into_inner());
        watch.fresh.clear();
        watch.entered = None;
    }
    publish(app, "", false);
}

/// Человек сам отметил шаг (или вернулся к прошлому).
pub fn move_step(app: &AppHandle, delta: i32) {
    let say = with(app, |p| {
        let total = p.scenario.as_ref().map_or(0, |s| s.steps.len());
        if total == 0 {
            return String::new();
        }
        if delta > 0 {
            advance(p)
        } else {
            p.done = false;
            p.step = p.step.saturating_sub(1);
            let text = step_intro(p);
            note(p, "noa", "step", &text);
            text
        }
    });
    publish(app, &say, false);
}

fn step_intro(p: &Practice) -> String {
    let Some(scenario) = &p.scenario else { return String::new() };
    let Some(step) = scenario.steps.get(p.step) else { return String::new() };
    format!("Шаг {} из {}: {}. {}", p.step + 1, scenario.steps.len(), step.title, step.goal)
}

/// Шаг сделан — следующий. Отдаёт, что сказать.
fn advance(p: &mut Practice) -> String {
    let total = p.scenario.as_ref().map_or(0, |s| s.steps.len());
    if p.done || total == 0 {
        return String::new();
    }
    if p.step + 1 >= total {
        p.done = true;
        let text = "Сценарий пройден — всё собрано. Нажмите «Общая картина», и пройдёмся по тому, что получилось и как оно связано.";
        note(p, "noa", "step", text);
        return text.into();
    }
    p.step += 1;
    if let Some(node) = p.scenario.as_ref().and_then(|s| s.steps.get(p.step)).map(|s| s.node.clone()) {
        if !node.is_empty() {
            p.here = node;
        }
    }
    let text = format!("Готово, идём дальше. {}", step_intro(p));
    note(p, "noa", "step", &text);
    text
}

/* ── Модель ────────────────────────────────────────────────────────────── */

async fn model(app: &AppHandle, rules: &str, thread: &[ThreadItem], said: &str, long: bool) -> Result<String, String> {
    let (provider, limit) = {
        let state = app.state::<crate::state::AppState>();
        let limit = state.config().ai.call_limit();
        (state.provider(), if long { limit * 3 } else { limit })
    };
    match tokio::time::timeout(limit, provider.converse(rules, thread, said, long)).await {
        Ok(Ok(text)) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        Ok(Ok(_)) => Err("Модель прислала пустой ответ.".into()),
        Ok(Err(err)) => Err(format!("Не получилось: {}", err.user_text("модель не ответила"))),
        Err(_) => Err("Модель не успела ответить.".into()),
    }
}

/// JSON-объект из ответа модели: без ```json и текста вокруг.
fn json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| &text[start..=end])
}

fn name(app: &AppHandle) -> String {
    app.state::<crate::state::AppState>().wake_name()
}

/// Тема курса, из которой открыли практику, — для сценария.
fn topic_material(p: &Practice) -> String {
    let (Some(course), Some(topic)) = (&p.course, &p.topic) else { return String::new() };
    let Ok(course) = crate::learning::course(course) else { return String::new() };
    let Ok(topic) = crate::learning::topic(&course, topic) else { return String::new() };
    let lesson: String = topic.lesson.chars().take(3000).collect();
    let terms = topic.concepts.iter().map(|c| c.term.as_str()).collect::<Vec<_>>().join(", ");
    format!(
        "Практика по курсу «{}», тема «{}»: {}\nПонятия темы: {terms}\nНачало урока:\n{lesson}\n",
        course.title, topic.title, topic.summary
    )
}

/// Сценарий для модели: цель, общая картина, шаги с отметкой текущего.
fn scenario_material(p: &Practice) -> String {
    let Some(s) = &p.scenario else {
        return "Сценария пока нет: человек просто работает в терминале. Объясняй, что он делает.\n".into();
    };
    let steps = s
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let mark = if p.done || i < p.step { "✓" } else if i == p.step { "→ сейчас" } else { " " };
            format!("{}. [{mark}] {} — {} (проверка: {})", i + 1, step.title, step.goal, step.check)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let picture = serde_json::to_string(&s.picture).unwrap_or_default();
    format!(
        "Сценарий «{}». Цель: {}\nЧто нужно заранее: {}\nОбщая картина (блоки и связи): {picture}\nШаги:\n{steps}\n",
        s.title, s.goal, s.setup
    )
}

/// Составить сценарий практики по цели человека.
pub async fn plan(app: &AppHandle, goal: &str) -> Result<Practice, String> {
    let goal = goal.trim();
    let context = peek(app, topic_material);
    if goal.is_empty() && context.is_empty() {
        return Err("Напишите, что хотите собрать.".into());
    }
    let goal_line = if goal.is_empty() { "по теме ниже — выбери живую задачу на ней".to_string() } else { format!("«{goal}»") };
    let rules = format!(
        "Ты — репетитор {}: составляешь сценарий практики в терминале. Человек учится руками: \
         сам набирает команды на своём сервере Linux по SSH (терминал открыт на его компьютере), \
         а ты идёшь рядом, видишь терминал и подсказываешь.\n\n{context}\n\
         Ответь строго JSON, без текста вокруг:\n\
         {{\"title\": \"…\", \"goal\": \"что будет работать в конце — одной фразой\", \
         \"setup\": \"что нужно заранее: сколько серверов, какая система, доступ — коротко\", \
         \"picture\": {{\"title\": \"…\", \"points\": [\"…\"], \"nodes\": [{{\"id\": \"a\", \"label\": \"…\"}}], \
         \"edges\": [{{\"from\": \"a\", \"to\": \"b\", \"label\": \"…\"}}]}}, \
         \"steps\": [{{\"title\": \"…\", \"goal\": \"…\", \"why\": \"…\", \"check\": \"…\", \"node\": \"a\"}}]}}\n\
         - picture — общая картина того, что строим: 3–8 блоков по 1–4 слова (компоненты, \
         серверы, службы) и стрелки — кто к кому обращается или что из чего следует; points — \
         3–5 главных мыслей, каждая до 8 слов.\n\
         - steps — 5–10 шагов от подготовки до проверки результата. title — 2–5 слов. goal — что \
         сделать и что должно получиться, 1–2 фразы, без готовых команд: человек ищет сам, \
         команду подскажешь по ходу. why — зачем шаг в общей картине, одна фраза. check — как \
         убедиться, что получилось: команда или признак. node — id блока картины.\n\
         Пиши по-русски.",
        name(app)
    );
    let text = model(app, &rules, &[], &format!("Составь сценарий: {goal_line}."), true).await?;
    let mut scenario: Scenario = json_object(&text)
        .and_then(|json| serde_json::from_str(json).ok())
        .ok_or("Сценарий не сложился — попробуйте ещё раз или опишите цель иначе.")?;
    scenario.steps.retain(|s| !s.title.trim().is_empty());
    scenario.steps.truncate(12);
    if scenario.steps.is_empty() {
        return Err("В сценарии нет шагов — попробуйте ещё раз.".into());
    }
    log::info!("практика: сценарий «{}», шагов {}", scenario.title, scenario.steps.len());
    let say = with(app, |p| {
        p.here = scenario.steps[0].node.clone();
        p.scenario = Some(scenario.clone());
        p.step = 0;
        p.done = false;
        p.feed.clear();
        let intro = format!(
            "{}. В конце: {} Наверху справа — общая картина. Начнём. {}",
            scenario.title,
            scenario.goal.trim_end_matches('.').to_string() + ".",
            step_intro(p)
        );
        note(p, "noa", "step", &intro);
        intro
    });
    STORE.lock().unwrap_or_else(|err| err.into_inner()).thread.clear();
    publish(app, &say, false);
    Ok(state(app))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Look {
    say: String,
    done: bool,
    next: String,
    node: String,
    error: bool,
}

/// Ноа посмотрела на новое в терминале.
async fn observe(app: &AppHandle, chunk: String, running: bool) {
    let (material, thread) = {
        let material = peek(app, scenario_material);
        let thread = STORE.lock().unwrap_or_else(|err| err.into_inner()).thread.clone();
        (material, thread)
    };
    let rules = format!(
        "Ты — репетитор {}: ведёшь практику в терминале. Человек сам набирает команды, ты видишь \
         его терминал и идёшь рядом: объясняешь, что он сделал и что ответил сервер, замечаешь \
         ошибки, подсказываешь следующий шаг.\n\n{material}\n\
         Ответь строго JSON, без текста вокруг: \
         {{\"say\": \"…\", \"done\": false, \"next\": \"…\", \"node\": \"…\", \"error\": false}}\n\
         - say — что сказать вслух, 1–2 короткие разговорные фразы: что сейчас произошло и почему \
         это важно для цели. Ошибка — что она значит и как поправить. Рутина без нового смысла \
         (cd, ls, clear, опечатка, которую человек сам исправил) — пустая строка.\n\
         - done — true, только если по выводу видно, что текущий шаг сценария выполнен.\n\
         - next — следующее конкретное действие, можно с командой в `обратных кавычках`; пусто, \
         если человек и так идёт верно.\n\
         - node — id блока общей картины, которого касается сделанное, или пусто.\n\
         - error — true, если команда упала или сделано не то.\n\
         Команды в say не зачитывай — говори словами, что они делают. Без похвалы и вступлений. \
         Не повторяй сказанное раньше. По-русски.",
        name(app)
    );
    let said = format!(
        "Новое в терминале{}:\n```\n{chunk}\n```",
        if running { " (команда ещё выполняется)" } else { "" }
    );
    let text = match model(app, &rules, &thread, &said, false).await {
        Ok(text) => text,
        Err(err) => {
            log::warn!("практика: Ноа не посмотрела в терминал: {err}");
            return;
        }
    };
    let look: Look = json_object(&text)
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_else(|| Look { say: text.clone(), ..Look::default() });
    {
        let mut store = STORE.lock().unwrap_or_else(|err| err.into_inner());
        let short: String = chunk.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
        store.thread.push(ThreadItem { q: short, a: look.say.clone() });
        let excess = store.thread.len().saturating_sub(DEPTH);
        store.thread.drain(..excess);
    }
    let say = with(app, |p| {
        if !look.node.is_empty() {
            p.here = look.node.clone();
        }
        let mut say = look.say.trim().to_string();
        note(p, "noa", if look.error { "error" } else { "comment" }, &say);
        if look.done && !running {
            let next = advance(p);
            say = format!("{say} {next}").trim().to_string();
        } else if !look.next.trim().is_empty() {
            note(p, "noa", "hint", &format!("Дальше: {}", look.next.trim()));
        }
        say
    });
    publish(app, &speakable(&say), false);
}

/// Вопрос человека — текстом или голосом.
pub async fn ask(app: &AppHandle, text: &str, voice: bool) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Спросите что-нибудь.".into());
    }
    let material = with(app, |p| {
        note(p, "me", "answer", text);
        scenario_material(p)
    });
    publish(app, "", false);
    let tail = terminal_tail(3000);
    let length = if voice {
        "Ответ прозвучит вслух: 2–5 коротких фраз, без разметки; команду назови словами."
    } else {
        "Ответ читают рядом с терминалом: до 120 слов, команды — в `обратных кавычках`."
    };
    let rules = format!(
        "Ты — репетитор {}: ведёшь практику в терминале. Человек сам набирает команды; ты видишь \
         его терминал и отвечаешь на вопросы по ходу работы.\n\n{material}\n\
         Последнее в терминале:\n```\n{tail}\n```\n\
         Как отвечать: сразу по существу, простыми словами, с опорой на то, что сейчас в \
         терминале. Просят подсказку — сначала направление, команду — если застрял. Просят общую \
         картину — что уже построено, где мы сейчас, что впереди и как части связаны. {length} \
         Без вступлений и похвалы. По-русски.",
        name(app)
    );
    let thread = STORE.lock().unwrap_or_else(|err| err.into_inner()).thread.clone();
    let answer = model(app, &rules, &thread, text, false).await?;
    {
        let mut store = STORE.lock().unwrap_or_else(|err| err.into_inner());
        store.thread.push(ThreadItem { q: text.to_string(), a: answer.clone() });
        let excess = store.thread.len().saturating_sub(DEPTH);
        store.thread.drain(..excess);
    }
    let kind = if text.starts_with(OVERVIEW) { "overview" } else { "answer" };
    with(app, |p| note(p, "noa", kind, &answer));
    publish(app, &speakable(&answer), voice);
    Ok(answer)
}

/// Просьба об общей картине — по ней же лента помечает ответ.
pub const OVERVIEW: &str = "Общая картина:";

/// Без разметки — для голоса.
fn speakable(text: &str) -> String {
    text.replace(['`', '*', '#'], "")
}

/* ── Голос ─────────────────────────────────────────────────────────────── */

/// Спрашивают голосом: фразы разговора без рук идут сюда.
pub fn voice_on() -> bool {
    STORE.lock().unwrap_or_else(|err| err.into_inner()).voice
}

pub fn set_voice(on: bool) {
    STORE.lock().unwrap_or_else(|err| err.into_inner()).voice = on;
}

/* ── Терминал ──────────────────────────────────────────────────────────── */

struct Term {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    /// Номер сеанса: старый поток чтения не пишет в новый терминал.
    id: u64,
}

static TERM: Mutex<Option<Term>> = Mutex::new(None);

/// Что видно в терминале и чего Ноа ещё не видела.
struct Watch {
    /// Новое с прошлого взгляда — как пришло, с управляющими кодами.
    fresh: String,
    /// Последнее в терминале — для вопросов и для окна после перезагрузки.
    replay: String,
    /// Когда нажали Enter — с этого момента ждём, чем ответит сервер.
    entered: Option<Instant>,
    last_out: Option<Instant>,
    /// Полноэкранная программа (vim, htop, less): её экран — не разговор.
    full_screen: bool,
    /// Про долгую команду уже сказали.
    long_noted: bool,
    /// Модель смотрит — следующий взгляд подождёт.
    busy: bool,
    session: u64,
}

static WATCH: Mutex<Watch> = Mutex::new(Watch {
    fresh: String::new(),
    replay: String::new(),
    entered: None,
    last_out: None,
    full_screen: false,
    long_noted: false,
    busy: false,
    session: 0,
});

const MAX_FRESH: usize = 40_000;
const MAX_REPLAY: usize = 64_000;
/// Сервер замолчал на столько после Enter — команда, скорее всего, кончилась.
const QUIET: Duration = Duration::from_millis(1400);
/// Команда идёт дольше — Ноа смотрит, не дожидаясь конца.
const LONG: Duration = Duration::from_secs(25);

fn shell() -> CommandBuilder {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = CommandBuilder::new("powershell.exe");
        cmd.arg("-NoLogo");
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let mut cmd = CommandBuilder::new(shell);
        cmd.arg("-l");
        cmd
    }
}

fn trim_front(text: &mut String, max: usize) {
    if text.len() > max {
        let mut cut = text.len() - max;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        text.drain(..cut);
    }
}

/// Запускает терминал. Уже идёт — отдаёт то, что в нём было, чтобы окно
/// после перезагрузки показало прежний экран.
pub fn term_start(app: &AppHandle, cols: u16, rows: u16) -> Result<String, String> {
    let mut term = TERM.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(running) = term.as_mut() {
        if running.child.try_wait().ok().flatten().is_none() {
            let _ = running.master.resize(size(cols, rows));
            // Запрос положения курсора (ESC[6n) ConPTY шлёт один раз при запуске,
            // и xterm на него отвечает. Повтор из записи вызвал бы второй ответ —
            // он ушёл бы в оболочку мусором «^[[1;1R».
            let replay = WATCH.lock().unwrap_or_else(|err| err.into_inner()).replay.replace("\x1b[6n", "");
            return Ok(replay);
        }
    }
    let pair = native_pty_system()
        .openpty(size(cols, rows))
        .map_err(|err| format!("терминал не открылся: {err}"))?;
    let mut cmd = shell();
    cmd.env("TERM", "xterm-256color");
    if let Some(home) = app.path().home_dir().ok() {
        cmd.cwd(home);
    }
    let child = pair.slave.spawn_command(cmd).map_err(|err| format!("оболочка не запустилась: {err}"))?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|err| err.to_string())?;
    let writer = pair.master.take_writer().map_err(|err| err.to_string())?;

    let id = {
        let mut watch = WATCH.lock().unwrap_or_else(|err| err.into_inner());
        watch.session += 1;
        watch.fresh.clear();
        watch.replay.clear();
        watch.entered = None;
        watch.full_screen = false;
        watch.session
    };
    *term = Some(Term { master: pair.master, writer, child, id });
    log::info!("практика: терминал запущен ({cols}×{rows})");

    let reading = app.clone();
    std::thread::Builder::new()
        .name("sufler-practice-pty".into())
        .spawn(move || {
            let mut buf = [0u8; 8192];
            let mut carry: Vec<u8> = Vec::new();
            loop {
                let read = match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                carry.extend_from_slice(&buf[..read]);
                // Конец куска может разрезать букву пополам — хвост ждёт следующего.
                let valid = match std::str::from_utf8(&carry) {
                    Ok(_) => carry.len(),
                    Err(err) if err.error_len().is_none() => err.valid_up_to(),
                    Err(_) => carry.len(),
                };
                let text = String::from_utf8_lossy(&carry[..valid]).into_owned();
                carry.drain(..valid);
                if text.is_empty() {
                    continue;
                }
                {
                    let mut watch = WATCH.lock().unwrap_or_else(|err| err.into_inner());
                    if watch.session != id {
                        break;
                    }
                    for (on, off) in [("\x1b[?1049h", "\x1b[?1049l"), ("\x1b[?47h", "\x1b[?47l"), ("\x1b[?1047h", "\x1b[?1047l")] {
                        match (text.rfind(on), text.rfind(off)) {
                            (Some(a), Some(b)) => watch.full_screen = a > b,
                            (Some(_), None) => watch.full_screen = true,
                            (None, Some(_)) => watch.full_screen = false,
                            _ => {}
                        }
                    }
                    watch.fresh.push_str(&text);
                    trim_front(&mut watch.fresh, MAX_FRESH);
                    watch.replay.push_str(&text);
                    trim_front(&mut watch.replay, MAX_REPLAY);
                    watch.last_out = Some(Instant::now());
                }
                let _ = reading.emit_to(LABEL, "practice:out", text);
            }
            let ended = WATCH.lock().unwrap_or_else(|err| err.into_inner()).session == id;
            if ended {
                log::info!("практика: оболочка закрылась");
                let _ = reading.emit_to(LABEL, "practice:exit", ());
            }
        })
        .map_err(|err| err.to_string())?;

    let watching = app.clone();
    tauri::async_runtime::spawn(async move { watch_loop(watching, id).await });
    Ok(String::new())
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(5),
        cols: cols.max(20),
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub fn term_write(data: &str) -> Result<(), String> {
    let mut term = TERM.lock().unwrap_or_else(|err| err.into_inner());
    let term = term.as_mut().ok_or("терминал не запущен")?;
    term.writer.write_all(data.as_bytes()).map_err(|err| err.to_string())?;
    let _ = term.writer.flush();
    if data.contains('\r') {
        let mut watch = WATCH.lock().unwrap_or_else(|err| err.into_inner());
        watch.entered = Some(Instant::now());
        watch.long_noted = false;
    }
    Ok(())
}

pub fn term_resize(cols: u16, rows: u16) {
    if let Some(term) = TERM.lock().unwrap_or_else(|err| err.into_inner()).as_ref() {
        let _ = term.master.resize(size(cols, rows));
    }
}

pub fn term_stop() {
    if let Some(mut term) = TERM.lock().unwrap_or_else(|err| err.into_inner()).take() {
        let _ = term.child.kill();
        WATCH.lock().unwrap_or_else(|err| err.into_inner()).session += 1;
        log::info!("практика: терминал закрыт (сеанс {})", term.id);
    }
}

/// Последнее в терминале, уже очищенное, — для вопросов.
fn terminal_tail(max: usize) -> String {
    let replay = WATCH.lock().unwrap_or_else(|err| err.into_inner()).replay.clone();
    shorten(&redact(&clean(&replay)), max)
}

/// Следит за терминалом: команда кончилась (сервер замолчал после Enter) или
/// идёт долго — Ноа смотрит на новое.
async fn watch_loop(app: AppHandle, id: u64) {
    loop {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let watching = peek(&app, |p| p.watching);
        let take = {
            let mut watch = WATCH.lock().unwrap_or_else(|err| err.into_inner());
            if watch.session != id {
                return;
            }
            let Some(entered) = watch.entered else { continue };
            if !watching || watch.busy || watch.full_screen {
                continue;
            }
            let quiet = watch.last_out.is_some_and(|at| at.elapsed() >= QUIET) && entered.elapsed() >= QUIET;
            let long = !watch.long_noted && entered.elapsed() >= LONG;
            if !quiet && !long {
                continue;
            }
            if quiet {
                watch.entered = None;
            } else {
                watch.long_noted = true;
            }
            watch.busy = true;
            Some((std::mem::take(&mut watch.fresh), !quiet))
        };
        let Some((raw, running)) = take else { continue };
        let chunk = shorten(&redact(&clean(&raw)), 5000);
        // Модель отвечает секунды. Частое — без неё: рутину (cd, ls) Ноа
        // пропускает молча, известные ошибки разбирает сразу.
        if running {
            observe(&app, chunk, true).await;
        } else if let Some(hint) = instant(&chunk) {
            with(&app, |p| note(p, "noa", "error", hint));
            publish(&app, hint, false);
        } else if !routine(&chunk) && chunk.trim().chars().count() >= 2 {
            observe(&app, chunk, false).await;
        }
        WATCH.lock().unwrap_or_else(|err| err.into_inner()).busy = false;
    }
}

/// Команда из первой строки куска: после приглашения `$ `, `# ` или `> `.
fn command_of(chunk: &str) -> String {
    let first = chunk.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let at = ["$ ", "# ", "> "].iter().filter_map(|mark| first.rfind(mark).map(|i| i + 2)).max().unwrap_or(0);
    first[at..].trim().to_string()
}

/// Рутина без нового смысла: походить по папкам, посмотреть, очистить экран.
/// Ошибка в выводе — уже не рутина.
fn routine(chunk: &str) -> bool {
    let command = command_of(chunk);
    let word = command.split_whitespace().next().unwrap_or("");
    const PLAIN: &[&str] = &["cd", "ls", "ll", "la", "dir", "pwd", "clear", "cls", "history", "whoami", "exit", "logout", "echo"];
    let failed = chunk.lines().skip(1).any(|l| {
        let l = l.to_lowercase();
        l.contains("error") || l.contains("not found") || l.contains("no such") || l.contains("denied") || l.contains("ошибк")
    });
    (command.is_empty() || PLAIN.contains(&word)) && !failed
}

/// Частые ошибки — ответ сразу, без модели. Первая подходящая.
fn instant(chunk: &str) -> Option<&'static str> {
    let lower = chunk.to_lowercase();
    const KNOWN: &[(&[&str], &str)] = &[
        (
            &["permission denied (publickey"],
            "Сервер не принял ключ: на нём нет вашего открытого ключа или указан не тот пользователь. Проверьте имя в ssh и ключ в ~/.ssh/authorized_keys на сервере.",
        ),
        (
            &["could not open lock file", "are you root", "operation not permitted", "permission denied"],
            "Не хватает прав: этой команде нужен root. Повторите её через sudo.",
        ),
        (
            &["command not found", "is not recognized as", "не распознано как имя"],
            "Такой команды здесь нет: опечатка или программа ещё не установлена.",
        ),
        (
            &["unable to locate package", "no match for argument"],
            "Пакет не найден: обновите список пакетов — apt update — или проверьте имя.",
        ),
        (
            &["could not resolve host", "name or service not known", "temporary failure in name resolution"],
            "Имя не находится: опечатка в адресе, или на сервере не работает DNS.",
        ),
        (
            &["connection refused"],
            "Соединение отклонено: служба на этом порту не запущена или слушает другой адрес.",
        ),
        (
            &["connection timed out", "operation timed out", "no route to host"],
            "Ответа нет: адрес недоступен — неверный IP, закрытый порт или фаервол.",
        ),
        (
            &["address already in use"],
            "Порт уже занят другой программой. Кем — покажет ss -ltnp.",
        ),
        (
            &["host key verification failed", "remote host identification has changed"],
            "Ключ сервера изменился с прошлого раза — так бывает после переустановки. Если это ваш сервер, уберите старую запись: ssh-keygen -R адрес.",
        ),
        (
            &["no such file or directory"],
            "Такого файла или папки нет: проверьте путь — pwd и ls покажут, где вы.",
        ),
    ];
    KNOWN
        .iter()
        .find(|(marks, _)| marks.iter().any(|mark| lower.contains(mark)))
        .map(|(_, text)| *text)
}

/// Длинный вывод — начало и конец: середина сборок и установок однообразна.
fn shorten(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    let head: String = chars[..max / 5].iter().collect();
    let tail: String = chars[chars.len() - (max - max / 5)..].iter().collect();
    format!("{head}\n…(пропущено {} знаков)…\n{tail}", chars.len() - max)
}

/// Вывод терминала — в простой текст: без цветов, движений курсора и
/// перерисовок. Перемещение курсора по строкам становится переводом строки,
/// чтобы перерисованный ConPTY экран не слипался в одну строку.
pub fn clean(raw: &str) -> String {
    let mut lines: Vec<String> = vec![String::new()];
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    let mut last = ' ';
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            last = next;
                            break;
                        }
                        params.push(next);
                    }
                    match last {
                        'H' | 'f' | 'A' | 'B' | 'E' | 'F' | 'd' => {
                            if !lines.last().is_some_and(|l| l.trim().is_empty()) {
                                lines.push(String::new());
                            } else if let Some(line) = lines.last_mut() {
                                line.clear();
                            }
                        }
                        'C' => {
                            let n = params.parse::<usize>().unwrap_or(1).min(40);
                            if let Some(line) = lines.last_mut() {
                                line.push_str(&" ".repeat(n));
                            }
                        }
                        _ => {}
                    }
                }
                // OSC: заголовок окна и прочее — до BEL или ESC \.
                Some(']') => {
                    while let Some(next) = chars.next() {
                        if next == '\x07' {
                            break;
                        }
                        if next == '\x1b' {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('(' | ')') => {
                    chars.next();
                }
                _ => {}
            },
            '\n' => lines.push(String::new()),
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    // Строку перерисовывают поверх — прогресс, приглашение.
                    if let Some(line) = lines.last_mut() {
                        if chars.peek().is_some_and(|n| *n != '\x1b') {
                            line.clear();
                        }
                    }
                }
            }
            '\x08' => {
                if let Some(line) = lines.last_mut() {
                    line.pop();
                }
            }
            '\t' => lines.last_mut().unwrap().push_str("    "),
            c if c.is_control() => {}
            c => lines.last_mut().unwrap().push(c),
        }
    }
    let mut out: Vec<&str> = Vec::new();
    for line in &lines {
        let line = line.trim_end();
        if line.is_empty() && out.last().is_some_and(|l: &&str| l.is_empty()) {
            continue;
        }
        // ConPTY перерисовывает строки целиком — подряд одинаковые не нужны.
        if !line.is_empty() && out.last() == Some(&line) {
            continue;
        }
        out.push(line);
    }
    out.join("\n").trim().to_string()
}

/// Закрывает похожее на секреты: пароли и токены в командах, ключи API,
/// закрытые ключи. Вывод уходит модели — ей это знать незачем.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_key = false;
    for line in text.split_inclusive('\n') {
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
            in_key = true;
            out.push_str("[закрытый ключ скрыт]\n");
            continue;
        }
        if in_key {
            if line.contains("-----END") {
                in_key = false;
            }
            continue;
        }
        out.push_str(&redact_line(line));
    }
    out
}

fn redact_line(line: &str) -> String {
    const NAMES: &[&str] = &["password", "passwd", "pass", "token", "secret", "apikey", "api_key", "api-key", "пароль"];
    let mut words: Vec<String> = Vec::new();
    let mut hide_next = false;
    for word in line.split(' ') {
        if hide_next && !word.trim().is_empty() {
            words.push("[скрыто]".into());
            hide_next = false;
            continue;
        }
        let lower = word.to_lowercase();
        let bare = lower.trim_start_matches('-');
        // --password=xxx, TOKEN=xxx, password: xxx
        if let Some(at) = word.find(['=', ':']) {
            let key = bare.split(['=', ':']).next().unwrap_or("");
            if NAMES.iter().any(|n| key.ends_with(n)) && word.len() > at + 1 {
                words.push(format!("{}[скрыто]", &word[..=at]));
                continue;
            }
        }
        // --password xxx, -p xxx у mysql — значение следующим словом.
        if NAMES.iter().any(|n| bare == *n || bare.ends_with(&format!("_{n}"))) && word.starts_with('-') {
            hide_next = true;
            words.push(word.to_string());
            continue;
        }
        words.push(if looks_secret(word) { "[скрыто]".to_string() } else { word.to_string() });
    }
    words.join(" ")
}

/// Токены по виду: известные приставки и длинные случайные строки.
fn looks_secret(word: &str) -> bool {
    let word = word.trim_matches(|c: char| "\"'`,;()[]{}<>".contains(c) || c.is_whitespace());
    const PREFIXES: &[&str] = &["sk-", "ghp_", "gho_", "github_pat_", "xoxb-", "xoxp-", "glpat-", "AKIA", "eyJ", "K10"];
    if word.len() >= 20 && PREFIXES.iter().any(|p| word.starts_with(p)) {
        return true;
    }
    // Длинная смесь букв разного регистра и цифр — похоже на ключ. Хеши из
    // одних шестнадцатеричных цифр (образы, коммиты) не трогаем: они не тайна.
    let len = word.chars().count();
    if len >= 32 && !word.contains('/') && !word.contains('.') {
        let upper = word.chars().any(|c| c.is_ascii_uppercase());
        let lower = word.chars().any(|c| c.is_ascii_lowercase());
        let digit = word.chars().any(|c| c.is_ascii_digit());
        let plain = word.chars().all(|c| c.is_ascii_alphanumeric() || "+=_-:".contains(c));
        let hex = word.chars().all(|c| c.is_ascii_hexdigit());
        return plain && upper && lower && digit && !hex;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_colors_and_redraws() {
        let raw = "\x1b[32mroot@node1\x1b[0m:~# kubectl get nodes\r\nNAME    STATUS\r\nnode1   Ready\r\n";
        assert_eq!(clean(raw), "root@node1:~# kubectl get nodes\nNAME    STATUS\nnode1   Ready");
    }

    #[test]
    fn clean_keeps_last_progress_frame() {
        assert_eq!(clean("Downloading 10%\rDownloading 50%\rDownloading 100%\r\n"), "Downloading 100%");
    }

    #[test]
    fn clean_applies_backspace_and_cursor_moves() {
        assert_eq!(clean("lss\x08 -la"), "ls -la");
        assert_eq!(clean("PS C:\\>\x1b[2;1Hline two"), "PS C:\\>\nline two");
        assert_eq!(clean("\x1b]0;title\x07ok"), "ok");
    }

    #[test]
    fn redact_hides_passwords_and_tokens() {
        let line = "mysql -u root --password=hunter2 && export GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz1234";
        let out = redact(line);
        assert!(!out.contains("hunter2"), "{out}");
        assert!(!out.contains("ghp_abc"), "{out}");
        assert!(out.contains("mysql -u root"));
    }

    #[test]
    fn redact_hides_private_keys_but_not_hashes() {
        let text = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n-----END OPENSSH PRIVATE KEY-----\nsha256:4f2a9c1e0b7d3a5f6c8e9d0b1a2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c\n";
        let out = redact(text);
        assert!(out.contains("[закрытый ключ скрыт]"));
        assert!(!out.contains("b3BlbnNzaC1rZXk"));
        assert!(out.contains("4f2a9c1e"));
    }

    #[test]
    fn routine_commands_skip_the_model() {
        assert!(routine("root@node1:~# ls -la\ntotal 8\ndrwx------ 2 root root 4096 ."));
        assert!(routine("PS C:\\Users\\manda> cd projects"));
        assert!(!routine("root@node1:~# ls /nope\nls: cannot access '/nope': No such file or directory"));
        assert!(!routine("root@node1:~# kubectl get nodes\nNAME STATUS"));
    }

    #[test]
    fn known_errors_answer_at_once() {
        let lock = "root@node1:~$ apt install nginx\nE: Could not open lock file /var/lib/dpkg/lock-frontend - open (13: Permission denied)";
        assert!(instant(lock).unwrap().contains("sudo"));
        assert!(instant("me@pc:~$ ssh root@host\nroot@host: Permission denied (publickey).").unwrap().contains("ключ"));
        assert!(instant("$ kubeclt get pods\nkubeclt: command not found").unwrap().contains("команды"));
        assert_eq!(instant("$ kubectl get nodes\nnode1 Ready"), None);
    }

    #[test]
    fn json_object_survives_fences() {
        assert_eq!(json_object("```json\n{\"say\": \"a\"}\n```"), Some("{\"say\": \"a\"}"));
        assert_eq!(json_object("нет json"), None);
    }

    #[test]
    fn shorten_keeps_head_and_tail() {
        let text = "a".repeat(100) + &"b".repeat(100);
        let short = shorten(&text, 50);
        assert!(short.starts_with("aaaaaaaaaa") && short.ends_with("bbbb") && short.contains("пропущено"));
    }
}

#[cfg(test)]
mod pty_tests {
    use super::*;

    /// Настоящая оболочка в псевдотерминале: команда уходит, вывод приходит.
    /// Запуск: `cargo test --lib practice::pty_tests -- --ignored`.
    #[test]
    #[ignore]
    fn shell_runs_in_pty() {
        let pair = native_pty_system().openpty(size(100, 30)).unwrap();
        let mut child = pair.slave.spawn_command(shell()).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(String::from_utf8_lossy(&buf[..n]).into_owned()).is_err() {
                    break;
                }
            }
        });
        // ConPTY ждёт ответа на запрос положения курсора — в окне его даёт xterm.
        #[cfg(target_os = "windows")]
        writer.write_all(b"\x1b[1;1R").unwrap();
        std::thread::sleep(Duration::from_millis(2500));
        #[cfg(target_os = "windows")]
        writer.write_all(b"echo ('noa-' + (2+3) + '-ok')\r").unwrap();
        #[cfg(not(target_os = "windows"))]
        writer.write_all(b"echo noa-$((2+3))-ok\r").unwrap();
        let mut seen = String::new();
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until && !clean(&seen).contains("noa-5-ok") {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(200)) {
                seen.push_str(&chunk);
            }
        }
        let _ = child.kill();
        println!("{}", clean(&seen));
        assert!(clean(&seen).contains("noa-5-ok"), "{}", clean(&seen));
    }
}
