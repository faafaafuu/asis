//! Запись голоса: удержанием клавиши и разговором без рук.
//!
//! Два режима. Первый — клавишу держат, пока говорят: начало и конец фразы
//! известны точно. Второй — разговор: микрофон открыт постоянно, а границы фраз
//! приходится находить самим, по тишине.
//!
//! Запись идёт своим потоком, который владеет устройством ввода: поток cpal
//! нельзя передавать между потоками, а начинают и заканчивают запись по нажатию
//! клавиши, то есть откуда угодно.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Частота, на которой работает whisper. Любую другую он всё равно приведёт
/// к этой, но лучше сделать это самим: передавать по сети процессов лишние
/// сто килобайт в секунду незачем.
const WHISPER_RATE: u32 = 16_000;

/// Дольше этого не пишем одну фразу.
///
/// Не ограничение ради ограничения: клавишу можно зажать и забыть, а память
/// под запись растёт всё это время. Две минуты — заведомо больше любого
/// вопроса, который задают голосом.
const MAX_SECONDS: usize = 120;

/// Сколько дописывать после отпускания клавиши.
///
/// Не техническая задержка, а человеческая. Порция звука от устройства — это
/// десятки миллисекунд, их перекрыла бы и треть секунды. Но клавишу отпускают
/// не после последнего слова, а вместе с ним, а часто и чуть раньше: рука
/// быстрее речи. Секунда лишней тишины расшифровке не мешает, а потерянное
/// слово меняет смысл вопроса.
const TAIL_MS: u64 = 900;

/// Сколько тишины дописать в конец записи.
///
/// Whisper разбирает звук окном и последний отрезок заканчивает по границе
/// тишины. Записи, обрывающейся на полуслове, такой границы не даёт — и модель
/// нередко просто не выдаёт последнее слово. Полсекунды тишины дают ей эту
/// границу.
const SILENCE_TAIL_MS: u64 = 500;

/// Ниже какой громкости считаем, что записана тишина, а не речь.
const SILENCE_LEVEL: f32 = 0.02;

/// До какого уровня подтягивать тихую запись.
///
/// Микрофоны сильно отличаются по чувствительности, а на тихой записи Whisper
/// ошибается заметно чаще: тихая речь для него ближе к шуму. Приводим всё
/// к одной громкости — это не «улучшение звука», а выравнивание условий.
const TARGET_PEAK: f32 = 0.85;

/// Во сколько раз самое большее подтягивать громкость.
///
/// Без потолка усиление на границе тишины доходит до сорока раз, и шум
/// вентилятора превращается в громкий неразборчивый звук. Для Whisper это уже
/// не тишина, а речь, которую надо расшифровать, — и она её выдумывает. То есть
/// выравнивание громкости кормило бы ровно ту петлю, которую мы чиним.
const MAX_GAIN: f32 = 8.0;

/// Сколько тишины считать концом фразы в разговоре.
///
/// Между словами человек молчит до полусекунды — на запятой, на вдохе, подбирая
/// слово. Меньше секунды здесь означало бы рвать фразу посередине и отвечать на
/// половину вопроса. Больше — заметная пауза перед каждым ответом.
const END_OF_PHRASE_MS: u64 = 1000;

/// Короче этого — не фраза, а кашель, щелчок мыши или скрип стула.
const MIN_SPEECH_MS: u64 = 400;

/// Сколько молчать, чтобы разговор закончился сам.
///
/// Шесть секунд оказались слишком строгими: человек читает ответ на экране,
/// обдумывает следующий вопрос, отвлекается на секунду — и разговор обрывался
/// у него под руками. Минута — это уже точно «ушёл и не вернулся», а не пауза
/// в разговоре. Закрыть раньше всегда можно клавишей Esc.
const SILENCE_ENDS_TALK_MS: u64 = 60_000;

