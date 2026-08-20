//! Распознавание речи: whisper.cpp на этом же компьютере.
//!
//! Тот же приём, что и с озвучиванием: не библиотека, а готовая программа рядом.
//! Собрать whisper.cpp из исходников означало бы CMake и, для видеокарты, ещё и
//! CUDA-тулчейн — несколько гигабайт сборочного хозяйства у каждого, кто просто
//! хочет продиктовать вопрос. Готовые сборки уже собраны и с тем и с другим.
//!
//! Запись идёт своим потоком, который владеет устройством ввода: поток cpal
//! нельзя передавать между потоками, а начинают и заканчивают запись по нажатию
//! клавиши, то есть откуда угодно.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Частота, на которой работает whisper. Любую другую он всё равно приведёт
/// к этой, но лучше сделать это самим: передавать по сети процессов лишние
/// сто килобайт в секунду незачем.
const WHISPER_RATE: u32 = 16_000;

/// Дольше этого не пишем.
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

enum Command {
    /// Имя устройства записи. Пусто — то, что система считает основным.
    Start(String),
    /// Закончить и отдать записанное.
    Stop(Sender<Recording>),
}

#[derive(Default)]
struct Recording {
    samples: Vec<f32>,
    sample_rate: u32,
}

static COMMANDS: OnceLock<Sender<Command>> = OnceLock::new();

/// Поток, владеющий микрофоном. Создаётся при первом обращении.
fn commands() -> Option<&'static Sender<Command>> {
    COMMANDS
        .get_or_init(|| {
            let (tx, rx) = channel::<Command>();
            std::thread::Builder::new()
                .name("sufler-mic".into())
                .spawn(move || {
                    let mut active: Option<(cpal::Stream, std::sync::Arc<Mutex<Vec<f32>>>, u32)> =
                        None;

                    for command in rx {
                        match command {
                            Command::Start(preferred) => {
                                active = open_input(&preferred).map(|(stream, buffer, rate)| {
                                    if let Err(err) = stream.play() {
                                        log::warn!("микрофон не запустился: {err}");
                                    }
                                    (stream, buffer, rate)
                                });
                                if active.is_none() {
                                    log::warn!("микрофон недоступен — записывать нечем");
                                }
                            }
                            Command::Stop(reply) => {
                                let recording = match active.take() {
                                    Some((stream, buffer, rate)) => {
                                        // Прежде чем закрыть микрофон — подождать.
                                        //
                                        // Звук приходит не по одному отсчёту, а
                                        // порциями, и последняя порция в момент
                                        // отпускания клавиши ещё в пути. Закрывая
                                        // устройство сразу, мы теряли хвост фразы:
                                        // человек договаривал слово, а в записи его
                                        // уже не было. Треть секунды с запасом
                                        // перекрывает размер порции у любого
                                        // разумного устройства — но не запас
                                        // на человека: клавишу отпускают на
                                        // последнем слоге, а то и раньше него.
                                        std::thread::sleep(
                                            std::time::Duration::from_millis(TAIL_MS),
                                        );
                                        drop(stream);
                                        let samples = std::mem::take(
                                            &mut *buffer
                                                .lock()
                                                .unwrap_or_else(|err| err.into_inner()),
                                        );
                                        Recording {
                                            samples,
                                            sample_rate: rate,
                                        }
                                    }
                                    None => Recording::default(),
                                };
                                let _ = reply.send(recording);
                            }
                        }
                    }
                })
                .ok();
            tx
        })
        .into()
}

/// Какие устройства записи видит система.
///
/// Списком, потому что «основное устройство» и «то, в которое человек говорит»
/// — разные вещи чаще, чем кажется: виртуальные микрофоны от программ вроде
/// DroidCam и микрофоны геймпадов охотно занимают первое место и пишут тишину.
pub fn devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|list| list.filter_map(|d| d.description().ok().map(|d| d.name().to_string())).collect())
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
            .find(|d| d.description().map(|d| d.name() == preferred).unwrap_or(false))
            .or_else(|| host.default_input_device())?
    };

    let name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "без имени".into());
    let config = device.default_input_config().ok()?;
    log::info!(
        "пишу с устройства «{name}»: {} Гц, каналов {}",
        config.sample_rate(),
        config.channels()
    );
    // В cpal 0.17 частота отдаётся уже числом, без обёртки.
    let rate = config.sample_rate();
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

    // Громкость самого громкого места. Без неё «не расслышало» и «микрофон
    // пишет тишину» выглядят в журнале одинаково, а чинятся по-разному.
    let peak = recording
        .samples
        .iter()
        .fold(0.0_f32, |max, s| max.max(s.abs()));
    let seconds = recording.samples.len() as f32 / recording.sample_rate.max(1) as f32;
    log::info!("записано {seconds:.1} с, громкость {:.0}%", peak * 100.0);

    // Меньше четверти секунды — это не вопрос, а случайное касание клавиши.
    if recording.samples.len() < recording.sample_rate as usize / 4 {
        return None;
    }

    // Порог не «абсолютная тишина», а «ничего, кроме шума». На пустой записи
    // whisper не молчит, а выдумывает — «Продолжение следует...» и прочие титры
    // из роликов, на которых его учили. Лучше честно сказать, что не расслышали.
    if peak < SILENCE_LEVEL {
        log::warn!("в записи тишина — проверьте, тот ли микрофон выбран");
        return None;
    }

    let mut samples = resample(&recording.samples, recording.sample_rate, WHISPER_RATE);

    // Выравниваем громкость: только вверх и только тихие записи. Громкую
    // трогать нечем — она уже на пределе, а «прижать» её значило бы вносить
    // искажения там, где всё в порядке.
    if peak > SILENCE_LEVEL && peak < TARGET_PEAK {
        let gain = TARGET_PEAK / peak;
        for sample in &mut samples {
            *sample *= gain;
        }
        log::info!("тихая запись — подтянул громкость в {gain:.1} раза");
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
    if from == to || input.is_empty() {
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
        assert_eq!(i16::from_le_bytes(bytes[46..48].try_into().unwrap()), i16::MAX);
    }
}
