//! Снимки экрана: весь экран или окно одной программы.
//!
//! Снимается средствами Windows (GDI), в PNG кодирует встроенный кодировщик
//! изображений Windows — ни библиотек, ни программ со стороны. Снимок
//! ложится в «Изображения\Суфлёр»; попросили из Telegram — уходит туда же.

use std::ffi::c_void;
use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, IsIconic, SetForegroundWindow, ShowWindow, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_RESTORE,
};

/// Весь экран — все мониторы разом.
pub fn screen() -> Result<Vec<u8>, String> {
    // SAFETY: только читает размеры рабочего стола.
    let (x, y, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    png(&grab(x, y, width, height)?, width as u32, height as u32)
}

/// Окно программы. Свёрнутое сперва разворачивается и выходит вперёд: снять
/// можно только то, что на экране.
pub fn window(handle: isize) -> Result<Vec<u8>, String> {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};

    let hwnd = HWND(handle as *mut c_void);
    // SAFETY: обычные просьбы к окну — развернуться и выйти вперёд.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    // Окну нужно мгновение, чтобы дорисоваться поверх остальных.
    std::thread::sleep(Duration::from_millis(400));

    let mut rect = RECT::default();
    // Видимые границы — без невидимой полосы для растягивания, которую
    // `GetWindowRect` включает в окно.
    // SAFETY: пишет в свою же переменную ровно её размер.
    let bounds = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    };
    if bounds.is_err() {
        // SAFETY: то же, запасной путь.
        unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|err| format!("окно не нашлось: {err}"))?;
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 {
        return Err("окно не видно на экране".into());
    }
    png(&grab(rect.left, rect.top, width, height)?, width as u32, height as u32)
}

/// Кладёт снимок в «Изображения\Суфлёр» и отдаёт путь и то, как назвать
/// папку вслух.
///
/// «Изображения» — папка, которую Windows может охранять от незнакомых
/// программ («Контролируемый доступ к папкам» в Защитнике): запись туда
/// тогда отклоняется. Снимок в этом случае ложится в папку самой программы —
/// сделанный снимок не должен пропадать из-за того, куда его не пустили.
pub fn save(app: &AppHandle, png: &[u8]) -> Result<(PathBuf, &'static str), String> {
    let name = format!(
        "снимок {}.png",
        chrono::Local::now().format("%Y-%m-%d %H-%M-%S")
    );
    let write = |dir: PathBuf| -> Result<PathBuf, String> {
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let path = dir.join(&name);
        std::fs::write(&path, png).map_err(|err| err.to_string())?;
        Ok(path)
    };

    let pictures = app
        .path()
        .picture_dir()
        .map_err(|err| err.to_string())
        .and_then(|dir| write(dir.join("Суфлёр")));
    match pictures {
        Ok(path) => Ok((path, "«Изображения\\Суфлёр»")),
        Err(err) => {
            log::warn!("в «Изображения» снимок не записался ({err}) — кладу в папку программы");
            let own = app
                .path()
                .app_local_data_dir()
                .map_err(|err| err.to_string())?
                .join("снимки");
            write(own).map(|path| (path, "папку программы"))
        }
    }
}

/// Пиксели прямоугольника экрана: BGRA, сверху вниз.
fn grab(x: i32, y: i32, width: i32, height: i32) -> Result<Vec<u8>, String> {
    if width <= 0 || height <= 0 {
        return Err("экран нулевого размера".into());
    }
    // SAFETY: всё созданное здесь здесь же и освобождается; буфер пикселей
    // ровно того размера, который описан в заголовке.
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(memory, bitmap.into());
        let copied = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            Some(screen),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        );

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Отрицательная высота — строки сверху вниз, как в PNG.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let lines = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut c_void),
            &mut info,
            DIB_RGB_COLORS,
        );

        SelectObject(memory, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);

        copied.map_err(|err| format!("экран не снялся: {err}"))?;
        if lines == 0 {
            return Err("экран не снялся".into());
        }
        Ok(pixels)
    }
}

/// BGRA в PNG — встроенным кодировщиком Windows.
pub fn png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapEncoder, BitmapPixelFormat};
    use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    // Кодировщик — объект Windows Runtime, ему нужен COM в этом потоке.
    // SAFETY: инициализация COM в текущем потоке; повторная безвредна.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let encode = || -> windows::core::Result<Vec<u8>> {
        let stream = InMemoryRandomAccessStream::new()?;
        let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId()?, &stream)?.join()?;
        encoder.SetPixelData(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Ignore,
            width,
            height,
            96.0,
            96.0,
            pixels,
        )?;
        encoder.FlushAsync()?.join()?;
        let size = stream.Size()? as u32;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        reader.LoadAsync(size)?.join()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes)?;
        Ok(bytes)
    };
    encode().map_err(|err| format!("снимок не закодировался: {err}"))
}

#[cfg(test)]
mod live {
    /// `cargo test --lib shots::live -- --ignored --nocapture`
    #[test]
    #[ignore = "снимает настоящий экран"]
    fn the_screen_is_captured() {
        let png = super::screen().expect("снимок");
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        println!("снимок: {} КБ", png.len() / 1024);
    }
}