/// Зачем открыт микрофон. От этого зависит, насколько он придирчив.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Listening {
    /// Разговор: человек обращается к программе и знает, что его слушают.
    /// Здесь важно не пропустить тихую фразу.
    Talk,
    /// Ожидание обращения: программа слушает комнату часами. Здесь важно
    /// обратное — не принимать за речь всё подряд. Каждая принятая «фраза»
    /// это работа видеокарты, а комнатный шум её не стоит.
    Wake,
}

impl Listening {
    /// Во сколько раз речь должна быть громче фонового шума.
    fn over_noise(self) -> f32 {
        match self {
            Listening::Talk => 4.0,
            // Чуть строже разговора, но именно чуть. Сначала было в полтора
            // раза строже — и зов перестал доходить: на клавишу помощник
            // отзывался, на имя почти никогда. Отсеивать шум лучше не порогом,
            // а тем, что в шуме не окажется имени.
            Listening::Wake => 4.5,
        }
    }

    /// Ниже какой громкости не считаем речью вовсе.
    fn floor(self) -> f32 {
        match self {
            Listening::Talk => SILENCE_LEVEL,
            // Обращение произносят внятно и в сторону компьютера, но микрофоны
            // бывают тихие: на здешнем речь идёт по трети шкалы, и порог в
            // девять сотых съедал половину зовов. Пять — это вдвое выше порога
            // разговора и всё ещё ниже любой внятной речи.
            Listening::Wake => 0.05,
        }
    }

    /// Короче этого фразу даже не рассматриваем.
    fn min_speech_ms(self) -> u64 {
        match self {
            Listening::Talk => MIN_SPEECH_MS,
            // «Хэй, ноа», сказанное быстро, укладывается в полсекунды — и это
            // законный зов, на который надо ответить. Порог выше отбрасывал
            // ровно его: с вопросом фраза проходила, без вопроса — нет.
            Listening::Wake => 500,
        }
    }

    /// Писать ли в журнал про каждую услышанную фразу.
    ///
    /// В ожидании обращения — не писать: программа слушает часами, и журнал
    /// из полезного превращается в поток, который сам себя вытесняет. Именно
    /// это и случилось: настоящие ошибки терялись среди записей о шуме.
    fn verbose(self) -> bool {
        self == Listening::Talk
    }
}

/// Что услышали в разговоре.
pub enum Heard {
    /// Человек договорил фразу — вот она, готовая к расшифровке.
    Phrase(Vec<u8>),
    /// Человек молчит слишком долго. Разговор пора заканчивать.
    LongSilence,
}

enum Command {
    /// Запись удержанием клавиши. Строка — имя устройства, пусто — основное.
    Start(String),
    /// Закончить и отдать записанное.
    Stop(Sender<Recording>),
    /// Разговор: писать постоянно и отдавать фразы по мере того, как человек
    /// их договаривает.
    StartTalk(String, Listening, Sender<Heard>),
    StopTalk,
}

#[derive(Default)]
struct Recording {
    samples: Vec<f32>,
    sample_rate: u32,
}

static COMMANDS: OnceLock<Sender<Command>> = OnceLock::new();

/// Разговор приостановлен: программа думает над ответом или читает его вслух.
static PAUSED: AtomicBool = AtomicBool::new(false);

/// До какого момента не слушать вовсе.
///
/// Ставится, пока программа говорит, и держится ещё некоторое время после.
/// Причина в том, что тишина наступает не тогда, когда мы перестали отдавать
/// звук: то, что уже ушло в звуковую систему, доигрывает из её буфера. Микрофон
/// в этот момент открыт и пишет комнату — то есть конец собственной фразы.
/// Расшифровка честно превращала его в вопрос, программа на него отвечала,
/// ответ снова попадал в микрофон, и разговор уходил сам с собой по кругу.
static MUTED_UNTIL: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Сколько глухоты оставлять после собственной речи.
///
/// Буфер звуковой системы на Windows — сотня-другая миллисекунд; берём с запасом
/// на медленные устройства вроде bluetooth-колонок, где задержка больше.
const DEAF_AFTER_SPEECH_MS: u64 = 700;

