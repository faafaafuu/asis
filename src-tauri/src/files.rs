//! Поиск файлов на компьютере: «найди фото паспорта», «где мой договор».
//!
//! Ищется по имени файла и по его типу, а не по содержимому картинок: фото с
//! телефона называются по дате — «IMG_20240312_101512.jpg», — и узнать в нём
//! паспорт можно, только посмотрев на снимок. Об этом Ноа говорит прямо,
//! когда по имени ничего не нашлось, и открывает папку, где такие фото лежат.
//!
//! Сначала — индекс поиска Windows: он отвечает мгновенно и знает всё, что
//! система проиндексировала. Потом — обход папок человека: индекс бывает
//! выключен или не видит папку, а искать там, где лежат документы и фото,
//! надо в любом случае. Найденное открывается в проводнике: один файл —
//! выделенным в его папке, несколько — окном поиска с тем же запросом.

use std::path::{Path, PathBuf};

/// Что за файлы ищем.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Any,
    Image,
    Document,
    Video,
    Audio,
}

impl Kind {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "image" | "photo" | "picture" => Self::Image,
            "document" | "doc" => Self::Document,
            "video" => Self::Video,
            "audio" | "music" => Self::Audio,
            _ => Self::Any,
        }
    }

    fn extensions(self) -> &'static [&'static str] {
        match self {
            Kind::Any => &[],
            Kind::Image => &["jpg", "jpeg", "png", "heic", "webp", "bmp", "gif", "tif", "tiff"],
            Kind::Document => &["pdf", "doc", "docx", "odt", "rtf", "txt", "xls", "xlsx", "ppt", "pptx"],
            Kind::Video => &["mp4", "mov", "mkv", "avi", "webm"],
            Kind::Audio => &["mp3", "m4a", "wav", "flac", "ogg", "opus"],
        }
    }

    fn fits(self, path: &Path) -> bool {
        let list = self.extensions();
        list.is_empty()
            || path
                .extension()
                .map(|ext| list.iter().any(|want| ext.eq_ignore_ascii_case(want)))
                .unwrap_or(false)
    }
}

/// Как ещё может называться то, что просят: файлы часто названы по-английски
/// или транслитом, а просят по-русски.
const SYNONYMS: &[(&str, &[&str])] = &[
    ("паспорт", &["passport", "pasport"]),
    ("скрин", &["screenshot", "снимок экрана"]),
    ("резюме", &["resume", "cv", "rezume"]),
    ("договор", &["contract", "dogovor"]),
    ("квитанц", &["receipt", "kvitanc"]),
    ("чек", &["receipt", "check"]),
    ("билет", &["ticket", "bilet"]),
    ("права", &["license", "prava"]),
    ("снилс", &["snils"]),
    ("полис", &["polis", "insurance"]),
    ("фото", &["photo", "img"]),
];

/// Слова запроса, по которым ищем имя файла.
///
/// Короткие слова и сами слова о типе файла («фото», «документ») в имя не
/// идут: «фото паспорта» — это файл-картинка со словом «паспорт» в имени, а
/// не файл, в имени которого есть «фото».
fn terms_of(query: &str) -> Vec<String> {
    const ABOUT_KIND: &[&str] = &[
        "фото", "фотк", "фотограф", "картин", "изображ", "снимок", "документ", "файл", "видео",
        "запис", "музык", "песн", "мой", "моя", "моё", "мои", "моего", "моей",
    ];
    let mut terms: Vec<String> = Vec::new();
    for word in query
        .to_lowercase()
        .replace('ё', "е")
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3)
    {
        if ABOUT_KIND.iter().any(|skip| word.starts_with(skip)) {
            continue;
        }
        // Основа слова: «паспорта» — «паспорт», «договоры» — «договор».
        // Отрезается только гласная окончания: «договор» остаётся целым.
        let mut stem = word.to_string();
        if stem.chars().count() >= 5 && stem.ends_with(|ch: char| "аяыиуюеоьй".contains(ch)) {
            stem.pop();
        }
        for (root, others) in SYNONYMS {
            let short_of_root = stem.chars().count() >= 4 && root.starts_with(stem.as_str());
            if stem.starts_with(root) || short_of_root {
                // И сам корень: «паспортов» в имени «паспорт.jpg» не найдётся.
                terms.push((*root).to_string());
                terms.extend(others.iter().map(|other| (*other).to_string()));
            }
        }
        terms.push(stem);
    }
    terms.sort();
    terms.dedup();
    terms
}

