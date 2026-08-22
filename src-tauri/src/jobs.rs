//! Дочерние процессы, которые обязаны уйти вместе с программой.
//!
//! Сервер распознавания держит в видеопамяти полтора гигабайта. Обычного
//! `kill()` при выходе мало: программу могут снять из диспетчера задач, и тогда
//! наш код не выполнится вовсе, а сервер останется работать. Несколько таких
//! осиротевших серверов заполняют видеопамять целиком, и машина начинает
//! тормозить вся — вплоть до того, что виноватым выглядит что угодно, кроме
//! них.
//!
//! Windows умеет решать это сама: если процесс приписан к «заданию» с флагом
//! «убить при закрытии задания», система снимет его, как только закроется
//! последняя ссылка на задание. Ссылка закрывается вместе с нашим процессом —
//! любым способом, включая снятие из диспетчера.

#[cfg(target_os = "windows")]
pub fn adopt(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::AssignProcessToJobObject;

    let Some(job) = job() else { return };

    // SAFETY: дескриптор задания создан нами и жив всё время работы программы,
    // дескриптор процесса принадлежит переданному `Child` и жив, пока жив он.
    let assigned = unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) };
    if let Err(err) = assigned {
        log::warn!("не удалось привязать процесс к заданию: {err}");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn adopt(_child: &std::process::Child) {}

#[cfg(target_os = "windows")]
struct Job(windows::Win32::Foundation::HANDLE);

// SAFETY: дескриптор задания — обычное число, которое Windows разрешает
// использовать из любого потока. Ничего, кроме передачи в системные вызовы,
// с ним не делается.
#[cfg(target_os = "windows")]
unsafe impl Send for Job {}
#[cfg(target_os = "windows")]
unsafe impl Sync for Job {}

/// Задание, к которому приписываются дочерние процессы. Создаётся один раз.
#[cfg(target_os = "windows")]
fn job() -> Option<&'static Job> {
    use std::sync::OnceLock;
    use windows::Win32::System::JobObjects::{
        JobObjectExtendedLimitInformation, SetInformationJobObject, CreateJobObjectW,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    static JOB: OnceLock<Option<Job>> = OnceLock::new();

    JOB.get_or_init(|| {
        // SAFETY: создаём безымянное задание с настройками по умолчанию и сразу
        // же выставляем ему единственное ограничение. Оба вызова только
        // работают с только что полученным дескриптором.
        unsafe {
            let handle = match CreateJobObjectW(None, None) {
                Ok(handle) => handle,
                Err(err) => {
                    log::warn!("задание для дочерних процессов не создалось: {err}");
                    return None;
                }
            };

            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let set = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            );
            if let Err(err) = set {
                log::warn!("заданию не выставилось ограничение: {err}");
                return None;
            }

            Some(Job(handle))
        }
    })
    .as_ref()
}
