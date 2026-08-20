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

/// Слышен ли сейчас наш голос из колонок.
pub fn speaking() -> bool {
    PLAYS_UNTIL
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .map(|until| std::time::Instant::now() < until)
        .unwrap_or(false)
}

/// Канал к аудиопотоку. Поток создаётся при первом обращении и живёт дальше:
/// открытие устройства вывода занимает десятки миллисекунд, и платить их на
/// каждую фразу незачем.
pub fn sender() -> Option<&'static Sender<Command>> {
    SENDER
        .get_or_init(|| {
            let (tx, rx) = channel::<Command>();
            std::thread::Builder::new()
                .name("sufler-audio".into())
                .spawn(move || {
                    let stream = match rodio::OutputStreamBuilder::open_default_stream() {
                        Ok(stream) => stream,
                        Err(err) => {
                            // Наушники не воткнуты, устройства нет, драйвер отвалился.
                            // Молча не озвучиваем — программа продолжает работать.
                            log::warn!("звук недоступен: {err}");
                            return;
                        }
                    };
                    let sink = rodio::Sink::connect_new(stream.mixer());

                    for command in rx {
                        match command {
                            Command::Play {
                                samples,
                                sample_rate,
                            } => {
                                sink.append(rodio::buffer::SamplesBuffer::new(
                                    1,
                                    sample_rate,
                                    samples,
                                ));
                                // Обязательно после каждой порции, и вот почему.
                                //
                                // `clear()` ниже не просто выбрасывает очередь —
                                // он ещё и ставит воспроизведение на паузу. А
                                // остановка вызывается в начале КАЖДОЙ фразы,
                                // чтобы оборвать предыдущую. Без этой строки
                                // первый же вызов глушил звук навсегда: всё
                                // дальнейшее исправно складывалось в очередь
                                // поставленного на паузу проигрывателя, и
                                // программа молчала, не сообщая об ошибке.
                                sink.play();
                            }
                            Command::Stop => sink.clear(),
                        }
                    }
                })
                .ok();
            tx
        })
        .into()
}

/// Ставит кусок звука в очередь.
pub fn play(samples: Vec<f32>, sample_rate: u32) {
    // Срок звучания продлеваем на длительность куска. Если предыдущий ещё
    // не доиграл — считаем от его конца, иначе от текущего момента.
    {
        let seconds = samples.len() as f64 / f64::from(sample_rate.max(1));
        let mut until = PLAYS_UNTIL.lock().unwrap_or_else(|err| err.into_inner());
        let now = std::time::Instant::now();
        let from = until.filter(|end| *end > now).unwrap_or(now);
        *until = Some(from + std::time::Duration::from_secs_f64(seconds));
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
/// Синтезируем, а не носим с собой файл: полтораста миллисекунд синусоиды —
/// это двадцать строк арифметики против отдельного ресурса в установщике,
/// который надо где-то взять, куда-то положить и не забыть про лицензию.
///
/// Две ноты сверху вниз с быстрым затуханием. Вниз — потому что нисходящий
/// интервал слышится как завершение, восходящий — как начало; это не вкусовое
/// решение, а то, как устроен слух.
pub fn chime() {
    const RATE: u32 = 22_050;
    // Ми шестой октавы и си пятой: чистая кварта, звучит нейтрально-технично,
    // без мажорной радости и минорной печали.
    const TONES: [(f32, f32); 2] = [(1318.5, 0.07), (987.8, 0.13)];

    let mut samples: Vec<f32> = Vec::new();
    for (hz, seconds) in TONES {
        let count = (RATE as f32 * seconds) as usize;
        for i in 0..count {
            let time = i as f32 / RATE as f32;
            // Мгновенная атака щёлкает, поэтому первые пять миллисекунд —
            // нарастание, дальше экспоненциальное затухание.
            let attack = (time / 0.005).min(1.0);
            let decay = (-time * 14.0).exp();
            let wave = (time * hz * std::f32::consts::TAU).sin();
            samples.push(wave * attack * decay * 0.22);
        }
    }
    play(samples, RATE);
}

/// Обрывает воспроизведение.
pub fn stop() {
    *PLAYS_UNTIL.lock().unwrap_or_else(|err| err.into_inner()) = None;
    if let Some(tx) = sender() {
        let _ = tx.send(Command::Stop);
    }
}