/// Ищет и открывает найденное; отдаёт ответ вслух.
pub fn find(query: &str, kind: Kind) -> String {
    let terms = terms_of(query);
    if terms.is_empty() && kind == Kind::Any {
        return "Не понял, что искать.".into();
    }

    let mut found = indexed(&terms, kind);
    if found.is_empty() {
        found = walked(&terms, kind);
    }
    log::info!("поиск файлов по «{query}»: {:?} → {}", terms, found.len());

    let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    match found.as_slice() {
        [] => {
            // По имени ничего — для фото это обычное дело: у снимков с
            // телефона в имени дата. Открываем, где такие фото лежат.
            if kind == Kind::Image {
                let _ = crate::pc::open(&home.join("Pictures").to_string_lossy());
                return format!(
                    "Файлов с «{}» в названии не нашёл. Фото с телефона обычно названы по \
                     дате — открыл «Изображения», посмотрите там.",
                    query.trim()
                );
            }
            format!("Не нашёл файлов по «{}».", query.trim())
        }
        [single] => {
            let _ = select_in_explorer(single);
            crate::planner::offer_file(single.clone());
            format!(
                "Нашёл: {} — открыл папку с ним.",
                single.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default()
            )
        }
        many => {
            let term = terms.first().cloned().unwrap_or_default();
            let _ = crate::pc::open(&search_window(&term, &home));
            let names: Vec<String> = many
                .iter()
                .take(3)
                .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
                .collect();
            format!(
                "Нашёл {} — например: {}. Открыл их списком.",
                count_files(many.len()),
                names.join(", ")
            )
        }
    }
}

fn count_files(count: usize) -> String {
    let word = match (count % 100, count % 10) {
        (11..=14, _) => "файлов",
        (_, 1) => "файл",
        (_, 2..=4) => "файла",
        _ => "файлов",
    };
    format!("{count} {word}")
}

/// Окно поиска проводника с тем же запросом — по индексу Windows.
fn search_window(term: &str, home: &Path) -> String {
    let encode = |text: &str| {
        text.bytes()
            .map(|byte| match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (byte as char).to_string(),
                other => format!("%{other:02X}"),
            })
            .collect::<String>()
    };
    format!(
        "search-ms:query={}&crumb=location:{}",
        encode(term),
        encode(&home.to_string_lossy())
    )
}

