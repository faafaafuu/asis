//! Нейроголоса Microsoft через службу Azure Speech.
//!
//! Те самые голоса, которыми читает вслух Edge (`ru-RU-SvetlanaNeural`,
//! `ru-RU-DmitryNeural`), но через официальный REST-интерфейс по ключу: текст
//! уходит запросом, обратно приходит готовый WAV. На бесплатном тарифе F0 —
//! полмиллиона знаков в месяц.
//!
//! Сеть и чужой сервер, поэтому при любой неудаче голос возвращается к Piper,
//! а не пропадает (см. `voice::speak`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub const VOICES: &[(&str, &str)] = &[
    ("ru-RU-SvetlanaNeural", "Светлана — женский"),
    ("ru-RU-DmitryNeural", "Дмитрий — мужской"),
    ("ru-RU-DariyaNeural", "Дарья — женский"),
];

pub const DEFAULT_VOICE: &str = "ru-RU-SvetlanaNeural";
pub const DEFAULT_REGION: &str = "westeurope";

/// Номер последней фразы. Ответ Azure, пришедший после того, как речь
/// остановили или начали новую, не звучит.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Говорит текст голосом Azure. Возвращается, когда звук поставлен в очередь.
pub async fn speak(key: &str, region: &str, voice: &str, rate: f32, text: &str) -> Result<(), String> {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let wav = synthesize(key, region, voice, rate, text).await?;
    if GENERATION.load(Ordering::SeqCst) != generation {
        return Ok(());
    }
    let (samples, hz) = decode_wav(&wav)?;
    super::audio::play(samples, hz);
    Ok(())
}

/// Отменяет фразу, ответ на которую ещё в пути.
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Синтезирует текст в WAV (24 кГц, 16 бит, моно).
pub async fn synthesize(
    key: &str,
    region: &str,
    voice: &str,
    rate: f32,
    text: &str,
) -> Result<Vec<u8>, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("нет ключа Azure Speech".into());
    }
    let region = match region.trim() {
        "" => DEFAULT_REGION,
        region => region,
    };
    let voice = match voice.trim() {
        "" => DEFAULT_VOICE,
        voice => voice,
    };

    let client = crate::net::client_builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|err| format!("HTTP-клиент не собрался: {err}"))?;
    let response = client
        .post(format!("https://{region}.tts.speech.microsoft.com/cognitiveservices/v1"))
        .header("Ocp-Apim-Subscription-Key", key)
        .header("Content-Type", "application/ssml+xml")
        .header("X-Microsoft-OutputFormat", "riff-24khz-16bit-mono-pcm")
        .header("User-Agent", "Sufler")
        .body(ssml(voice, rate, text))
        .send()
        .await
        .map_err(|err| format!("Azure не ответил — проверьте регион «{region}»: {err}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "Azure не принял ключ — проверьте ключ и регион".into(),
            429 => "Azure: исчерпан лимит бесплатного тарифа".into(),
            code => format!("Azure ответил ошибкой {code}"),
        });
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("звук от Azure не дошёл: {err}"))?;
    Ok(bytes.to_vec())
}

/// Разметка запроса: голос и скорость. Скорость — в процентах от обычной.
fn ssml(voice: &str, rate: f32, text: &str) -> String {
    let percent = ((rate.clamp(0.5, 2.0) - 1.0) * 100.0).round() as i32;
    format!(
        "<speak version='1.0' xml:lang='ru-RU'><voice name='{voice}'>\
         <prosody rate='{percent:+}%'>{}</prosody></voice></speak>",
        escape(text)
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Разбирает WAV с 16-битным PCM в отсчёты от -1 до 1 и частоту. Каналы
/// сводятся к первому: речь моно.
fn decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Azure прислал не WAV".into());
    }
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);

    let (mut hz, mut channels, mut bits) = (24_000, 1usize, 16);
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(pos + 4) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = usize::from(u16_at(body + 2)).max(1);
            hz = u32_at(body + 4);
            bits = u16_at(body + 14);
        }
        if id == b"data" {
            if bits != 16 {
                return Err(format!("Azure прислал {bits}-битный звук вместо 16-битного"));
            }
            let end = body.saturating_add(size).min(bytes.len());
            let samples = bytes[body..end]
                .chunks_exact(2 * channels)
                .map(|frame| f32::from(i16::from_le_bytes([frame[0], frame[1]])) / 32768.0)
                .collect();
            return Ok((samples, hz));
        }
        pos = body.saturating_add(size).saturating_add(size & 1);
    }
    Err("в ответе Azure нет звука".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(samples: &[i16], hz: u32) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // моно
        out.extend_from_slice(&hz.to_le_bytes());
        out.extend_from_slice(&(hz * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn wav_is_decoded_to_samples() {
        let (samples, hz) = decode_wav(&wav(&[0, 16384, -32768], 24_000)).unwrap();
        assert_eq!(hz, 24_000);
        assert_eq!(samples, vec![0.0, 0.5, -1.0]);
    }

    #[test]
    fn not_a_wav_is_an_error() {
        assert!(decode_wav(b"<html>error</html>").is_err());
    }

    #[test]
    fn text_is_escaped_and_rate_becomes_percent() {
        let ssml = ssml("ru-RU-DmitryNeural", 1.4, "a < b & «c»");
        assert!(ssml.contains("a &lt; b &amp; «c»"));
        assert!(ssml.contains("rate='+40%'"));
        assert!(ssml.contains("name='ru-RU-DmitryNeural'"));
    }
}
