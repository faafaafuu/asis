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

/// Когда в последний раз отдавали звук на воспроизведение.
///
/// Нужно, чтобы отличить «программа сейчас говорит» от «молчит». Спросить об
/// этом сам проигрыватель нельзя: он живёт в своём потоке. А знать надо: если
/// включить микрофон, пока из колонок идёт речь, он запишет её, и расшифровка
/// приклеит к вопросу человека пару слов, сказанных программой.
static LAST_PLAY: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Похоже ли, что прямо сейчас из колонок идёт наша речь.
///
/// Звук отдаётся кусками по трети секунды подряд, пока фраза не кончится, —
/// поэтому «последний кусок был меньше секунды назад» означает, что речь идёт
/// либо только что кончилась и ещё доигрывает в буфере звуковой системы.
pub fn speaking() -> bool {
    LAST_PLAY
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .map(|at| at.elapsed() < std::time::Duration::from_secs(1))
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
    *LAST_PLAY.lock().unwrap_or_else(|err| err.into_inner()) = Some(std::time::Instant::now());
    if let Some(tx) = sender() {
        let _ = tx.send(Command::Play {
            samples,
            sample_rate,
        });
    }
}

/// Обрывает воспроизведение.
pub fn stop() {
    *LAST_PLAY.lock().unwrap_or_else(|err| err.into_inner()) = None;
    if let Some(tx) = sender() {
        let _ = tx.send(Command::Stop);
    }
}