fn select_in_explorer(path: &Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

/// Индекс поиска Windows. Пусто — индекс выключен, не видит папку или не нашёл.
///
/// Слова запроса попадают в текст запроса к индексу, поэтому из них остаются
/// только буквы и цифры: кавычка превратила бы слово в команду.
fn indexed(terms: &[String], kind: Kind) -> Vec<PathBuf> {
    let clean: Vec<String> = terms
        .iter()
        .map(|term| term.chars().filter(|ch| ch.is_alphanumeric()).collect::<String>())
        .filter(|term| !term.is_empty())
        .collect();
    let mut conditions: Vec<String> = Vec::new();
    if !clean.is_empty() {
        let names: Vec<String> = clean
            .iter()
            .map(|term| format!("System.FileName LIKE '%{term}%'"))
            .collect();
        conditions.push(format!("({})", names.join(" OR ")));
    }
    let extensions = kind.extensions();
    if !extensions.is_empty() {
        let list: Vec<String> = extensions.iter().map(|ext| format!("'.{ext}'")).collect();
        conditions.push(format!("System.FileExtension IN ({})", list.join(",")));
    }
    if conditions.is_empty() {
        return Vec::new();
    }

    // Путь берётся из адреса файла, а не из «пути для показа»: тот написан
    // именами папок, как их показывает проводник, — «Документы», «Загрузки», —
    // а на диске папки называются Documents и Downloads, и такого пути нет.
    let sql = format!(
        "SELECT TOP 30 System.ItemUrl FROM SystemIndex WHERE {} ORDER BY System.DateModified DESC",
        conditions.join(" AND ")
    );
    let script = format!(
        "$ErrorActionPreference = 'Stop'\n\
         [Console]::OutputEncoding = [Text.Encoding]::UTF8\n\
         try {{\n\
           $c = New-Object -ComObject ADODB.Connection\n\
           $c.Open(\"Provider=Search.CollatorDSO;Extended Properties='Application=Windows';\")\n\
           $rs = $c.Execute(\"{sql}\")\n\
           while (-not $rs.EOF) {{ $rs.Fields.Item('System.ItemUrl').Value; $rs.MoveNext() }}\n\
         }} catch {{ }}"
    );
    crate::sysinfo::powershell(&script)
        .unwrap_or_default()
        .lines()
        .filter_map(path_of_url)
        .filter(|path| path.exists())
        .collect()
}

/// Путь файла из его адреса в индексе: «file:C:/Users/…» — «C:\Users\…».
///
/// Адреса не файлов — письма, записи календаря — путями не бывают и
/// отбрасываются. Метка порядка байтов в начале вывода — от кодировки консоли.
fn path_of_url(url: &str) -> Option<PathBuf> {
    let url = url.trim().trim_start_matches('\u{feff}');
    let rest = url.strip_prefix("file:").or_else(|| url.strip_prefix("FILE:"))?;
    Some(PathBuf::from(rest.replace('/', "\\")))
}

/// Обход папок человека: рабочий стол, документы, загрузки, изображения,
/// OneDrive. Глубина и число просмотренных файлов ограничены, чтобы поиск
/// заканчивался за секунды даже на заваленном диске.
fn walked(terms: &[String], kind: Kind) -> Vec<PathBuf> {
    let Ok(home) = std::env::var("USERPROFILE").map(PathBuf::from) else {
        return Vec::new();
    };
    let roots = [
        home.join("Desktop"),
        home.join("Documents"),
        home.join("Downloads"),
        home.join("Pictures"),
        home.join("OneDrive"),
    ];
    let mut found = Vec::new();
    let mut budget = 60_000usize;
    for root in roots {
        walk(&root, 0, terms, kind, &mut found, &mut budget);
    }
    found.truncate(30);
    found
}

fn walk(dir: &Path, depth: usize, terms: &[String], kind: Kind, found: &mut Vec<PathBuf>, budget: &mut usize) {
    if depth > 6 || found.len() >= 30 {
        return;
    }
    let Ok(children) = std::fs::read_dir(dir) else { return };
    for child in children.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = child.path();
        if path.is_dir() {
            let name = child.file_name().to_string_lossy().to_lowercase();
            // Служебные папки программ файлов человека не содержат.
            if name.starts_with('.') || name == "node_modules" || name == "appdata" {
                continue;
            }
            walk(&path, depth + 1, terms, kind, found, budget);
            continue;
        }
        let name = child.file_name().to_string_lossy().to_lowercase().replace('ё', "е");
        let named = terms.is_empty() || terms.iter().any(|term| name.contains(term.as_str()));
        if named && kind.fits(&path) {
            found.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_photo_of_a_passport_is_a_passport_picture() {
        let terms = terms_of("фото паспорта");
        assert!(terms.contains(&"паспорт".to_string()), "{terms:?}");
        assert!(terms.contains(&"passport".to_string()), "{terms:?}");
        assert!(!terms.iter().any(|term| term.starts_with("фото")), "{terms:?}");
        assert_eq!(Kind::parse("image"), Kind::Image);
    }

    #[test]
    fn words_about_ownership_are_not_searched() {
        assert_eq!(terms_of("мой договор"), vec!["contract", "dogovor", "договор"]);
    }

    #[test]
    fn kinds_filter_by_extension() {
        assert!(Kind::Image.fits(Path::new("C:/x/IMG_1.JPG")));
        assert!(!Kind::Image.fits(Path::new("C:/x/passport.pdf")));
        assert!(Kind::Any.fits(Path::new("C:/x/anything.bin")));
    }

    #[test]
    fn paths_come_from_file_addresses() {
        assert_eq!(
            path_of_url("file:C:/Users/me/Documents/паспорт.jpg"),
            Some(PathBuf::from("C:\\Users\\me\\Documents\\паспорт.jpg"))
        );
        assert_eq!(
            path_of_url("\u{feff}file:D:/фото/скан.pdf"),
            Some(PathBuf::from("D:\\фото\\скан.pdf"))
        );
        // Письмо из почты — не файл.
        assert_eq!(path_of_url("mapi://{S-1-5-21}/Входящие/письмо"), None);
    }

    #[test]
    fn files_are_counted_in_russian() {
        assert_eq!(count_files(1), "1 файл");
        assert_eq!(count_files(3), "3 файла");
        assert_eq!(count_files(12), "12 файлов");
        assert_eq!(count_files(21), "21 файл");
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// `cargo test --lib files::live -- --ignored --nocapture`
    #[test]
    #[ignore = "ищет на настоящем диске"]
    fn the_index_answers() {
        let terms = terms_of("readme");
        let started = std::time::Instant::now();
        let found = indexed(&terms, Kind::Any);
        println!("индекс: {} за {} мс", found.len(), started.elapsed().as_millis());
        let started = std::time::Instant::now();
        let found = walked(&terms, Kind::Any);
        println!("обход: {} за {} мс", found.len(), started.elapsed().as_millis());
    }
}
