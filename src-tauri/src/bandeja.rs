//! Icono de la bandeja del sistema: clic izquierdo alterna el panel y el menú (clic derecho)
//! ofrece abrir, ajustes, pausar la apertura por borde, buscar actualizaciones, ocultar la app
//! sin cerrarla y salir.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::actualizador;
use crate::config;
use crate::estado::Estado;
use crate::ventanas;
use crate::widgets;

/// Elemento «Ocultar / Mostrar Netrunner Panel»: su texto sigue al estado de `ventanas`.
struct ItemOcultar(MenuItem<tauri::Wry>);

/// Casilla «Editar widgets»: se sincroniza con el modo edición de `widgets`.
struct ItemEdicion(CheckMenuItem<tauri::Wry>);

/// Pone al día la casilla cuando el modo edición cambia desde otro sitio.
pub fn actualizar_edicion(app: &AppHandle, activa: bool) {
    if let Some(item) = app.try_state::<ItemEdicion>() {
        let _ = item.0.set_checked(activa);
    }
}

pub fn etiqueta_ocultar(oculto: bool) -> &'static str {
    if oculto {
        "Mostrar Netrunner Panel"
    } else {
        "Ocultar Netrunner Panel"
    }
}

/// Pone al día el texto del elemento tras cambiar el estado de ocultación.
pub fn actualizar_oculto(app: &AppHandle) {
    if let Some(item) = app.try_state::<ItemOcultar>() {
        let _ = item.0.set_text(etiqueta_ocultar(ventanas::esta_oculto()));
    }
}

pub fn crear(app: &AppHandle) -> tauri::Result<()> {
    let pausado = app.state::<Estado>().config.lock().unwrap().borde_pausado;

    let abrir = MenuItem::with_id(app, "abrir", "Abrir panel", true, None::<&str>)?;
    let ajustes = MenuItem::with_id(app, "ajustes", "Ajustes", true, None::<&str>)?;
    let pausa = CheckMenuItem::with_id(app, "pausa", "Pausar apertura por borde", true, pausado, None::<&str>)?;
    let actualizar = MenuItem::with_id(app, "actualizar", "Buscar actualizaciones", true, None::<&str>)?;
    let edicion = CheckMenuItem::with_id(app, "widgets_editar", "Editar widgets", true, widgets::edicion_activa(app), None::<&str>)?;
    app.manage(ItemEdicion(edicion.clone()));
    let ocultar = MenuItem::with_id(app, "ocultar", etiqueta_ocultar(ventanas::esta_oculto()), true, None::<&str>)?;
    app.manage(ItemOcultar(ocultar.clone()));
    let salir = MenuItem::with_id(app, "salir", "Salir", true, None::<&str>)?;
    let separador = PredefinedMenuItem::separator(app)?;
    let separador2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&abrir, &ajustes, &pausa, &separador, &actualizar, &edicion, &ocultar, &separador2, &salir],
    )?;

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
            "actualizar" => actualizador::buscar_desde_bandeja(app),
            "widgets_editar" => widgets::fijar_edicion(app, !widgets::edicion_activa(app)),
            "ocultar" => ventanas::fijar_oculto(app, !ventanas::esta_oculto()),
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

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn etiqueta_segun_el_estado() {
        assert_eq!(etiqueta_ocultar(false), "Ocultar Netrunner Panel");
        assert_eq!(etiqueta_ocultar(true), "Mostrar Netrunner Panel");
    }
}