/// Поток, владеющий микрофоном. Создаётся при первом обращении.
fn commands() -> Option<&'static Sender<Command>> {
    COMMANDS
        .get_or_init(|| {
            let (tx, rx) = channel::<Command>();
            std::thread::Builder::new()
                .name("sufler-mic".into())
                .spawn(move || mic_loop(rx))
                .ok();
            tx
        })
        .into()
}

/// Единственное место, где живёт микрофон.
///
/// Команду ждём с коротким сроком, а не бесконечно: в разговоре надо ещё и
/// регулярно разбирать накопленный звук на фразы, и делать это надо здесь же —
/// буфер принадлежит этому потоку.
fn mic_loop(rx: Receiver<Command>) {
    let mut active: Option<(cpal::Stream, std::sync::Arc<Mutex<Vec<f32>>>, u32)> = None;
    let mut talk: Option<Segmenter> = None;

    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(Command::Start(preferred)) => {
                active = open(&preferred);
            }
            Ok(Command::Stop(reply)) => {
                let recording = match active.take() {
                    Some((stream, buffer, rate)) => {
                        // Прежде чем закрыть микрофон — подождать. Звук приходит
                        // порциями, и последняя в момент отпускания клавиши ещё
                        // в пути; закрывая устройство сразу, мы теряли хвост.
                        std::thread::sleep(std::time::Duration::from_millis(TAIL_MS));
                        drop(stream);
                        let samples =
                            std::mem::take(&mut *buffer.lock().unwrap_or_else(|e| e.into_inner()));
                        Recording {
                            samples,
                            sample_rate: rate,
                        }
                    }
                    None => Recording::default(),
                };
                let _ = reply.send(recording);
            }
            Ok(Command::StartTalk(preferred, mode, sender)) => {
                active = open(&preferred);
                talk = active
                    .as_ref()
                    .map(|(_, _, rate)| Segmenter::new(*rate, mode, sender));
            }
            Ok(Command::StopTalk) => {
                active = None;
                talk = None;
            }
            Err(RecvTimeoutError::Timeout) => {
                if let (Some((_, buffer, rate)), Some(segmenter)) = (&active, &mut talk) {
                    let chunk =
                        std::mem::take(&mut *buffer.lock().unwrap_or_else(|e| e.into_inner()));
                    segmenter.feed(&chunk, *rate);
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn open(preferred: &str) -> Option<(cpal::Stream, std::sync::Arc<Mutex<Vec<f32>>>, u32)> {
    match open_input(preferred) {
        Some((stream, buffer, rate)) => {
            if let Err(err) = stream.play() {
                log::warn!("микрофон не запустился: {err}");
            }
            Some((stream, buffer, rate))
        }
        None => {
            log::warn!("микрофон недоступен — записывать нечем");
            None
        }
    }
}

/// Какие устройства записи видит система.
///
/// Списком, потому что «основное устройство» и «то, в которое человек говорит»
/// — разные вещи чаще, чем кажется: виртуальные микрофоны от программ вроде
/// DroidCam и микрофоны геймпадов охотно занимают первое место и пишут тишину.
pub fn devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|list| {
            list.filter_map(|d| d.name().ok()).collect()
        })
        .unwrap_or_default()
}

/// Открывает устройство ввода и начинает складывать отсчёты в общий буфер.
fn open_input(preferred: &str) -> Option<(cpal::Stream, std::sync::Arc<Mutex<Vec<f32>>>, u32)> {
    let host = cpal::default_host();

    let device = if preferred.trim().is_empty() {
        host.default_input_device()?
    } else {
        // Названное устройство, а если его больше нет (отключили наушники) —
        // основное: молчать из-за пропавшего микрофона хуже, чем писать не с того.
        host.input_devices()
            .ok()?
            .find(|d| d.name().map(|name| name == preferred).unwrap_or(false))
            .or_else(|| host.default_input_device())?
    };

    let name = device.name().unwrap_or_else(|_| "без имени".into());
    let config = device.default_input_config().ok()?;
    log::info!(
        "пишу с устройства «{name}»: {} Гц, каналов {}",
        config.sample_rate().0,
        config.channels()
    );

    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let limit = rate as usize * MAX_SECONDS;

    let buffer = std::sync::Arc::new(Mutex::new(Vec::<f32>::with_capacity(rate as usize * 8)));
    let sink = buffer.clone();

    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut samples = sink.lock().unwrap_or_else(|err| err.into_inner());
                if samples.len() >= limit {
                    return;
                }
                // Микрофон может быть стереофоническим — сводим в моно. Речь
                // от этого не страдает, а данных вдвое меньше.
                if channels <= 1 {
                    samples.extend_from_slice(data);
                } else {
                    for frame in data.chunks(channels) {
                        let sum: f32 = frame.iter().sum();
                        samples.push(sum / channels as f32);
                    }
                }
            },
            |err| log::warn!("сбой записи: {err}"),
            None,
        )
        .ok()?;

    Some((stream, buffer, rate))
}

