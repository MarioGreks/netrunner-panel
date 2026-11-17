//! Actualizador automático con `tauri-plugin-updater` contra GitHub Releases.
//!
//! Comprueba a los 10 s de arrancar y cada 6 h. Si hay una versión nueva abre la
//! ventana HUD «aviso» (aviso.html). Estado propio: no toca `estado.rs`.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_updater::{Update, UpdaterExt};

const ESPERA_INICIAL: Duration = Duration::from_secs(10);
const INTERVALO: Duration = Duration::from_secs(6 * 60 * 60);
const ETIQUETA: &str = "aviso";

#[derive(Default)]
pub struct EstadoActualizador {
    pendiente: Mutex<Option<Update>>,
    pospuesta: Mutex<Option<String>>,
}

#[derive(Serialize, Clone, Debug)]
pub struct InfoActualizacion {
    pub version: String,
    pub version_actual: String,
    pub notas: Option<String>,
    pub fecha: Option<String>,
}

#[derive(Serialize, Clone)]
struct Progreso {
    descargado: u64,
    total: Option<u64>,
}

fn info(u: &Update) -> InfoActualizacion {
    InfoActualizacion {
        version: u.version.clone(),
        version_actual: u.current_version.clone(),
        notas: u.body.clone().filter(|n| !n.trim().is_empty()),
        // Solo la parte AAAA-MM-DD
        fecha: u.date.map(|d| d.to_string().chars().take(10).collect()),
    }
}

/// Busca una actualización y guarda la encontrada como pendiente.
async fn comprobar(app: &AppHandle) -> Result<Option<Update>, String> {
    let actualizador = app
        .updater()
        .map_err(|e| format!("No se pudo preparar la comprobación: {e}"))?;
    let hallada = actualizador
        .check()
        .await
        .map_err(|e| format!("No se pudo comprobar las actualizaciones: {e}"))?;
    let estado = app.state::<EstadoActualizador>();
    *estado.pendiente.lock().unwrap() = hallada.clone();
    Ok(hallada)
}

/// Abre la ventana de aviso o, si ya existe, la trae al frente.
fn abrir_aviso(app: &AppHandle) {
    if let Some(v) = app.get_webview_window(ETIQUETA) {
        let _ = v.show();
        let _ = v.set_focus();
        return;
    }
    let res = WebviewWindowBuilder::new(app, ETIQUETA, WebviewUrl::App("aviso.html".into()))
        .title("Netrunner Panel · actualización")
        .inner_size(420.0, 260.0)
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
        Err(e) => eprintln!("actualizador: no se pudo abrir la ventana de aviso: {e}"),
    }
}

pub fn iniciar(app: &AppHandle) {
    app.manage(EstadoActualizador::default());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio_sleep(ESPERA_INICIAL).await;
        loop {
            match comprobar(&app).await {
                Ok(Some(u)) => {
                    let pospuesta = app.state::<EstadoActualizador>().pospuesta.lock().unwrap().clone();
                    if pospuesta.as_deref() != Some(u.version.as_str()) {
                        abrir_aviso(&app);
                    }
                }
                Ok(None) => {}
                // En segundo plano los fallos (sin red, sin latest.json…) no molestan
                Err(e) => eprintln!("actualizador: {e}"),
            }
            tokio_sleep(INTERVALO).await;
        }
    });
}

async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

/// Comprobación manual (AJUSTES → SISTEMA). Si hay versión nueva abre la ventana
/// aunque estuviera pospuesta.
#[tauri::command]
pub async fn buscar_actualizacion(app: AppHandle) -> Result<Option<InfoActualizacion>, String> {
    let hallada = comprobar(&app).await?;
    Ok(hallada.map(|u| {
        abrir_aviso(&app);
        info(&u)
    }))
}

/// La ventana de aviso pide aquí la actualización pendiente al cargar.
#[tauri::command]
pub fn info_actualizacion(estado: State<'_, EstadoActualizador>) -> Option<InfoActualizacion> {
    estado.pendiente.lock().unwrap().as_ref().map(info)
}

#[tauri::command]
pub async fn instalar_actualizacion(
    app: AppHandle,
    estado: State<'_, EstadoActualizador>,
) -> Result<(), String> {
    let update = estado
        .pendiente
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No hay ninguna actualización pendiente".to_string())?;
    let mut descargado: u64 = 0;
    let app_progreso = app.clone();
    let app_fin = app.clone();
    update
        .download_and_install(
            move |trozo, total| {
                descargado += trozo as u64;
                let _ = app_progreso.emit("actualizador://progreso", Progreso { descargado, total });
            },
            move || {
                let _ = app_fin.emit("actualizador://instalando", ());
            },
        )
        .await
        .map_err(|e| format!("No se pudo instalar la actualización: {e}"))?;
    // En Windows el instalador cierra la app y NSIS la relanza; si se llega aquí, se reinicia a mano
    app.restart();
}

#[tauri::command]
pub fn posponer_actualizacion(app: AppHandle, estado: State<'_, EstadoActualizador>) {
    if let Some(u) = estado.pendiente.lock().unwrap().as_ref() {
        *estado.pospuesta.lock().unwrap() = Some(u.version.clone());
    }
    if let Some(v) = app.get_webview_window(ETIQUETA) {
        let _ = v.close();
    }
}
