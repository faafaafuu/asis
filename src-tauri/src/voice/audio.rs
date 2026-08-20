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
use std::sync::OnceLock;

pub enum Command {
    /// Кусок готового звука: моно, значения от -1 до 1.
    Play { samples: Vec<f32>, sample_rate: u32 },
    /// Оборвать всё, что играет и что стоит в очереди.
    Stop,
}

static SENDER: OnceLock<Sender<Command>> = OnceLock::new();

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
    if let Some(tx) = sender() {
        let _ = tx.send(Command::Play {
            samples,
            sample_rate,
        });
    }
}

/// Обрывает воспроизведение.
pub fn stop() {
    if let Some(tx) = sender() {
        let _ = tx.send(Command::Stop);
    }
}