/* ── Запись удержанием клавиши ────────────────────────────────────────────── */

/// Начать запись с названного устройства (пусто — с основного).
pub fn start(device: &str) {
    if let Some(tx) = commands() {
        let _ = tx.send(Command::Start(device.to_string()));
    }
}

/// Закончить запись и отдать её в виде готового к распознаванию WAV.
///
/// Пустой результат — не ошибка: человек мог нажать и сразу отпустить, а
/// микрофона могло не оказаться вовсе.
pub fn stop() -> Option<Vec<u8>> {
    let tx = commands()?;
    let (reply, answer) = channel();
    tx.send(Command::Stop(reply)).ok()?;
    let recording = answer.recv().ok()?;

    // Меньше четверти секунды — это не вопрос, а случайное касание клавиши.
    if recording.samples.len() < recording.sample_rate as usize / 4 {
        return None;
    }
    prepare(recording.samples, recording.sample_rate, true, true)
}

/* ── Разговор без рук ─────────────────────────────────────────────────────── */

/// Начинает разговор: микрофон открыт, фразы приходят в возвращённый канал.
pub fn start_conversation(device: &str, mode: Listening) -> Option<Receiver<Heard>> {
    let tx = commands()?;
    let (utterances, rx) = channel();
    PAUSED.store(false, Ordering::Relaxed);
    // Разговор часто начинается сразу после того, как программа договорила
    // или её оборвали. Первые доли секунды слушать нечего, кроме её же хвоста.
    *MUTED_UNTIL.lock().unwrap_or_else(|err| err.into_inner()) =
        Some(std::time::Instant::now() + std::time::Duration::from_millis(DEAF_AFTER_SPEECH_MS));
    tx.send(Command::StartTalk(device.to_string(), mode, utterances))
        .ok()?;
    Some(rx)
}

pub fn stop_conversation() {
    PAUSED.store(false, Ordering::Relaxed);
    if let Some(tx) = commands() {
        let _ = tx.send(Command::StopTalk);
    }
}

/// Приостановить разговор: программа думает или говорит.
///
/// Не закрываем микрофон, а перестаём слушать: открытие устройства занимает
/// сотни миллисекунд, и человек, заговоривший сразу после ответа, оказался бы
/// не услышан.
pub fn pause_conversation(paused: bool) {
    PAUSED.store(paused, Ordering::Relaxed);
}

