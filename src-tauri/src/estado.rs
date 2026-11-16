//! Estado compartido de la aplicación.

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use crate::config::Config;

pub struct Estado {
    pub config: Mutex<Config>,
    /// El panel está mostrado (la ventana visible).
    pub panel_visible: AtomicBool,
    /// Alto lógico de la miniventana; lo fija el frontend según su contenido.
    pub alto_mini: Mutex<f64>,
}

impl Estado {
    pub fn nuevo(config: Config) -> Self {
        Estado {
            config: Mutex::new(config),
            panel_visible: AtomicBool::new(false),
            alto_mini: Mutex::new(crate::ventanas::MINI_ALTO_MIN),
        }
    }
}
