//! Состояние приложения. Живёт в managed state Tauri и доступно всем командам.

use std::sync::{Arc, Mutex, RwLock};

use crate::ai_client::{build_provider, AiProvider};
use crate::config::Config;
use crate::selection::Selection;

pub struct AppState {
    pub config: RwLock<Config>,
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

    /// Пересобирает провайдера после смены настроек — чтобы не требовать перезапуска.
    pub fn rebuild_provider(&self, config: &crate::config::AiConfig) {
        let provider: Arc<dyn AiProvider> = Arc::from(build_provider(config));
        *self.provider.write().expect("provider poisoned") = provider;
    }

    pub fn provider(&self) -> Arc<dyn AiProvider> {
        self.provider.read().expect("provider poisoned").clone()
    }

    pub fn set_selection(&self, selection: Selection) {
        *self.selection.lock().expect("selection poisoned") = Some(selection);
    }

    pub fn selection(&self) -> Option<Selection> {
        self.selection.lock().expect("selection poisoned").clone()
    }

    pub fn clear_selection(&self) {
        *self.selection.lock().expect("selection poisoned") = None;
    }

    pub fn error_text(&self) -> String {
        self.config
            .read()
            .expect("config poisoned")
            .ui
            .error_text
            .clone()
    }
}