/// Находит границы фраз в непрерывном потоке звука.
///
/// Способ простой и старый: громкость по кадрам в 20 мс. Речь заметно громче
/// комнатного шума, а порог берётся не из головы, а от самого шума — в тихой
/// комнате и в шумной он получится разным. Отдельная модель определения речи
/// была бы точнее, но она весит десятки мегабайт и решает задачу, с которой
/// здесь справляется арифметика.
struct Segmenter {
    sender: Sender<Heard>,
    mode: Listening,
    rate: u32,
    utterance: Vec<f32>,
    speaking: bool,
    silence: usize,
    /// Сколько подряд молчим, когда фразы нет вовсе. По этому счётчику
    /// разговор заканчивается сам.
    quiet: usize,
    /// Сказали ли уже, что тишина затянулась. Один раз, а не каждый кадр.
    gave_up: bool,
    /// Оценка уровня шума. Обновляется только в тишине.
    noise: f32,
}

impl Segmenter {
    fn new(rate: u32, mode: Listening, sender: Sender<Heard>) -> Self {
        Self {
            sender,
            mode,
            rate,
            utterance: Vec::new(),
            speaking: false,
            silence: 0,
            quiet: 0,
            gave_up: false,
            noise: 0.005,
        }
    }

    fn feed(&mut self, chunk: &[f32], rate: u32) {
        if chunk.is_empty() {
            return;
        }

        // Пока говорит программа — не слушаем вовсе. Иначе микрофон запишет
        // её же голос из колонок, и она задаст вопрос сама себе.
        let now = std::time::Instant::now();
        if PAUSED.load(Ordering::Relaxed) || crate::voice::speaking() {
            // Отодвигаем глухоту на будущее: речь ещё доиграет после того, как
            // мы перестанем её отдавать.
            *MUTED_UNTIL.lock().unwrap_or_else(|err| err.into_inner()) =
                Some(now + std::time::Duration::from_millis(DEAF_AFTER_SPEECH_MS));
            self.reset();
            return;
        }

        // Речь кончилась, но хвост её ещё звучит — молчим до конца запаса.
        let muted = *MUTED_UNTIL.lock().unwrap_or_else(|err| err.into_inner());
        if muted.map(|until| now < until).unwrap_or(false) {
            self.reset();
            return;
        }

        let frame = (rate / 50).max(1) as usize;
        for part in chunk.chunks(frame) {
            let rms = (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt();

            if !self.speaking {
                // Шум оцениваем медленно и только пока молчим: иначе громкая
                // речь сама поднимет порог, и следующая фраза утонет.
                self.noise = self.noise * 0.97 + rms * 0.03;
            }
            let threshold = (self.noise * self.mode.over_noise()).max(self.mode.floor());

            if rms > threshold {
                self.speaking = true;
                self.silence = 0;
                self.quiet = 0;
                self.gave_up = false;
                self.utterance.extend_from_slice(part);
            } else if self.speaking {
                // Тишину внутри фразы тоже пишем: без неё слова склеились бы
                // в одно, и расшифровка стала бы хуже, а не лучше.
                self.silence += part.len();
                self.utterance.extend_from_slice(part);
                if ms(self.silence, rate) >= END_OF_PHRASE_MS {
                    self.finish();
                }
            } else {
                // Речи нет вовсе. Копим тишину: если она затянется, разговор
                // закончится сам.
                self.quiet += part.len();
                if !self.gave_up && ms(self.quiet, rate) >= SILENCE_ENDS_TALK_MS {
                    self.gave_up = true;
                    let _ = self.sender.send(Heard::LongSilence);
                }
            }
        }

        if self.utterance.len() > rate as usize * MAX_SECONDS {
            self.finish();
        }
    }

    /// Забыть всё, что успели услышать, и начать слушать заново.
    fn reset(&mut self) {
        self.utterance.clear();
        self.speaking = false;
        self.silence = 0;
        // Пока отвечали — человек молчал не от того, что ему нечего сказать.
        // Отсчёт тишины начинается заново с момента, когда он снова может
        // говорить, иначе разговор обрывался бы посреди длинного ответа.
        self.quiet = 0;
    }

    fn finish(&mut self) {
        let samples = std::mem::take(&mut self.utterance);
        self.speaking = false;
        let silence = std::mem::take(&mut self.silence);

        // Из длительности вычитаем хвостовую тишину: полсекунды кашля с
        // секундой тишины после — это не фраза.
        let speech = samples.len().saturating_sub(silence);
        if ms(speech, self.rate) < self.mode.min_speech_ms() {
            return;
        }

        if let Some(wav) = prepare(samples, self.rate, false, self.mode.verbose()) {
            let _ = self.sender.send(Heard::Phrase(wav));
        }
    }
}

fn ms(samples: usize, rate: u32) -> u64 {
    samples as u64 * 1000 / rate.max(1) as u64
}

/* ── Общая подготовка записи ──────────────────────────────────────────────── */

/// Приводит запись к тому виду, в котором её лучше всего разбирает whisper.
///
/// `complain` — ругаться ли в журнал на пустую запись. При удержании клавиши
/// это полезно (человек нажал и ничего не сказал), в разговоре — нет: там
/// тишина отсеивается раньше и является нормой.
fn prepare(samples: Vec<f32>, rate: u32, complain: bool, verbose: bool) -> Option<Vec<u8>> {
    let peak = samples.iter().fold(0.0_f32, |max, s| max.max(s.abs()));
    let seconds = samples.len() as f32 / rate.max(1) as f32;
    if verbose {
        log::info!("записано {seconds:.1} с, громкость {:.0}%", peak * 100.0);
    } else {
        log::debug!("записано {seconds:.1} с, громкость {:.0}%", peak * 100.0);
    }

    // Порог не «абсолютная тишина», а «ничего, кроме шума». На пустой записи
    // whisper не молчит, а выдумывает — «Продолжение следует...» и прочие титры
    // из роликов, на которых его учили. Лучше честно сказать, что не расслышали.
    if peak < SILENCE_LEVEL {
        if complain {
            log::warn!("в записи тишина — проверьте, тот ли микрофон выбран");
        }
        return None;
    }

    let mut samples = resample(&samples, rate, WHISPER_RATE);

    // Выравниваем громкость: только вверх и только тихие записи. Громкую
    // трогать нечем — она уже на пределе, а «прижать» её значило бы вносить
    // искажения там, где всё в порядке.
    if peak < TARGET_PEAK {
        let gain = (TARGET_PEAK / peak).min(MAX_GAIN);
        for sample in &mut samples {
            *sample *= gain;
        }
        if verbose {
            log::info!("тихая запись — подтянул громкость в {gain:.1} раза");
        }
    }

    samples.resize(
        samples.len() + (WHISPER_RATE as u64 * SILENCE_TAIL_MS / 1000) as usize,
        0.0,
    );
    Some(wav(&samples, WHISPER_RATE))
}

/// Приведение к нужной частоте линейной интерполяцией.
///
/// Для речи этого достаточно: полоса голоса заканчивается далеко ниже предела,
/// и слышимых искажений такой пересчёт не даёт. Полноценный ресемплер здесь
/// был бы отдельной зависимостью ради разницы, которой не услышит ни человек,
/// ни распознавание.
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    // Нулевая частота сюда прийти не должна, но если придёт — делить на неё
    // нельзя: длина получилась бы бесконечной, а запрос такой памяти уронил бы
    // поток голоса.
    if from == to || from == 0 || to == 0 || input.is_empty() {
        return input.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let len = (input.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(len);

    for i in 0..len {
        let position = i as f64 * ratio;
        let left = position.floor() as usize;
        let right = (left + 1).min(input.len() - 1);
        let weight = (position - left as f64) as f32;
        out.push(input[left] * (1.0 - weight) + input[right] * weight);
    }
    out
}

/// Собирает WAV: 16 бит, моно. Заголовок короткий и известный, отдельная
/// библиотека ради сорока четырёх байт не нужна.
fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // размер этого блока
    out.extend_from_slice(&1u16.to_le_bytes()); // без сжатия
    out.extend_from_slice(&1u16.to_le_bytes()); // моно
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // байт в секунду
    out.extend_from_slice(&2u16.to_le_bytes()); // байт на кадр
    out.extend_from_slice(&16u16.to_le_bytes()); // бит на отсчёт
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());

    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_halves_the_count_when_the_rate_halves() {
        let input: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        assert_eq!(resample(&input, 32_000, 16_000).len(), 50);
        // Та же частота — та же запись, без лишней работы.
        assert_eq!(resample(&input, 16_000, 16_000).len(), 100);
    }

