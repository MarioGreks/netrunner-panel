//! Ventana HUD de aviso del 80 % / 100 % (`aviso-consumo.html`). No existe hasta que hace falta y
//! se destruye al cerrarla; si ya está abierta, se sustituye el texto. Misma configuración
//! visual que la ventana «aviso» del actualizador.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

const ETIQUETA: &str = "aviso-consumo";

/// Lo que enseña la ventana.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ContenidoAviso {
    pub titulo: String,
    pub texto: String,
    /// Límite alcanzado (100 %): se pinta en rojo.
    pub alerta: bool,
}

#[derive(Default)]
pub struct AvisoConsumo {
    contenido: Mutex<Option<ContenidoAviso>>,
}

/// Abre la ventana con el aviso o, si ya está abierta, cambia el texto y la trae al frente.
pub fn mostrar(app: &AppHandle, limite: bool, texto: &str) {
    let estado = app.state::<AvisoConsumo>();
    let titulo = if limite { "LÍMITE ALCANZADO" } else { "CONSUMO // 80 %" };
    *estado.contenido.lock().unwrap_or_else(|e| e.into_inner()) = Some(ContenidoAviso {
        titulo: titulo.into(),
        texto: texto.into(),
        alerta: limite,
    });
    if let Some(v) = app.get_webview_window(ETIQUETA) {
        let _ = app.emit_to(ETIQUETA, "consumo://aviso", ());
        let _ = v.show();
        let _ = v.set_focus();
        return;
    }
    let res = WebviewWindowBuilder::new(app, ETIQUETA, WebviewUrl::App("aviso-consumo.html".into()))
        .title("Netrunner Panel · consumo")
        .inner_size(420.0, 220.0)
        .center()
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(false)
        .build();
    match res {
        Ok(v) => {
            let _ = v.set_focus();
        }
        Err(e) => eprintln!("consumo: no se pudo abrir la ventana de aviso: {e}"),
    }
}

/// La ventana pide aquí lo que tiene que enseñar (al cargar y cuando llega `consumo://aviso`).
#[tauri::command]
pub fn aviso_consumo_actual(estado: State<'_, AvisoConsumo>) -> Option<ContenidoAviso> {
    estado.contenido.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// ENTENDIDO: destruye la ventana (no se oculta).
#[tauri::command]
pub fn cerrar_aviso_consumo(app: AppHandle, estado: State<'_, AvisoConsumo>) {
    *estado.contenido.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if let Some(v) = app.get_webview_window(ETIQUETA) {
        let _ = v.destroy();
    }
}
