//! Заглушка для платформ вне списка поддерживаемых (в том числе мобильных сборок,
//! где выделение приходит из нативного плагина, а не из системного хука).

use super::{Capability, PlatformIntegration, Selection};
use crate::config::TriggerConfig;

pub struct Platform;

impl PlatformIntegration for Platform {
    fn capability(&self) -> Capability {
        Capability::Unavailable {
            title: "Системное выделение недоступно".into(),
            hint: "На этой платформе попап открывается не глобальным хуком, а пунктом \
                   «Объяснить» в меню выделения текста."
                .into(),
        }
    }

    fn poll_trigger(&self, _config: &TriggerConfig) -> Option<Selection> {
        None
    }
}

pub fn create() -> Box<dyn PlatformIntegration> {
    Box::new(Platform)
}
