mod accesos;
mod actualizador;
mod ajustes;
mod bandeja;
mod calendario;
mod config;
mod consumo;
mod cuentas;
mod estado;
mod interfaz;
mod meteo;
mod microsoft;
mod musica;
mod notas;
mod outlook;
mod pantalla_completa;
mod pendientes;
mod sistema;
mod sync;
mod tarjetas;
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::set_config,
            config::abrir_carpeta_datos,
            ventanas::alternar_panel,
            ventanas::ocultar_panel,
            ventanas::ajustar_mini,
            ventanas::ir_a_evento,
            ventanas::abrir_ajustes_consumo,
            ventanas::estado_pantalla_completa,
            sistema::muestra_sistema,
            calendario::estado_calendario,
            calendario::sincronizar_calendario,
            consumo::estado_consumo,
            consumo::contexto_consumo,
            consumo::abrir_chat_consumo,
            consumo::vista_consumo,
            consumo::serie_consumo,
            consumo::ventana::abrir_ventana_consumo,
            tarjetas::abrir_tarjeta,
            tarjetas::tarjetas_abiertas,
            tarjetas::cerrar_tarjeta,
            tarjetas::arrastre_tarjeta,
            tarjetas::fantasma_mostrar,
            tarjetas::fantasma_mover,
            tarjetas::fantasma_ocultar,
            consumo::detalle_consumo,
            consumo::historico_vista::historico_consumo,
            consumo::historico_vista::semanas_archivadas,
            consumo::informe::generar_informe_consumo,
            consumo::informe::generar_historico_consumo,
            consumo::vigilancia::diagnostico_consumo,
            consumo::vigilancia::instalar_plugin_consumo,
            consumo::vigilancia::datos_consumo,
            consumo::ajustes::ajustes_consumo,
            consumo::ajustes::guardar_ajustes_consumo,
            consumo::ajustes::vista_historico,
            consumo::ajustes::guardar_vista_historico_consumo,
            consumo::equipos::equipo_consumo,
            consumo::aviso::aviso_consumo_actual,
            consumo::aviso::cerrar_aviso_consumo,
            cuentas::estado_cuentas,
            cuentas::marcar_cuenta,
            cuentas::desmarcar_cuenta,
            microsoft::estado_cuentas_ms,
            microsoft::conectar_cuenta_ms,
            microsoft::desconectar_cuenta_ms,
            microsoft::renombrar_cuenta_ms,
            microsoft::probar_cuenta_ms,
            meteo::estado_meteo,
            meteo::buscar_ciudad,
            notas::estado_notas,
            notas::guardar_tarea,
            notas::guardar_nota,
            notas::borrar_tarea,
            notas::borrar_nota,
            notas::sincronizar_notas,
            notas::carpeta_onedrive,
            interfaz::estado_interfaz,
            interfaz::guardar_interfaz,
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
            outlook::estado_correo,
            outlook::sincronizar_correo,
            outlook::abrir_correo,
            outlook::abrir_outlook,
            outlook::categorias_outlook,
            outlook::crear_evento,
            outlook::borrar_evento,
            actualizador::buscar_actualizacion,
            actualizador::info_actualizacion,
            actualizador::instalar_actualizacion,
            actualizador::posponer_actualizacion,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = config::cargar(&handle);
            config::sincronizar_autoarranque(&handle, cfg.autostart);
            app.manage(Estado::nuevo(cfg));
            // Siempre arranca solo con la pestaña (con o sin --oculto)
            ventanas::preparar(&handle);
            tarjetas::iniciar(&handle);
            sistema::iniciar(&handle);
            cuentas::iniciar(&handle);
            interfaz::iniciar(&handle);
            ajustes::iniciar(&handle);
            microsoft::iniciar(&handle);
            pendientes::iniciar(&handle);
            calendario::iniciar(&handle);
            consumo::iniciar(&handle);
            meteo::iniciar(&handle);
            notas::iniciar(&handle);
            musica::iniciar(&handle);
            accesos::iniciar(&handle);
            outlook::iniciar(&handle);
            actualizador::iniciar(&handle);
            bandeja::crear(&handle)?;
            ventanas::iniciar_deteccion(handle);
            Ok(())
        })
        .on_window_event(|ventana, evento| {
            // Los eventos de ventana pueden llegar antes de que `setup` registre el estado
            if ventana.label() == "panel" && ventana.app_handle().try_state::<Estado>().is_some() {
                // Durante el arrastre de una tarjeta el panel pierde el foco sin que deba cerrarse
                if let WindowEvent::Focused(false) = evento {
                    if tarjetas::arrastre_activo(ventana.app_handle()) {
                        return;
                    }
                    ventanas::pedir_ocultar(ventana.app_handle());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error al ejecutar netrunner-panel");
}
