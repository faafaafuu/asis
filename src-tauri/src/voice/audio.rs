//! Воспроизведение звука: один поток на всё приложение.
//!
//! Почему отдельный поток, а не вызов из любого места. Устройство вывода в rodio
//! держит поток cpal, который нельзя перемещать между потоками, а озвучивание
//! зовётся из асинхронных задач Tauri, живущих где придётся. Отдельный владелец
//! снимает вопрос целиком: наружу торчит только канал команд.
//!
//! Заодно это решает остановку. «Замолчи» — не отдельное умение синтезатора,
//! а просто очистка очереди воспроизведения, и очередь должна быть одна: иначе
//! два объяснения подряд заговорили бы хором.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};

pub enum Command {
    /// Кусок готового звука: моно, значения от -1 до 1.
    Play { samples: Vec<f32>, sample_rate: u32 },
    /// Оборвать всё, что играет и что стоит в очереди.
    Stop,
}

static SENDER: OnceLock<Sender<Command>> = OnceLock::new();

/// До какого момента звук, уже отданный на воспроизведение, будет слышен.
///
/// Не «когда отдавали в последний раз», как было раньше. Разница решающая:
/// синтез идёт вдесятеро быстрее речи, поэтому вся минутная фраза попадает
/// в очередь за несколько секунд. Признак «отдавали недавно» гас на середине
/// фразы — и программа начинала слушать микрофон, пока сама ещё говорила,
/// а потом прилежно расшифровывала собственный голос и отвечала на него.
///
/// Здесь копится именно длительность: каждый кусок добавляет к сроку столько,
/// сколько он звучит.
static PLAYS_UNTIL: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Громкость речи по отрезкам: когда какой кусок будет звучать и насколько он
/// громкий.
///
/// Нужна индикатору. Кольцо должно шевелиться в такт голосу, а не жить своей
/// придуманной жизнью: несинхронная анимация читается как заставка, синхронная —
/// как то, что программа действительно говорит.
///
/// Заполняется наперёд, потому что звук отдаётся в очередь заранее: к моменту,
/// когда кусок зазвучит, считать его громкость будет поздно.
static ENVELOPE: Mutex<Vec<(std::time::Instant, f32)>> = Mutex::new(Vec::new());

/// Насколько громко звучит речь прямо сейчас: от 0 до 1.
pub fn level() -> f32 {
    let now = std::time::Instant::now();
    let mut envelope = ENVELOPE.lock().unwrap_or_else(|err| err.into_inner());

    // Прошедшее выбрасываем: дорожка не должна расти всю дорогу.
    envelope.retain(|(at, _)| *at + std::time::Duration::from_millis(SEGMENT_MS) > now);

    envelope
        .iter()
        .find(|(at, _)| *at <= now)
        .map(|(_, level)| *level)
        .unwrap_or(0.0)
}

/// На какие отрезки режем звук, считая громкость. Пятьдесят миллисекунд — это
/// двадцать измерений в секунду: чаще человеческий глаз в анимации не различит,
/// реже — движение отстаёт от речи.
const SEGMENT_MS: u64 = 50;

/// Слышен ли сейчас наш голос из колонок.
pub fn speaking() -> bool {
    PLAYS_UNTIL
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .map(|until| std::time::Instant::now() < until)
        .unwrap_or(false)
}

/// Канал к аудиопотоку. Поток создаётся при первом обращении и живёт дальше.
pub fn sender() -> Option<&'static Sender<Command>> {
    SENDER
        .get_or_init(|| {
            let (tx, rx) = channel::<Command>();
            std::thread::Builder::new()
                .name("sufler-audio".into())
                .spawn(move || play_loop(rx))
                .ok();
            tx
        })
        .into()
}

/// Единственное место, где открыто устройство вывода.
///
/// Устройство открывается на время фразы и закрывается вместе с ней — а не один
/// раз на всю жизнь программы, как было. Причин две, и обе наблюдались вживую.
///
/// Первая: устройство вывода меняется под ногами. Bluetooth-колонка засыпает,
/// наушники выдёргивают, звук уходит на HDMI — открытый когда-то поток остаётся
/// связан с исчезнувшим устройством. Ошибки при этом нет: очередь исправно
/// принимает звук, который никто уже не услышит. Со стороны это «голос
/// отвалился и больше не работает».
///
/// Вторая: `Sink::clear()`, которым раньше обрывалась фраза, внутри ждёт, пока
/// доиграет очередь. На мёртвом устройстве она не доиграет никогда — и поток
/// вставал намертво вместе со всем звуком приложения. Здесь очередь не чистится,
/// а уничтожается вместе с устройством: это и мгновенно, и не может зависнуть.
fn play_loop(rx: std::sync::mpsc::Receiver<Command>) {
    let mut output: Option<(rodio::OutputStream, rodio::Sink)> = None;

    for command in rx {
        match command {
            Command::Play {
                samples,
                sample_rate,
            } => {
                if output.is_none() {
                    output = open_output();
                }
                let Some((_, sink)) = &output else { continue };
                sink.append(rodio::buffer::SamplesBuffer::new(1, sample_rate, samples));
                sink.play();
            }
            // Уронить очередь вместе с устройством — самый надёжный способ
            // замолчать: следующая фраза откроет то устройство, которое к тому
            // времени будет основным.
            Command::Stop => output = None,
        }
    }
}

