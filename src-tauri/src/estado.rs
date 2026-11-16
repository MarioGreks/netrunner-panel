//! Estado compartido de la aplicación.

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use crate::config::Config;

pub struct Estado {
    pub config: Mutex<Config>,
    /// El panel está mostrado (la ventana visible).
    pub panel_visible: AtomicBool,
}

impl Estado {
    pub fn nuevo(config: Config) -> Self {
        Estado {
            config: Mutex::new(config),
            panel_visible: AtomicBool::new(false),
        }
    }
}
