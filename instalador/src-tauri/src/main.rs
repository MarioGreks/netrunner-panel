// En release no abre consola.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ajustes;
mod registro;

use ajustes::{carpeta_config_app, fusionar_config, leer_opciones, Opciones};
use serde::Serialize;
use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

const PAQUETE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/paquete_nsis.exe"));
const VERSION_NUEVA: &str = env!("NETRUNNER_VERSION");
const EXE_APP: &str = "netrunner-panel.exe";
const SIN_VENTANA: u32 = 0x0800_0000; // CREATE_NO_WINDOW
const DESLIGADO: u32 = 0x0000_0008; // DETACHED_PROCESS

#[derive(Serialize)]
struct EstadoInstalador {
    modo: &'static str,
    version_nueva: &'static str,
    version_instalada: Option<String>,
    opciones: Opciones,
}

fn es_actualizacion() -> bool {
    std::env::args().any(|a| a.eq_ignore_ascii_case("/UPDATE"))
}

#[tauri::command]
fn estado_instalador() -> EstadoInstalador {
    let instalada = registro::leer_instalacion();
    let version_instalada = instalada.and_then(|i| i.version);
    let modo = if es_actualizacion() {
        "actualizacion"
    } else if version_instalada.is_some() {
        "detectada"
    } else {
        "nueva"
    };
    let opciones = carpeta_config_app()
        .map(|d| leer_opciones(&d.join("config.json")))
        .unwrap_or_default();
    EstadoInstalador { modo, version_nueva: VERSION_NUEVA, version_instalada, opciones }
}

fn paso(app: &AppHandle, texto: &str, hecho: bool) {
    let _ = app.emit("instalador://paso", serde_json::json!({ "texto": texto, "hecho": hecho }));
}

fn progreso(app: &AppHandle, pct: f64) {
    let _ = app.emit("instalador://progreso", serde_json::json!({ "pct": pct.round() }));
}

fn app_en_marcha() -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {EXE_APP}"), "/NH", "/FO", "CSV"])
        .creation_flags(SIN_VENTANA)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains(EXE_APP))
        .unwrap_or(false)
}

fn cerrar_app() -> Result<(), String> {
    if !app_en_marcha() {
        return Ok(());
    }
    let _ = Command::new("taskkill").args(["/IM", EXE_APP, "/T", "/F"]).creation_flags(SIN_VENTANA).output();
    let limite = Instant::now() + Duration::from_secs(5);
    while Instant::now() < limite {
        if !app_en_marcha() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err("No se pudo cerrar Netrunner Panel. Ciérralo a mano y vuelve a intentarlo.".into())
}

fn ruta_temporal() -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    std::env::temp_dir().join(format!("netrunner-setup-{}-{nanos}.exe", std::process::id()))
}

/// Ejecuta el NSIS en silencio y emite progreso estimado (asintótico de 10 % hacia 90 %).
fn ejecutar_setup(app: &AppHandle, setup: &Path, actualizacion: bool) -> Result<(), String> {
    let mut cmd = Command::new(setup);
    cmd.arg("/S");
    if actualizacion {
        // /UPDATE: el NSIS conserva los accesos directos que ya existan.
        cmd.arg("/UPDATE");
    }
    let mut hijo = cmd
        .creation_flags(SIN_VENTANA)
        .spawn()
        .map_err(|e| format!("No se pudo iniciar el instalador interno: {e}"))?;
    let inicio = Instant::now();
    loop {
        match hijo.try_wait() {
            Ok(Some(estado)) => {
                return if estado.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "La instalación terminó con error (código {}).",
                        estado.code().map_or("desconocido".to_string(), |c| c.to_string())
                    ))
                };
            }
            Ok(None) => {
                let t = inicio.elapsed().as_secs_f64();
                progreso(app, 10.0 + 80.0 * (1.0 - (-t / 6.0).exp()));
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("No se pudo esperar al instalador interno: {e}")),
        }
    }
}

fn instalar_bloqueante(app: &AppHandle, opciones: Opciones) -> Result<(), String> {
    if PAQUETE.is_empty() {
        return Err("Este instalador no contiene el paquete de la aplicación.".into());
    }
    let actualizacion = es_actualizacion();
    let ajustes = carpeta_config_app()
        .ok_or_else(|| "No se encontró la carpeta de datos del usuario (APPDATA).".to_string())?
        .join("config.json");

    paso(app, "Cerrando Netrunner Panel", false);
    progreso(app, 2.0);
    cerrar_app()?;
    paso(app, "Cerrando Netrunner Panel", true);

    paso(app, "Extrayendo paquete", false);
    let setup = ruta_temporal();
    fs::write(&setup, PAQUETE).map_err(|e| format!("No se pudo extraer el paquete: {e}"))?;
    progreso(app, 8.0);
    paso(app, "Extrayendo paquete", true);

    paso(app, "Instalando archivos", false);
    // El NSIS en silencio crea también el acceso del escritorio; si no existía, se retira después.
    let lnk = registro::carpeta_escritorio().map(|d| d.join("Netrunner Panel.lnk"));
    let lnk_existia = lnk.as_ref().is_some_and(|p| p.exists());
    let resultado = ejecutar_setup(app, &setup, actualizacion);
    let _ = fs::remove_file(&setup);
    resultado?;
    if !actualizacion && !lnk_existia {
        if let Some(p) = &lnk {
            let _ = fs::remove_file(p);
        }
    }
    progreso(app, 92.0);
    paso(app, "Instalando archivos", true);

    paso(app, "Aplicando tema y borde", false);
    fusionar_config(&ajustes, &opciones)?;
    progreso(app, 100.0);
    paso(app, "Aplicando tema y borde", true);
    Ok(())
}

#[tauri::command]
async fn instalar(app: AppHandle, opciones: Opciones) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || instalar_bloqueante(&app, opciones))
        .await
        .map_err(|e| format!("La instalación se interrumpió: {e}"))?
}

#[tauri::command]
fn abrir_app() -> Result<(), String> {
    let carpeta = registro::leer_instalacion()
        .and_then(|i| i.carpeta)
        .map(PathBuf::from)
        .ok_or_else(|| "No se encontró la carpeta de instalación de Netrunner Panel.".to_string())?;
    let exe = carpeta.join(EXE_APP);
    Command::new(&exe)
        .current_dir(&carpeta)
        .creation_flags(DESLIGADO)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("No se pudo abrir Netrunner Panel: {e}"))
}

#[tauri::command]
fn salir(app: AppHandle) {
    app.exit(0);
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![estado_instalador, instalar, abrir_app, salir])
        .run(tauri::generate_context!())
        .expect("error al iniciar el instalador");
}
