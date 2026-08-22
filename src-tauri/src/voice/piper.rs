//! Озвучивание своим голосом: Piper на этом же компьютере.
//!
//! Piper — отдельная программа, а не библиотека, и это осознанный выбор. Внутри
//! у неё ONNX Runtime и espeak-ng; тащить их в сборку означало бы C-тулчейн,
//! CMake и полтораста мегабайт в установщике ради возможности, которая нужна не
//! всем. Запуск программы рядом — ровно тот же приём, которым в проекте уже
//! поднимается Ollama: скачали при первом включении голоса, дальше зовём.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use tauri::AppHandle;

use super::{assets, audio};

/// Работающий сейчас синтез. Нужен, чтобы «замолчи» останавливало не только звук,
/// но и саму работу: Piper продолжал бы синтезировать уже ненужный текст, занимая
/// процессор, а следующая фраза ждала бы очереди.
static CURRENT: Mutex<Option<(u64, Child)>> = Mutex::new(None);

/// Номер текущего озвучивания.
///
/// Нужен, потому что фразы сменяют друг друга: человек нажал пробел на новом
/// объяснении, не дослушав старое. Без номера поток, читавший предыдущую фразу,
/// доходил до конца и убирал за собой чужого, уже нового ребёнка — а заодно
/// вставал на `wait()` до конца новой фразы, держа замок. Со стороны это
/// выглядело как «озвучка работает через раз».
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Сколько звука забираем за раз.
///
/// 8192 отсчёта при 22050 Гц — примерно треть секунды. Мельче дробить незачем:
/// каждый кусок это отдельная запись в очередь воспроизведения. Крупнее —
/// заметна задержка перед первым словом.
const CHUNK: usize = 8192 * 2;

/// Читает частоту дискретизации из описания голоса.
///
/// У каждого голоса своя: medium — 22050 Гц, high — 44100. Ошибиться здесь
/// нельзя: звук воспроизведётся с неправильной скоростью, и голос станет то
/// басом, то писком.
fn sample_rate(json: &std::path::Path) -> u32 {
    const FALLBACK: u32 = 22_050;

    let Ok(raw) = std::fs::read_to_string(json) else {
        return FALLBACK;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v["audio"]["sample_rate"].as_u64())
        .map(|rate| rate as u32)
        .unwrap_or(FALLBACK)
}

