//! Consumo de Claude: lee `usage.json` (lo escribe el plugin de Claude), mantiene el historial
//! del % de cada ventana y calcula ritmo y estimaciones como la app Consumo.
//!
//! Gasto de recursos mínimo: un hilo que cada 60 s mira solo la fecha de modificación de
//! `usage.json`; si no cambió, no hace nada más.

//! Detalle por transcripciones: un lector incremental de `~/.claude/projects` que corre cada
//! 10 min, y el contexto por sesión, que solo se calcula (cada 5 s) mientras la interfaz tenga a
//! la vista la pestaña CONSUMO o la ventana flotante.

pub mod contexto;
pub mod limites;
pub mod rutas;
pub mod transcripciones;
#[cfg(test)]
mod pruebas;
#[cfg(test)]
mod pruebas_detalle;

use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager, State};

use contexto::{Contexto, SesionContexto, CTX_MAX};
use transcripciones::{raiz_proyectos, Lector, CUBO};

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
    app.manage(Detalle::nuevo());
    let app = app.clone();
    let a = app.clone();
    thread::spawn(move || loop {
        vuelta(&a, &a.state::<Consumo>());
        thread::sleep(INTERVALO);
    });
    let a = app.clone();
    thread::spawn(move || hilo_contexto(&a));
    thread::spawn(move || hilo_lector(&app));
}

// --- Detalle por transcripciones y contexto por sesión ---

/// Ventana de contexto con la que se calcula. El ajuste del usuario vendrá en otro encargo:
/// cambiar solo esto.
const CTX_MAX_ACTUAL: u64 = CTX_MAX;
/// Cada cuánto se recalcula el contexto mientras la vista está a la vista.
const CADA_CONTEXTO: Duration = Duration::from_secs(5);
/// Primera lectura de transcripciones tras arrancar.
const INICIO_LECTOR: Duration = Duration::from_secs(30);
/// Al hacerse visible la vista, se relee si la última lectura es más vieja que esto.
const LECTURA_VIEJA: Duration = Duration::from_secs(60);

/// Estado compartido del detalle: el lector, el contexto y lo último que se calculó.
pub struct Detalle {
    lector: Mutex<Lector>,
    contexto: Mutex<Contexto>,
    /// Último contexto calculado (None: aún no se ha calculado).
    ultimo: Mutex<Option<Vec<SesionContexto>>>,
    /// id de sesión -> id del chat de la app de Claude, de las sesiones del último cálculo.
    chats: Mutex<std::collections::HashMap<String, String>>,
    /// La pestaña CONSUMO o la ventana flotante se ven.
    visible: AtomicBool,
    /// Para despertar al hilo de contexto al instante cuando cambia `visible`.
    aviso_ctx: (Mutex<()>, Condvar),
    /// Petición de lectura anticipada al hilo del lector.
    pedir_lectura: (Mutex<bool>, Condvar),
    leyendo: AtomicBool,
    ultima_lectura: Mutex<Option<Instant>>,
}

fn bloquear<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Detalle {
    fn nuevo() -> Detalle {
        Detalle {
            lector: Mutex::new(Lector::new(raiz_proyectos())),
            contexto: Mutex::new(Contexto::real()),
            ultimo: Mutex::new(None),
            chats: Mutex::new(Default::default()),
            visible: AtomicBool::new(false),
            aviso_ctx: (Mutex::new(()), Condvar::new()),
            pedir_lectura: (Mutex::new(false), Condvar::new()),
            leyendo: AtomicBool::new(false),
            ultima_lectura: Mutex::new(None),
        }
    }
}

/// Último contexto calculado por sesión (vacío hasta el primer cálculo: se calcula y se emite
/// `consumo://contexto` en cuanto `vista_consumo(true)` avisa de que se ve).
#[tauri::command]
pub fn contexto_consumo(detalle: State<'_, Detalle>) -> Vec<SesionContexto> {
    bloquear(&detalle.ultimo).clone().unwrap_or_default()
}

