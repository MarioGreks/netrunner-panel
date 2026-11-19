//! Consumo de Claude: lee `usage.json` (lo escribe el plugin de Claude), mantiene el historial
//! del % de cada ventana y calcula ritmo y estimaciones como la app Consumo.
//!
//! Gasto de recursos mínimo: un hilo que cada 60 s mira solo la fecha de modificación de
//! `usage.json`; si no cambió, no hace nada más.

pub mod limites;
#[cfg(test)]
mod pruebas;

use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager, State};

use limites::{
    apuntar, calcular_cinco, calcular_semanal, cargar_hist, guardar_hist, leer_uso,
    renovar_ventanas, EstadoVentana, Muestra, Uso, RETENER_5H, RETENER_SEMANAL, VIEJO,
};

/// Cada cuánto despierta el hilo (solo para comprobar la fecha de usage.json).
const INTERVALO: Duration = Duration::from_secs(60);
/// Proceso de la app Consumo, que mientras siga instalada también escribe el historial.
const PROCESO_CONSUMO: &str = "Consumo.exe";

/// Lo que ve la interfaz.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoConsumo {
    /// Sin usage.json, sin ventanas o con datos de más de 6 h.
    pub sin_datos: bool,
    /// Segundos desde el `ts` de usage.json; None si falta.
    pub antiguedad: Option<i64>,
    pub semanal: EstadoVentana,
    pub cinco_horas: EstadoVentana,
}

/// Calcula el estado a un instante dado (s desde la época). Pura: no toca disco.
pub fn calcular_estado(
    uso: &Uso,
    hist: &[Muestra],
    hist5: &[Muestra],
    ahora: f64,
) -> EstadoConsumo {
    let uso = renovar_ventanas(uso, ahora);
    let hay_ventanas = uso.semanal.is_some() || uso.cinco_horas.is_some();
    let antiguedad = (uso.ts > 0.0).then(|| (ahora - uso.ts).max(0.0) as i64);
    EstadoConsumo {
        sin_datos: !hay_ventanas || uso.ts <= 0.0 || ahora - uso.ts > VIEJO,
        antiguedad,
        semanal: calcular_semanal(uso.semanal.as_ref(), hist, ahora),
        cinco_horas: calcular_cinco(uso.cinco_horas.as_ref(), hist5, ahora),
    }
}

/// Datos en memoria; solo se rehacen cuando cambia usage.json (o el historial en disco).
#[derive(Default)]
struct Interno {
    uso: Uso,
    hist: Vec<Muestra>,
    hist5: Vec<Muestra>,
    mtime_uso: Option<SystemTime>,
    mtime_hist: Option<SystemTime>,
    mtime_hist5: Option<SystemTime>,
    /// Hay muestras en memoria que aún no están en disco.
    sucio_hist: bool,
    sucio_hist5: bool,
}

pub struct Consumo {
    interno: Mutex<Interno>,
    carpeta: Option<PathBuf>,
}

fn ahora_s() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

fn mtime(ruta: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(ruta).and_then(|m| m.modified()).ok()
}

/// La app Consumo está en marcha (enumera procesos: solo llamar justo antes de escribir).
fn consumo_en_marcha() -> bool {
    let Ok(mut sys) = System::new() else {
        return false;
    };
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let hay = sys
        .processes_by_exact_name(OsStr::new(PROCESO_CONSUMO))
        .next()
        .is_some();
    hay
}

fn calcular_ahora(c: &Consumo) -> EstadoConsumo {
    let g = c.interno.lock().unwrap_or_else(|e| e.into_inner());
    calcular_estado(&g.uso, &g.hist, &g.hist5, ahora_s())
}

/// Estado actual del consumo (los cálculos dependen de la hora: se hacen al pedirlos).
#[tauri::command]
pub fn estado_consumo(consumo: State<'_, Consumo>) -> EstadoConsumo {
    calcular_ahora(&consumo)
}

/// Lee usage.json si cambió su fecha, apunta las muestras y avisa a la interfaz.
fn vuelta(app: &AppHandle, c: &Consumo) {
    let Some(carpeta) = c.carpeta.as_ref() else {
        return;
    };
    let ruta_uso = carpeta.join("usage.json");
    let (ruta_h, ruta_h5) = (
        carpeta.join("consumo_historial.json"),
        carpeta.join("consumo_historial_5h.json"),
    );
    let m = mtime(&ruta_uso);
    let hay_que_escribir = {
        let mut g = c.interno.lock().unwrap_or_else(|e| e.into_inner());
        if g.mtime_uso == m {
            return; // sin cambios: ni se lee ni se calcula
        }
        g.mtime_uso = m;
        g.uso = leer_uso(&ruta_uso);
        // El historial se relee solo si otro (la app Consumo) lo cambió en disco.
        let (mh, mh5) = (mtime(&ruta_h), mtime(&ruta_h5));
        if mh != g.mtime_hist {
            g.hist = cargar_hist(&ruta_h);
            g.mtime_hist = mh;
            g.sucio_hist = false;
        }
        if mh5 != g.mtime_hist5 {
            g.hist5 = cargar_hist(&ruta_h5);
            g.mtime_hist5 = mh5;
            g.sucio_hist5 = false;
        }
        let ahora = ahora_s();
        let uso = renovar_ventanas(&g.uso, ahora);
        let (s7, s5) = (uso.semanal, uso.cinco_horas);
        g.sucio_hist |= apuntar(&mut g.hist, s7.as_ref(), ahora, RETENER_SEMANAL);
        g.sucio_hist5 |= apuntar(&mut g.hist5, s5.as_ref(), ahora, RETENER_5H);
        g.sucio_hist || g.sucio_hist5
    };

    // Solo se enumeran procesos si hay algo que escribir; con Consumo.exe en marcha escribe él.
    if hay_que_escribir && !consumo_en_marcha() {
        let mut g = c.interno.lock().unwrap_or_else(|e| e.into_inner());
        if g.sucio_hist && guardar_hist(&ruta_h, &g.hist).is_ok() {
            g.sucio_hist = false;
            g.mtime_hist = mtime(&ruta_h);
        }
        if g.sucio_hist5 && guardar_hist(&ruta_h5, &g.hist5).is_ok() {
            g.sucio_hist5 = false;
            g.mtime_hist5 = mtime(&ruta_h5);
        }
    }
    let _ = app.emit("consumo://estado", calcular_ahora(c));
}

/// Registra el estado y arranca el hilo muestreador.
pub fn iniciar(app: &AppHandle) {
    let carpeta = app
        .path()
        .home_dir()
        .ok()
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .map(|h| h.join(".claude").join("statusbar"));
    app.manage(Consumo {
        interno: Mutex::new(Interno::default()),
        carpeta,
    });
    let app = app.clone();
    thread::spawn(move || loop {
        vuelta(&app, &app.state::<Consumo>());
        thread::sleep(INTERVALO);
    });
}