/// Говорит текст. Возвращается сразу: синтез и воспроизведение идут своим потоком.
pub fn speak(app: &AppHandle, voice: &str, rate: f32, text: &str) -> Result<(), String> {
    // Пустое имя голоса — не повод молчать: в настройках его могли стереть,
    // а умолчание известно.
    let voice = if voice.trim().is_empty() {
        assets::DEFAULT_VOICE
    } else {
        voice
    };

    let exe = assets::piper_exe(app)?;
    let model = assets::voice_path(app, voice)?;
    if !exe.exists() || !model.exists() {
        return Err("голос ещё не скачан".into());
    }

    // Предыдущая фраза отменяется молча. Человек нажал «прочитай» на новом
    // объяснении — старое ему уже не нужно, и дослушивать его он не собирается.
    stop();
    let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

    // Имя не `rate`: так зовётся скорость речи, и затенить её здесь означало бы
    // подставить в неё частоту дискретизации.
    let hz = sample_rate(&assets::json_beside(&model));

    let mut command = Command::new(&exe);
    command
        .arg("--model")
        .arg(&model)
        // Сырой звук в поток вывода, а не файл на диске: так первые слова
        // начинают звучать, пока конец фразы ещё синтезируется.
        //
        // Имя флага именно с подчёркиванием — так у Piper 2023.11.
        .arg("--output_raw")
        // Скорость наоборот: параметр задаёт длительность звуков, а не темп.
        // Просят вдвое быстрее — значит каждый звук вдвое короче.
        .arg("--length_scale")
        .arg(format!("{:.3}", length_scale(rate)))
        // Живость. Оба числа отвечают за разброс: первое — за интонацию в целом,
        // второе — за длительность отдельных звуков. На умолчаниях (0.667 и 0.8)
        // голос ровный до безжизненности: каждая фраза читается одинаково. Чуть
        // больший разброс даёт естественные колебания темпа и высоты — то, чем
        // живая речь отличается от диктора-автомата. Ещё выше поднимать нельзя:
        // начинает «плыть» произношение.
        .arg("--noise_scale")
        .arg("0.78")
        .arg("--noise_w")
        .arg("0.95")
        // Пауза между предложениями. Умолчание 0.2 с — речь звучит скороговоркой;
        // чуть длиннее пауза читается как осмысленная, а не как заминка.
        .arg("--sentence_silence")
        .arg("0.35")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Piper подробно рассказывает о себе в поток ошибок на каждую фразу.
        // В журнале приложения это только шум.
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("не удалось запустить синтезатор: {err}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or("синтезатор не принимает текст")?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("синтезатор не отдаёт звук")?;

    // Текст одной строкой: перевод строки для Piper — конец задания.
    let line = format!("{}\n", text.replace('\n', " "));
    std::thread::spawn(move || {
        let _ = stdin.write_all(line.as_bytes());
        // Закрыть вход обязательно: пока он открыт, Piper ждёт продолжения
        // и не заканчивает работу.
        drop(stdin);
    });

    crate::jobs::adopt(&child);
    *CURRENT.lock().unwrap_or_else(|err| err.into_inner()) = Some((generation, child));

    std::thread::Builder::new()
        .name("sufler-tts".into())
        .spawn(move || {
            let mut buffer = vec![0u8; CHUNK];
            let mut tail: Option<u8> = None;
            // Дочитали ли до конца. Если нет, процесс придётся снять силой —
            // почему, объясняется ниже, у самого снятия.
            let mut drained = false;

            loop {
                // Нас отменили — дальше читать нечего: звук уже никому не нужен,
                // а лить его в общую очередь поверх новой фразы тем более.
                if GENERATION.load(std::sync::atomic::Ordering::SeqCst) != generation {
                    break;
                }
                match stdout.read(&mut buffer) {
                    Ok(0) => {
                        drained = true;
                        break;
                    }
                    Ok(read) => {
                        let mut samples = Vec::with_capacity(read / 2 + 1);
                        let mut bytes = buffer[..read].iter().copied();

                        // Кусок мог оборваться на середине отсчёта: он из двух
                        // байтов, а чтение возвращает сколько придётся. Половинку
                        // держим до следующего круга, иначе звук пойдёт с треском.
                        if let Some(low) = tail.take() {
                            if let Some(high) = bytes.next() {
                                samples.push(pcm(low, high));
                            } else {
                                tail = Some(low);
                            }
                        }
                        loop {
                            let Some(low) = bytes.next() else { break };
                            let Some(high) = bytes.next() else {
                                tail = Some(low);
                                break;
                            };
                            samples.push(pcm(low, high));
                        }

                        if !samples.is_empty() {
                            audio::play(samples, hz);
                        }
                    }
                    Err(_) => break,
                }
            }

            // Снимаем с учёта только себя. Если за это время началась новая
            // фраза, в CURRENT уже её процесс — трогать его нельзя.
            //
            // Забираем процесс из-под замка и отпускаем замок сразу же. Ждать
            // завершения, держа замок, нельзя: в это время замок спрашивает
            // `stop()`, а он зовётся и из главного потока — и тот встаёт вместе
            // со всем окном. Вживую это и происходило: «программа не отвечает»
            // после нескольких фраз подряд.
            let mut child = {
                let mut current = CURRENT.lock().unwrap_or_else(|err| err.into_inner());
                let mine = current
                    .as_ref()
                    .map(|(id, _)| *id == generation)
                    .unwrap_or(false);
                if mine {
                    current.take().map(|(_, child)| child)
                } else {
                    None
                }
            };

            if let Some(child) = child.as_mut() {
                // Оборвались на середине — Piper снимаем силой.
                //
                // Он пишет звук в канал, который читали только мы. Перестав
                // читать, мы оставляем канал переполненным: Piper висит на
                // записи и сам не завершится никогда, а `wait()` будет ждать
                // его вечно. Так и получалась мёртвая пара — этот поток ждёт
                // процесс, главный поток ждёт этот поток.
                if !drained {
                    let _ = child.kill();
                }
                let _ = child.wait();
            }
        })
        .map_err(|err| format!("не удалось начать озвучивание: {err}"))?;

    Ok(())
}

/// Скорость речи в длительность звука.
///
/// Пределы не каприз: ниже 0.5 речь растягивается до неразборчивого воя, выше
/// 2.0 — сливается в скороговорку. Ноль и отрицательное сюда прийти не должны,
/// но настройки читаются с диска, и файл мог отредактировать кто угодно.
fn length_scale(rate: f32) -> f32 {
    1.0 / rate.clamp(0.5, 2.0)
}

/// Два байта в отсчёт: Piper отдаёт 16 бит со знаком, младший байт первым.
fn pcm(low: u8, high: u8) -> f32 {
    f32::from(i16::from_le_bytes([low, high])) / 32768.0
}

/// Прекращает и синтез, и воспроизведение.
pub fn stop() {
    audio::stop();
    // Отменяем текущее поколение: читающий поток увидит это и выйдет сам.
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    // Замок отпускается на закрывающей скобке блока — до `kill()`, а не после.
    // Иначе замок держался бы всё время снятия процесса, а его в этот же миг
    // спрашивает читающий поток.
    let child = CURRENT
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .take();

    if let Some((_, mut child)) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_becomes_sound_length() {
        assert_eq!(length_scale(1.0), 1.0);
        assert_eq!(length_scale(2.0), 0.5);
        // За пределами — прижимается к границе, а не уходит в бессмыслицу.
        assert_eq!(length_scale(10.0), 0.5);
        assert_eq!(length_scale(0.0), 2.0);
        assert_eq!(length_scale(-1.0), 2.0);
    }

    #[test]
    fn bytes_become_samples_in_the_right_order() {
        // 0x0000 — тишина, 0x7FFF — верхний предел, 0x8000 — нижний.
        assert_eq!(pcm(0x00, 0x00), 0.0);
        assert!((pcm(0xFF, 0x7F) - 0.999_97).abs() < 0.001);
        assert_eq!(pcm(0x00, 0x80), -1.0);
    }
}