fn open_output() -> Option<(rodio::OutputStream, rodio::Sink)> {
    match rodio::OutputStreamBuilder::open_default_stream() {
        Ok(stream) => {
            let sink = rodio::Sink::connect_new(stream.mixer());
            Some((stream, sink))
        }
        Err(err) => {
            // Наушники не воткнуты, устройства нет, драйвер отвалился.
            // Молча не озвучиваем — программа продолжает работать.
            log::warn!("звук недоступен: {err}");
            None
        }
    }
}

/// Ставит кусок звука в очередь.
pub fn play(samples: Vec<f32>, sample_rate: u32) {
    // Срок звучания продлеваем на длительность куска. Если предыдущий ещё
    // не доиграл — считаем от его конца, иначе от текущего момента.
    let starts_at = {
        let seconds = samples.len() as f64 / f64::from(sample_rate.max(1));
        let mut until = PLAYS_UNTIL.lock().unwrap_or_else(|err| err.into_inner());
        let now = std::time::Instant::now();
        let from = until.filter(|end| *end > now).unwrap_or(now);
        *until = Some(from + std::time::Duration::from_secs_f64(seconds));
        from
    };

    // Раскладываем кусок на отрезки и запоминаем громкость каждого — вместе
    // с тем, когда он зазвучит.
    {
        let per_segment = (sample_rate as u64 * SEGMENT_MS / 1000).max(1) as usize;
        let mut envelope = ENVELOPE.lock().unwrap_or_else(|err| err.into_inner());
        for (index, part) in samples.chunks(per_segment).enumerate() {
            let sum: f32 = part.iter().map(|s| s * s).sum();
            let rms = (sum / part.len() as f32).sqrt();
            // Речь редко превышает четверть шкалы, а показать надо весь размах:
            // растягиваем, но не даём выйти за единицу.
            let level = (rms * 3.5).min(1.0);
            let at = starts_at + std::time::Duration::from_millis(SEGMENT_MS * index as u64);
            envelope.push((at, level));
        }
        // Порядок по времени: `level()` берёт первый подходящий отрезок.
        envelope.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    }
    if let Some(tx) = sender() {
        let _ = tx.send(Command::Play {
            samples,
            sample_rate,
        });
    }
}

/// Короткий сигнал: «голосовой помощник закончил».
///
/// Готовый звук, а не синтезированный: синтезом получается пищалка, сколько её
/// ни украшай — чистые тоны слух опознаёт как сигнал бытовой техники. Файл
/// подобран человеком, обрезан до полусекунды полезного звучания и выровнен по
/// громкости.
///
/// Лежит прямо в исполняемом файле: семнадцать килобайт не стоят того, чтобы
/// класть их отдельным ресурсом, который надо найти на диске, а при неудачном
/// стечении обстоятельств — не найти.
const CHIME: &[u8] = include_bytes!("../../assets/chime.wav");

pub fn chime() {
    match wav_samples(CHIME) {
        Some((samples, rate)) => play(samples, rate),
        None => log::warn!("сигнал завершения не разобрался — пропускаю"),
    }
}

/// Достаёт отсчёты из WAV: 16 бит, моно.
///
/// Свой разбор вместо библиотеки: формат наш собственный, файл один и известен
/// заранее. Чанки всё же перебираем честно, а не читаем с 44-го байта: между
/// заголовком и звуком редакторы любят класть свои служебные блоки, и жёсткое
/// смещение однажды прочитало бы вместо звука их.
fn wav_samples(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }

    let mut rate = None;
    let mut position = 12;

    while position + 8 <= bytes.len() {
        let id = &bytes[position..position + 4];
        let size = u32::from_le_bytes(bytes[position + 4..position + 8].try_into().ok()?) as usize;
        let body = position + 8;
        if body + size > bytes.len() {
            return None;
        }

        match id {
            b"fmt " if size >= 16 => {
                rate = Some(u32::from_le_bytes(
                    bytes[body + 4..body + 8].try_into().ok()?,
                ));
            }
            b"data" => {
                let samples = bytes[body..body + size]
                    .chunks_exact(2)
                    .map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0)
                    .collect();
                return Some((samples, rate?));
            }
            _ => {}
        }
        // Нечётные чанки дополняются байтом до чётной длины.
        position = body + size + (size & 1);
    }
    None
}

/// Обрывает воспроизведение.
pub fn stop() {
    *PLAYS_UNTIL.lock().unwrap_or_else(|err| err.into_inner()) = None;
    ENVELOPE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clear();
    if let Some(tx) = sender() {
        let _ = tx.send(Command::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_signal_is_readable() {
        let (samples, rate) = wav_samples(CHIME).expect("сигнал должен разбираться");
        assert_eq!(rate, 22_050);
        // Полсекунды с запасом: если файл подменят на трёхсекундный, тест скажет.
        assert!(samples.len() > 1_000 && samples.len() < 22_050);
        // И это должен быть звук, а не тишина.
        let peak = samples.iter().fold(0.0_f32, |max, s| max.max(s.abs()));
        assert!(peak > 0.5, "сигнал слишком тихий: {peak}");
    }

    #[test]
    fn garbage_is_not_mistaken_for_sound() {
        assert!(wav_samples(b"").is_none());
        assert!(wav_samples(b"RIFF....WAVE").is_none());
    }
}