/// Abre el chat de una sesión en la app de Claude (`claude://claude.ai/epitaxy/<chat>`).
#[tauri::command]
pub fn abrir_chat_consumo(id: String, detalle: State<'_, Detalle>) -> Result<(), String> {
    let chat = bloquear(&detalle.chats).get(&id).cloned();
    let chat = chat.ok_or("La sesión no tiene chat en la app de Claude")?;
    if chat.is_empty() || !chat.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err("Id de chat no válido".into());
    }
    crate::accesos::abrir_shell(format!("claude://claude.ai/epitaxy/{chat}"))
}

/// La interfaz avisa de que la pestaña CONSUMO o la ventana flotante se ven (o dejan de verse):
/// solo mientras se ven corre el hilo de contexto.
#[tauri::command]
pub fn vista_consumo(visible: bool, detalle: State<'_, Detalle>) {
    {
        let _g = bloquear(&detalle.aviso_ctx.0);
        detalle.visible.store(visible, Ordering::SeqCst);
        detalle.aviso_ctx.1.notify_all();
    }
    if !visible || detalle.leyendo.load(Ordering::SeqCst) {
        return;
    }
    let vieja = bloquear(&detalle.ultima_lectura).is_none_or(|t| t.elapsed() > LECTURA_VIEJA);
    if vieja {
        *bloquear(&detalle.pedir_lectura.0) = true;
        detalle.pedir_lectura.1.notify_all();
    }
}

/// Recalcula el contexto, lo guarda y, si cambió (o `siempre`), avisa a la interfaz.
fn recalcular_contexto(app: &AppHandle, d: &Detalle, siempre: bool) {
    let lista = bloquear(&d.contexto).calcular(ahora_s(), CTX_MAX_ACTUAL);
    {
        let g = bloquear(&d.contexto);
        let mut chats = bloquear(&d.chats);
        chats.clear();
        for s in lista.iter().filter(|s| s.tiene_chat) {
            if let Some(c) = g.id_chat(&s.id) {
                chats.insert(s.id.clone(), c);
            }
        }
    }
    let cambio = bloquear(&d.ultimo).as_ref() != Some(&lista);
    if cambio || siempre {
        *bloquear(&d.ultimo) = Some(lista.clone());
        let _ = app.emit("consumo://contexto", lista);
    }
}

/// Hilo del contexto: dormido mientras la vista está oculta (se despierta con la `Condvar`, sin
/// sondeo); a la vista, cada 5 s.
fn hilo_contexto(app: &AppHandle) {
    let d = app.state::<Detalle>();
    loop {
        let mut siempre = false;
        {
            let g = bloquear(&d.aviso_ctx.0);
            if !d.visible.load(Ordering::SeqCst) {
                siempre = true; // al volver a verse, la interfaz recibe el estado aunque no haya cambiado
                let _g = d
                    .aviso_ctx
                    .1
                    .wait_while(g, |_| !d.visible.load(Ordering::SeqCst))
                    .unwrap_or_else(|e| e.into_inner());
            }
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            recalcular_contexto(app, &d, siempre)
        }));
        let g = bloquear(&d.aviso_ctx.0);
        let _ = d
            .aviso_ctx
            .1
            .wait_timeout_while(g, CADA_CONTEXTO, |_| d.visible.load(Ordering::SeqCst));
    }
}

/// Hilo del lector: 30 s después de arrancar, y luego cada 10 min (o antes si la vista se hace
/// visible con la última lectura vieja). Avisa con `consumo://detalle` (sin carga).
fn hilo_lector(app: &AppHandle) {
    let d = app.state::<Detalle>();
    let mut espera = INICIO_LECTOR;
    loop {
        {
            let g = bloquear(&d.pedir_lectura.0);
            let (mut g, _) = d
                .pedir_lectura
                .1
                .wait_timeout_while(g, espera, |p| !*p)
                .unwrap_or_else(|e| e.into_inner());
            *g = false;
        }
        d.leyendo.store(true, Ordering::SeqCst);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bloquear(&d.lector).actualizar(None)
        }));
        *bloquear(&d.ultima_lectura) = Some(Instant::now());
        d.leyendo.store(false, Ordering::SeqCst);
        let _ = app.emit("consumo://detalle", ());
        espera = Duration::from_secs(CUBO as u64);
    }
}
