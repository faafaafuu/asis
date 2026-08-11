//! Защита ключа AI-сервиса в файле настроек.
//!
//! Ключ лежал в `config.json` открытым текстом. Файл — обычный, с обычными
//! правами: его читает всё, что запущено от имени пользователя, а на общей
//! машине ещё и другие учётные записи, если каталог профиля им доступен (так
//! бывает чаще, чем кажется, — достаточно один раз выдать доступ «для обмена
//! файлами»). Ключ платного сервиса, утёкший таким образом, оплачивает не тот,
//! кто им пользуется.
//!
//! На Windows есть штатный способ это закрыть: DPAPI шифрует данные так, что
//! расшифровать их может только та же учётная запись на той же машине. Ни
//! пароля, ни хранилища ключей заводить не нужно — система делает это сама.
//!
//! Что здесь НЕ решается, и это честно: программа, запущенная от имени того же
//! человека, расшифрует ключ так же легко, как это делаем мы. DPAPI защищает от
//! чтения файла посторонним — от соседней учётной записи, из резервной копии,
//! с вынутого диска, — но не от вредоноса, уже работающего под тем же
//! пользователем. От такого не спасает ни одно хранилище на этом уровне.

/// Метка зашифрованного значения. По ней отличаем шифр от старого открытого
/// ключа: у людей уже есть настройки, и они обязаны продолжать работать.
const PREFIX: &str = "dpapi:";

/// Зашифровано ли уже это значение.
pub fn is_protected(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// Готовит ключ к записи на диск.
///
/// Не смогли зашифровать — возвращаем как есть. Отказ сохранить настройки был
/// бы хуже: человек остался бы без работающей программы из-за того, что мы не
/// сумели применить дополнительную защиту.
pub fn protect(value: &str) -> String {
    if value.is_empty() || value.starts_with(PREFIX) {
        return value.to_string();
    }

    #[cfg(target_os = "windows")]
    match windows_impl::encrypt(value.as_bytes()) {
        Ok(bytes) => return format!("{PREFIX}{}", base64(&bytes)),
        Err(err) => log::warn!("ключ не удалось зашифровать ({err}) — пишем как есть"),
    }

    value.to_string()
}

/// Восстанавливает ключ, прочитанный с диска.
///
/// Значение без метки — это ключ, записанный старой версией программы либо
/// вписанный человеком руками в файл. Так и отдаём: не наше дело отказывать
/// в работе из-за формата хранения.
pub fn reveal(value: &str) -> String {
    let Some(encoded) = value.strip_prefix(PREFIX) else {
        return value.to_string();
    };

    #[cfg(target_os = "windows")]
    {
        match unbase64(encoded).and_then(|bytes| windows_impl::decrypt(&bytes).ok()) {
            Some(plain) => return String::from_utf8_lossy(&plain).into_owned(),
            None => {
                // Расшифровать не вышло: файл принесли с другой машины или из
                // другой учётной записи — там ключ не восстановить в принципе.
                log::warn!("сохранённый ключ не расшифровывается — впишите его заново");
                return String::new();
            }
        }
    }

    // На остальных системах метки взяться неоткуда, но если файл принесли
    // с Windows — вернуть шифр как ключ было бы хуже, чем ничего.
    #[cfg(not(target_os = "windows"))]
    {
        let _ = encoded;
        log::warn!("ключ зашифрован средствами Windows и здесь не читается");
        String::new()
    }
}

/* ── base64 без зависимостей ─────────────────────────────────────────────── */

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn unbase64(text: &str) -> Option<Vec<u8>> {
    let mut bits = Vec::with_capacity(text.len());
    for ch in text.bytes() {
        if ch == b'=' {
            break;
        }
        let value = ALPHABET.iter().position(|c| *c == ch)?;
        bits.push(value as u8);
    }

    let mut out = Vec::with_capacity(bits.len() * 3 / 4);
    for group in bits.chunks(4) {
        let mut n = 0u32;
        for (i, value) in group.iter().enumerate() {
            n |= u32::from(*value) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if group.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if group.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/* ── DPAPI ───────────────────────────────────────────────────────────────── */

#[cfg(target_os = "windows")]
mod windows_impl {
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };

    /// Описание для системного окна запроса — мы его не показываем, но Windows
    /// хранит строку рядом с данными, и в диспетчере учётных данных видно, чьё это.
    fn describe() -> windows::core::HSTRING {
        windows::core::HSTRING::from("Суфлёр: ключ AI-сервиса")
    }

    pub fn encrypt(data: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let mut input = CRYPT_INTEGER_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB::default();

            CryptProtectData(
                &mut input,
                windows::core::PCWSTR(describe().as_ptr()),
                None,
                None,
                None,
                0,
                &mut output,
            )
            .map_err(|err| err.to_string())?;

            Ok(take(&mut output))
        }
    }

    pub fn decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let mut input = CRYPT_INTEGER_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB::default();

            CryptUnprotectData(&mut input, None, None, None, None, 0, &mut output)
                .map_err(|err| err.to_string())?;

            Ok(take(&mut output))
        }
    }

    /// Забирает данные из блоба и освобождает выданную системой память.
    /// Без LocalFree каждое сохранение настроек оставляло бы за собой утечку.
    unsafe fn take(blob: &mut CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let slice = std::slice::from_raw_parts(blob.pbData, blob.cbData as usize);
        let owned = slice.to_vec();
        let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            blob.pbData as *mut core::ffi::c_void,
        )));
        owned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_survives_round_trip() {
        for sample in ["", "a", "ab", "abc", "abcd", "ключ-с-кириллицей"] {
            let encoded = base64(sample.as_bytes());
            let decoded = unbase64(&encoded).expect("разбирается");
            assert_eq!(decoded, sample.as_bytes(), "образец «{sample}»");
        }
    }

    #[test]
    fn empty_and_already_protected_stay_untouched() {
        assert_eq!(protect(""), "");
        // Двойное шифрование ломало бы ключ при каждом сохранении настроек.
        assert_eq!(protect("dpapi:abc"), "dpapi:abc");
    }

    #[test]
    fn protection_is_recognised() {
        assert!(is_protected("dpapi:abc"));
        // Настоящий ключ Groq начинается с gsk_ и меткой быть не может.
        assert!(!is_protected("gsk_test_key"));
        assert!(!is_protected(""));
    }

    #[test]
    fn plain_key_is_returned_as_is() {
        // Настройки, записанные прежними версиями, обязаны продолжать работать.
        assert_eq!(reveal("gsk_test_key"), "gsk_test_key");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn protected_key_comes_back() {
        let secret = "gsk_очень_секретный_ключ";
        let stored = protect(secret);
        assert!(stored.starts_with(PREFIX), "ключ должен быть помечен: {stored}");
        assert!(!stored.contains(secret), "открытый ключ не должен остаться в строке");
        assert_eq!(reveal(&stored), secret);
    }
}
