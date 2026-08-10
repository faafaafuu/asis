//! Состояние приложения. Живёт в managed state Tauri и доступно всем командам.

use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::ai_client::{build_provider, AiProvider};
use crate::config::Config;
use crate::selection::Selection;

/// Доступ к общим данным, переживающий чужую панику.
///
/// Блокировки в стандартной библиотеке после паники держателя объявляются
/// «отравленными», и каждое следующее обращение к ним паникует снова. Для этого
/// приложения такое поведение разрушительно: команды выполняются в отдельных задачах,
/// где паника никого не роняет и нигде не печатается, — она просто не отвечает окну.
/// Одна случайная ошибка в одном месте молча и навсегда обрывала бы всё, что читает
/// настройки: и объяснения, и проверку ключа, и сохранение. Именно так и выглядит
/// «после сохранения всё встало колом».
///
/// Сами данные при отравлении целы — блокировка лишь помечена. Поэтому берём их
/// и работаем дальше, а не устраиваем вторую панику поверх первой.
fn unpoison<T>(result: Result<T, std::sync::PoisonError<T>>) -> T {
    result.unwrap_or_else(|err| err.into_inner())
}

pub struct AppState {
    config: RwLock<Config>,
    /// Провайдер под `Arc`, чтобы команда могла взять его клон и ждать сеть,
    /// не удерживая блокировку: иначе один медленный запрос подвесил бы остальные.
    provider: RwLock<Arc<dyn AiProvider>>,
    /// Выделение, для которого сейчас открыт попап: якорь для позиционирования и
    /// контекст для follow-up-вопросов.
    selection: Mutex<Option<Selection>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let provider: Arc<dyn AiProvider> = Arc::from(build_provider(&config.ai));
        Self {
            config: RwLock::new(config),
            provider: RwLock::new(provider),
            selection: Mutex::new(None),
        }
    }

    /// Конфигурация на чтение. Держите гварду как можно короче: пока она жива,
    /// сохранение настроек ждёт.
    pub fn config(&self) -> RwLockReadGuard<'_, Config> {
        unpoison(self.config.read())
    }

    pub fn config_mut(&self) -> RwLockWriteGuard<'_, Config> {
        unpoison(self.config.write())
    }

    /// Пересобирает провайдера после смены настроек — чтобы не требовать перезапуска.
    pub fn rebuild_provider(&self, config: &crate::config::AiConfig) {
        let provider: Arc<dyn AiProvider> = Arc::from(build_provider(config));
        *unpoison(self.provider.write()) = provider;
    }

    pub fn provider(&self) -> Arc<dyn AiProvider> {
        unpoison(self.provider.read()).clone()
    }

    fn selection_lock(&self) -> MutexGuard<'_, Option<Selection>> {
        unpoison(self.selection.lock())
    }

    pub fn set_selection(&self, selection: Selection) {
        *self.selection_lock() = Some(selection);
    }

    pub fn selection(&self) -> Option<Selection> {
        self.selection_lock().clone()
    }

    pub fn clear_selection(&self) {
        *self.selection_lock() = None;
    }

    pub fn error_text(&self) -> String {
        self.config().ui.error_text.clone()
    }
}
