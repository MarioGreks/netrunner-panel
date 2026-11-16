mod accesos;
mod bandeja;
mod calendario;
mod config;
mod cuentas;
mod estado;
mod meteo;
mod musica;
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
            config::abrir_carpeta_datos,
            ventanas::alternar_panel,
            ventanas::ocultar_panel,
            sistema::muestra_sistema,
            calendario::estado_calendario,
            calendario::sincronizar_calendario,
            cuentas::estado_cuentas,
            cuentas::marcar_cuenta,
            cuentas::desmarcar_cuenta,
            meteo::estado_meteo,
            meteo::buscar_ciudad,
            notas::estado_notas,
            notas::guardar_tarea,
            notas::guardar_nota,
            notas::borrar_tarea,
            notas::borrar_nota,
            notas::sincronizar_notas,
            notas::carpeta_onedrive,
            musica::estado_musica,
            musica::musica_alternar,
            musica::musica_siguiente,
            musica::musica_anterior,
            accesos::estado_accesos,
            accesos::anadir_acceso,
            accesos::renombrar_acceso,
            accesos::borrar_acceso,
            accesos::reordenar_accesos,
            accesos::abrir_acceso,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = config::cargar(&handle);
            config::sincronizar_autoarranque(&handle, cfg.autostart);
            app.manage(Estado::nuevo(cfg));
            // Siempre arranca solo con la pestaña (con o sin --oculto)
            ventanas::preparar(&handle);
            sistema::iniciar(&handle);
            cuentas::iniciar(&handle);
            calendario::iniciar(&handle);
            meteo::iniciar(&handle);
            notas::iniciar(&handle);
            musica::iniciar(&handle);
            accesos::iniciar(&handle);
            bandeja::crear(&handle)?;
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