    #[test]
    fn wav_header_says_what_follows() {
        let bytes = wav(&[0.0, 1.0, -1.0], 16_000);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[36..40], b"data");
        // Три отсчёта по два байта.
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 6);
        assert_eq!(bytes.len(), 44 + 6);
        // Единица и минус единица — края шкалы.
        assert_eq!(
            i16::from_le_bytes(bytes[46..48].try_into().unwrap()),
            i16::MAX
        );
    }

    #[test]
    fn zero_rate_does_not_blow_up() {
        let input = vec![0.1_f32; 10];
        assert_eq!(resample(&input, 0, 16_000).len(), 10);
        assert_eq!(resample(&input, 16_000, 0).len(), 10);
    }

    #[test]
    fn duration_is_counted_in_milliseconds() {
        assert_eq!(ms(16_000, 16_000), 1000);
        assert_eq!(ms(8_000, 16_000), 500);
        assert_eq!(ms(0, 16_000), 0);
    }

    #[test]
    fn silence_alone_is_not_a_phrase() {
        let (tx, rx) = channel();
        let mut segmenter = Segmenter::new(16_000, Listening::Talk, tx);
        // Секунда тишины: ни начала речи, ни фразы на выходе.
        segmenter.feed(&vec![0.0_f32; 16_000], 16_000);
        assert!(matches!(rx.try_recv(), Err(_)));
        assert!(!segmenter.speaking);
    }

    #[test]
    fn loud_stretch_followed_by_pause_becomes_a_phrase() {
        let (tx, rx) = channel();
        let mut segmenter = Segmenter::new(16_000, Listening::Talk, tx);

        // Полсекунды «речи»: чередующийся сигнал заметно громче порога.
        let speech: Vec<f32> = (0..8_000)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        segmenter.feed(&speech, 16_000);
        assert!(segmenter.speaking, "громкий отрезок должен считаться речью");

        // Секунда тишины после неё закрывает фразу.
        segmenter.feed(&vec![0.0_f32; 16_000], 16_000);
        assert!(
            matches!(rx.try_recv(), Ok(Heard::Phrase(_))),
            "фраза должна была уйти в канал"
        );
    }

    #[test]
    fn long_silence_ends_the_talk_once() {
        let (tx, rx) = channel();
        let mut segmenter = Segmenter::new(16_000, Listening::Talk, tx);

        // Тишины ровно столько, сколько отведено, плюс секунда. Считаем от
        // константы, а не числом: иначе тест разойдётся с кодом при первой же
        // правке порога — что уже однажды и случилось.
        let silence = 16_000 * (SILENCE_ENDS_TALK_MS / 1000 + 1) as usize;
        segmenter.feed(&vec![0.0_f32; silence], 16_000);
        assert!(
            matches!(rx.try_recv(), Ok(Heard::LongSilence)),
            "разговор должен был закончиться сам"
        );

        // И только один раз: следующая тишина уже ничего не шлёт.
        segmenter.feed(&vec![0.0_f32; silence], 16_000);
        assert!(rx.try_recv().is_err(), "сказано должно быть один раз");
    }
}
