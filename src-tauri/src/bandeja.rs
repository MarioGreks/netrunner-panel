//! Icono de la bandeja del sistema: clic izquierdo alterna el panel y el menú
//! (clic derecho) ofrece abrir, ajustes, pausar la apertura por borde y salir.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::config;
use crate::estado::Estado;
use crate::ventanas;

pub fn crear(app: &AppHandle) -> tauri::Result<()> {
    let pausado = app.state::<Estado>().config.lock().unwrap().borde_pausado;

    let abrir = MenuItem::with_id(app, "abrir", "Abrir panel", true, None::<&str>)?;
    let ajustes = MenuItem::with_id(app, "ajustes", "Ajustes", true, None::<&str>)?;
    let pausa = CheckMenuItem::with_id(app, "pausa", "Pausar apertura por borde", true, pausado, None::<&str>)?;
    let salir = MenuItem::with_id(app, "salir", "Salir", true, None::<&str>)?;
    let separador = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&abrir, &ajustes, &pausa, &separador, &salir])?;

    let casilla = pausa.clone();
    let mut constructor = TrayIconBuilder::with_id("principal")
        .tooltip("Netrunner Panel")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, evento| match evento.id().as_ref() {
            "abrir" => ventanas::mostrar_panel(app),
            "ajustes" => {
                ventanas::mostrar_panel(app);
                let _ = app.emit("panel://ajustes", ());
            }
            "pausa" => {
                // La casilla ya ha cambiado de estado al hacer clic
                let activo = casilla.is_checked().unwrap_or(false);
                config::fijar_pausa(app, activo);
            }
            "salir" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|bandeja, evento| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = evento
            {
                ventanas::alternar_panel(bandeja.app_handle().clone());
            }
        });
    if let Some(icono) = app.default_window_icon() {
        constructor = constructor.icon(icono.clone());
    }
    constructor.build(app)?;
    Ok(())
}
