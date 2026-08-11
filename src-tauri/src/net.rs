//! Общая настройка HTTP-клиента. Существует ради одной особенности Android.
//!
//! На настольных системах TLS системный (schannel, Security.framework, OpenSSL),
//! и здесь делать нечего. На Android системного OpenSSL нет, поэтому там мы
//! собираемся с rustls — а он в reqwest 0.13 всегда проверяет сертификаты через
//! rustls-platform-verifier. Тот на Android умеет работать только после
//! инициализации через JNI, и без неё падает при первом же запросе:
//!
//!     panicked at rustls-platform-verifier/src/android.rs:
//!     Expect rustls-platform-verifier to be initialized
//!
//! Для человека это выглядело как «внутренняя ошибка» на любое действие, где
//! нужна сеть, — от проверки ключа до объяснения слова.
//!
//! Инициализировать проверяльщик можно только добравшись до JNI-окружения, что
//! тянет отдельные зависимости и надежду на то, что окружение вообще доступно
//! в нужный момент. Вместо этого берём доверенные сертификаты прямо из
//! системного хранилища Android — оно лежит обычными файлами и читается всеми —
//! и отдаём их клиенту явно. Тогда reqwest берёт заданный список и к
//! проверяльщику не обращается вовсе.

/// Заготовка клиента с настроенным доверием к сертификатам.
pub fn client_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();

    #[cfg(target_os = "android")]
    {
        let roots = android_roots();
        if roots.is_empty() {
            // Пустой список означал бы «не доверять никому»: лучше оставить
            // клиента как есть и получить понятную ошибку сети, чем молча
            // обрубить все https-запросы.
            log::error!("не удалось прочитать сертификаты Android — https может не работать");
            return builder;
        }
        log::info!("сертификатов из хранилища Android: {}", roots.len());
        return builder.tls_certs_only(roots);
    }

    #[cfg(not(target_os = "android"))]
    builder
}

/// Доверенные сертификаты из системного хранилища Android.
///
/// Каталогов два: начиная с Android 14 хранилище переехало в модуль Conscrypt,
/// но старый путь на многих сборках остаётся. Читаем оба и складываем всё, что
/// разобралось: файлы там лежат по одному сертификату, имя — хеш, а внутри
/// PEM с текстовым описанием после него.
#[cfg(target_os = "android")]
fn android_roots() -> Vec<reqwest::Certificate> {
    const DIRS: [&str; 2] = [
        "/apex/com.android.conscrypt/cacerts",
        "/system/etc/security/cacerts",
    ];

    let mut roots = Vec::new();
    for dir in DIRS {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            // Битый или неожиданный файл — не повод бросать остальные:
            // одного плохого сертификата достаточно, чтобы остаться без сети.
            if let Ok(cert) = reqwest::Certificate::from_pem(&bytes) {
                roots.push(cert);
            }
        }
        if !roots.is_empty() {
            break;
        }
    }
    roots
}
