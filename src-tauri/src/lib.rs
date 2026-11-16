mod calendario;
mod config;
mod estado;
mod meteo;
mod notas;
mod sistema;
mod ventanas;

use estado::Estado;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Una segunda instancia muestra el panel de la primera
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            ventanas::mostrar_panel(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--oculto"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::set_config,
            ventanas::alternar_panel,
            ventanas::ocultar_panel,
            sistema::muestra_sistema,
            calendario::estado_calendario,
            calendario::sincronizar_calendario,
            meteo::estado_meteo,
            meteo::buscar_ciudad,
            notas::estado_notas,
            notas::guardar_tarea,
            notas::guardar_nota,
            notas::borrar_tarea,
            notas::borrar_nota,
            notas::sincronizar_notas,
            notas::carpeta_onedrive,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = config::cargar(&handle);
            config::sincronizar_autoarranque(&handle, cfg.autostart);
            app.manage(Estado::nuevo(cfg));
            // Siempre arranca solo con la pestaña (con o sin --oculto)
            ventanas::preparar(&handle);
            sistema::iniciar(&handle);
            calendario::iniciar(&handle);
            meteo::iniciar(&handle);
            notas::iniciar(&handle);
            ventanas::iniciar_deteccion(handle);
            Ok(())
        })
        .on_window_event(|ventana, evento| {
            if ventana.label() == "panel" {
                if let WindowEvent::Focused(false) = evento {
                    ventanas::pedir_ocultar(ventana.app_handle());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error al ejecutar netrunner-panel");
}
