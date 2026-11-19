//! Consumo de Claude: lee `usage.json` (lo escribe el plugin de Claude), mantiene el historial
//! del % de cada ventana y calcula ritmo y estimaciones como la app Consumo.
//!
//! Gasto de recursos mínimo: un hilo que cada 60 s mira solo la fecha de modificación de
//! `usage.json`; si no cambió, no hace nada más.

//! Detalle por transcripciones: un lector incremental de `~/.claude/projects` que corre cada
//! 10 min, y el contexto por sesión, que solo se calcula (cada 5 s) mientras la interfaz tenga a
//! la vista la pestaña CONSUMO o la ventana flotante.
//!
//! Reparto y equipos: tras cada lectura se publica lo propio en las carpetas de sincronización,
//! se lee lo de los demás equipos y se reparte el % semanal (`reparto`, `equipos`). El uso y el
//! historial del % son los de la cuenta: se fusionan con los de los demás equipos.

pub mod agregado;
pub mod ajustes;
pub mod archivar;
pub mod archivo;
pub mod aviso;
pub mod contexto;
pub mod disco;
pub mod equipos;
pub mod historico;
pub mod historico_vista;
pub mod informe;
pub mod informe_svg;
pub mod limites;
pub mod plugin;
pub mod ventana;
pub mod reparto;
pub mod rutas;
pub mod serie;
pub mod transcripciones;
pub mod vigilancia;
#[cfg(test)]
mod pruebas;
#[cfg(test)]
mod pruebas_archivo;
#[cfg(test)]
mod pruebas_ajustes;
#[cfg(test)]
mod pruebas_detalle;
#[cfg(test)]
mod pruebas_informe;
#[cfg(test)]
mod pruebas_plugin;
#[cfg(test)]
mod pruebas_reparto;

use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager, State};

use ajustes::Hechos;
use archivar::{archivar, Entrada, EstadoArchivo, Medidas, ResultadoArchivo};
use archivo::CacheArchivos;
use contexto::{Contexto, SesionContexto};
use equipos::{cubos_json, cubos_propios, fusionar_hist, fusionar_uso, Equipos};
use reparto::{calcular_detalle, unir_cubos, CuboEq, DetalleConsumo};
use transcripciones::{raiz_proyectos, Cubo, Lector, CUBO};

use limites::{
    apuntar_con, calcular_cinco, calcular_semanal, cargar_hist, guardar_hist, leer_uso,
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
    /// Uso de la cuenta: lo más reciente entre este equipo y los demás.
    uso: Uso,
    /// Lo que ve el plugin de este equipo (usage.json); es lo que se publica.
    uso_local: Uso,
    /// Historial propio (el único que se escribe) y fusionado con el de los demás (`todas*`).
    hist: Vec<Muestra>,
    hist5: Vec<Muestra>,
    todas: Vec<Muestra>,
    todas5: Vec<Muestra>,
    /// Firma de los ficheros de los otros equipos que se fusionaron por última vez.
    firma_otros: Option<u64>,
    /// El `consumo.json` propio no se pudo publicar del todo (Consumo.exe abierto, OneDrive a medias).
    pub_pendiente: bool,
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
    /// Sincronización con los otros equipos (carpetas, cachés y lo ya publicado).
    equipos: Mutex<Equipos>,
    /// No escribe nada en disco (la prueba de humo, que es solo de lectura).
    solo_lectura: bool,
    /// Ventana de contexto con la que se calcula (ajuste del usuario; se lee al arrancar).
    ctx_max: AtomicU64,
    /// Avisos del 80 % / 100 % ya dados.
    hechos: Mutex<Hechos>,
    /// Qué se archivó ya (archivo semanal): solo se rehace lo que cambió.
    archivo: Mutex<EstadoArchivo>,
    /// Ficheros del historial leídos con caché y último histórico calculado.
    lectura: Mutex<historico_vista::LecturaHistorico>,
}

impl Consumo {
    fn nuevo(carpeta: Option<PathBuf>, equipos: Equipos) -> Consumo {
        let ctx_max = carpeta.as_ref().map_or(contexto::CTX_MAX, |d| {
            ajustes::leer_ajustes(&d.join(ajustes::FICHERO_AJUSTES)).0
        });
        Consumo {
            interno: Mutex::new(Interno::default()),
            carpeta,
            equipos: Mutex::new(equipos),
            solo_lectura: false,
            ctx_max: AtomicU64::new(ctx_max),
            hechos: Mutex::new(Hechos::default()),
            archivo: Mutex::new(EstadoArchivo::default()),
            lectura: Mutex::new(historico_vista::LecturaHistorico::default()),
        }
    }

    /// Las carpetas `historial` de las carpetas de sincronización que existen, o la local si no hay.
    pub(crate) fn carpetas_historial(&self) -> Vec<PathBuf> {
        let activas = bloquear(&self.equipos).activas();
        disco::carpetas_historial(&activas, &self.carpeta.clone().unwrap_or_default())
    }

    /// Suscripción en $/mes (ajuste `suscripcion`; por defecto 20): la usa el resumen.
    pub(crate) fn suscripcion_usd(&self) -> f64 {
        self.carpeta.as_ref().map_or(20.0, |d| ajustes::leer_suscripcion_usd(&d.join(ajustes::FICHERO_AJUSTES)))
    }

    /// Precio de la suscripción en €/mes (ajuste `suscripcion_eur`; por defecto 18).
    pub(crate) fn suscripcion_eur(&self) -> f64 {
        self.carpeta
            .as_ref()
            .map_or(historico::PRECIO_EUR, |d| ajustes::leer_suscripcion(&d.join(ajustes::FICHERO_AJUSTES)))
    }
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
    calcular_estado(&g.uso, &g.todas, &g.todas5, ahora_s())
}

/// Estado actual del consumo (los cálculos dependen de la hora: se hacen al pedirlos).
#[tauri::command]
pub fn estado_consumo(consumo: State<'_, Consumo>) -> EstadoConsumo {
    calcular_ahora(&consumo)
}

/// Refresca lo que depende de ficheros: lee usage.json y los `consumo.json` de los otros equipos
/// si cambiaron, los fusiona, apunta las muestras y publica el `consumo.json` propio si cambió.
/// True si hay algo nuevo que avisar a la interfaz.
fn refrescar(c: &Consumo, mut proceso: impl FnMut() -> bool) -> bool {
    let Some(carpeta) = c.carpeta.as_ref() else {
        return false;
    };
    let ruta_uso = carpeta.join("usage.json");
    let (ruta_h, ruta_h5) = (
        carpeta.join("consumo_historial.json"),
        carpeta.join("consumo_historial_5h.json"),
    );
    let m = mtime(&ruta_uso);
    // Los de los otros equipos: solo se vuelve a leer lo que cambió (caché por fecha y tamaño).
    let (otros, firma) = bloquear(&c.equipos).leer_consumo();
    let mut marcha: Option<bool> = None;
    let mut en_marcha = || *marcha.get_or_insert_with(|| proceso());
    let mut g = c.interno.lock().unwrap_or_else(|e| e.into_inner());
    if g.mtime_uso == m && g.firma_otros == Some(firma) && !g.pub_pendiente {
        return false; // sin cambios: ni se lee ni se calcula
    }
    if g.mtime_uso != m {
        g.mtime_uso = m;
        g.uso_local = leer_uso(&ruta_uso);
    }
    g.firma_otros = Some(firma);
    // El historial propio se relee solo si otro (la app Consumo) lo cambió en disco.
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
    // El uso y el historial son los de la cuenta: lo más reciente de todos los equipos.
    let g = &mut *g;
    g.uso = fusionar_uso(&g.uso_local, &otros);
    let mut todas = fusionar_hist(&g.hist, otros.iter().map(|o| &o.hist[..]));
    let mut todas5 = fusionar_hist(&g.hist5, otros.iter().map(|o| &o.hist5[..]));
    let uso = renovar_ventanas(&g.uso, ahora);
    let a7 = apuntar_con(&mut g.hist, &todas, uso.semanal.as_ref(), ahora, RETENER_SEMANAL);
    let a5 = apuntar_con(&mut g.hist5, &todas5, uso.cinco_horas.as_ref(), ahora, RETENER_5H);
    if a7 {
        todas = fusionar_hist(&g.hist, otros.iter().map(|o| &o.hist[..]));
    }
    if a5 {
        todas5 = fusionar_hist(&g.hist5, otros.iter().map(|o| &o.hist5[..]));
    }
    (g.todas, g.todas5) = (todas, todas5);
    g.sucio_hist |= a7;
    g.sucio_hist5 |= a5;

    // Solo se enumeran procesos si hay algo que escribir; con Consumo.exe en marcha escribe él.
    if (g.sucio_hist || g.sucio_hist5) && !c.solo_lectura && !en_marcha() {
        if g.sucio_hist && guardar_hist(&ruta_h, &g.hist).is_ok() {
            g.sucio_hist = false;
            g.mtime_hist = mtime(&ruta_h);
        }
        if g.sucio_hist5 && guardar_hist(&ruta_h5, &g.hist5).is_ok() {
            g.sucio_hist5 = false;
            g.mtime_hist5 = mtime(&ruta_h5);
        }
    }
    // Publica lo propio en las carpetas de sincronización (solo si cambió; nunca con Consumo.exe).
    let p = bloquear(&c.equipos).publicar_consumo(&g.uso_local, &g.hist, &g.hist5, || !en_marcha());
    g.pub_pendiente = p.pendiente;
    true
}

/// Lee usage.json si cambió su fecha (o lo de otros equipos) y avisa a la interfaz. Cuando hubo
/// cambios, mira si hay que dar un aviso del 80 % / 100 %.
fn vuelta(app: &AppHandle, c: &Consumo) {
    if refrescar(c, consumo_en_marcha) {
        let _ = app.emit("consumo://estado", calcular_ahora(c));
        avisar(app, c);
    }
}

/// Avisos del 80 % y del 100 % con el uso fusionado. Con Consumo.exe abierto no se hace nada
/// (avisa él); los avisos desactivados se dan por hechos sin mostrar.
fn avisar(app: &AppHandle, c: &Consumo) {
    let Some(carpeta) = c.carpeta.as_ref() else {
        return;
    };
    if c.solo_lectura {
        return;
    }
    let uso = bloquear(&c.interno).uso.clone();
    let activos = ajustes::leer_ajustes(&carpeta.join(ajustes::FICHERO_AJUSTES)).1;
    let nuevos = ajustes::procesar_avisos(
        &mut bloquear(&c.hechos),
        &carpeta.join(ajustes::FICHERO_AVISOS),
        &uso,
        ahora_s(),
        activos,
        || !consumo_en_marcha(),
    );
    if !nuevos.is_empty() {
        let limite = nuevos.iter().any(|a| a.claves.iter().any(|k| k.2 >= 100));
        let texto = nuevos.iter().map(|a| a.texto.as_str()).collect::<Vec<_>>().join("\n\n");
        aviso::mostrar(app, limite, &texto);
    }
}


/// Registra el estado y arranca el hilo muestreador.
pub fn iniciar(app: &AppHandle) {
    let home = app
        .path()
        .home_dir()
        .ok()
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_default();
    let carpeta = home.join(".claude").join("statusbar");
    app.manage(Consumo::nuevo(
        Some(carpeta.clone()),
        Equipos::real(&carpeta, &home),
    ));
    app.manage(Detalle::nuevo());
    app.manage(aviso::AvisoConsumo::default());
    app.manage(vigilancia::Vigilancia::nuevo(home));
    let app = app.clone();
    let a = app.clone();
    thread::spawn(move || loop {
        vuelta(&a, &a.state::<Consumo>());
        // Diagnóstico automático del plugin: solo mira memoria salvo que haya síntoma.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| vigilancia::vigilar_plugin(&a)));
        thread::sleep(INTERVALO);
    });
    let a = app.clone();
    thread::spawn(move || hilo_contexto(&a));
    thread::spawn(move || hilo_lector(&app));
}

// --- Detalle por transcripciones y contexto por sesión ---

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
    /// Qué vistas se ven ahora (la pestaña CONSUMO y/o la ventana flotante).
    visibles: Mutex<Origenes>,
    /// Para despertar al hilo de contexto al instante cuando cambia `visible`.
    aviso_ctx: (Mutex<()>, Condvar),
    /// Petición de lectura anticipada al hilo del lector.
    pedir_lectura: (Mutex<bool>, Condvar),
    leyendo: AtomicBool,
    ultima_lectura: Mutex<Option<Instant>>,
    /// Último reparto calculado tras una lectura (None: aún no hay).
    resultado: Mutex<Option<DetalleConsumo>>,
}

fn bloquear<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Vistas que están a la vista. El hilo de contexto trabaja si hay alguna: así la pestaña y la
/// ventana no se apagan una a la otra.
#[derive(Debug, Default)]
pub struct Origenes(std::collections::HashSet<&'static str>);

/// Orígenes válidos de `vista_consumo`.
pub const ORIGENES: [&str; 2] = ["pestana", "ventana"];

impl Origenes {
    /// Marca un origen como visible o no. Falso si el origen no es de los conocidos.
    pub fn fijar(&mut self, origen: &str, visible: bool) -> bool {
        let Some(o) = ORIGENES.iter().find(|o| **o == origen) else {
            return false;
        };
        if visible {
            self.0.insert(o);
        } else {
            self.0.remove(o);
        }
        true
    }

    pub fn alguno(&self) -> bool {
        !self.0.is_empty()
    }
}

impl Detalle {
    fn hay_vista(&self) -> bool {
        bloquear(&self.visibles).alguno()
    }

    fn nuevo() -> Detalle {
        Detalle::con_lector(Lector::new(raiz_proyectos()))
    }

    fn con_lector(lector: Lector) -> Detalle {
        Detalle {
            lector: Mutex::new(lector),
            contexto: Mutex::new(Contexto::real()),
            ultimo: Mutex::new(None),
            chats: Mutex::new(Default::default()),
            visibles: Mutex::new(Origenes::default()),
            aviso_ctx: (Mutex::new(()), Condvar::new()),
            pedir_lectura: (Mutex::new(false), Condvar::new()),
            leyendo: AtomicBool::new(false),
            ultima_lectura: Mutex::new(None),
            resultado: Mutex::new(None),
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

/// La interfaz avisa de que una vista de CONSUMO (`origen`: "pestana" o "ventana") se ve o deja de
/// verse: el hilo de contexto corre mientras alguna se vea.
#[tauri::command]
pub fn vista_consumo(origen: String, visible: bool, detalle: State<'_, Detalle>) -> Result<(), String> {
    fijar_vista(&detalle, &origen, visible)
}

pub(crate) fn fijar_vista(detalle: &Detalle, origen: &str, visible: bool) -> Result<(), String> {
    let alguna = {
        let _g = bloquear(&detalle.aviso_ctx.0);
        if !bloquear(&detalle.visibles).fijar(origen, visible) {
            return Err(format!("Origen de vista no válido: {origen}"));
        }
        let alguna = detalle.hay_vista();
        detalle.aviso_ctx.1.notify_all();
        alguna
    };
    if !visible || !alguna || detalle.leyendo.load(Ordering::SeqCst) {
        return Ok(());
    }
    let vieja = bloquear(&detalle.ultima_lectura).is_none_or(|t| t.elapsed() > LECTURA_VIEJA);
    if vieja {
        *bloquear(&detalle.pedir_lectura.0) = true;
        detalle.pedir_lectura.1.notify_all();
    }
    Ok(())
}

/// Serie del historial de la semana (y de las 5 h) para la gráfica de ritmo. Del historial
/// fusionado que ya está en memoria: no lee ficheros.
#[tauri::command]
pub fn serie_consumo(consumo: State<'_, Consumo>) -> serie::SerieConsumo {
    let g = consumo.interno.lock().unwrap_or_else(|e| e.into_inner());
    let ahora = ahora_s();
    let uso = renovar_ventanas(&g.uso, ahora);
    serie::calcular_serie(uso.semanal.as_ref(), uso.cinco_horas.as_ref(), &g.todas, &g.todas5, ahora)
}

/// Recalcula el contexto, lo guarda y, si cambió (o `siempre`), avisa a la interfaz.
fn recalcular_contexto(app: &AppHandle, d: &Detalle, siempre: bool) {
    let ctx_max = app.state::<Consumo>().ctx_max.load(Ordering::SeqCst);
    let lista = bloquear(&d.contexto).calcular(ahora_s(), ctx_max);
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
            if !d.hay_vista() {
                siempre = true; // al volver a verse, la interfaz recibe el estado aunque no haya cambiado
                let _g = d
                    .aviso_ctx
                    .1
                    .wait_while(g, |_| !d.hay_vista())
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
            .wait_timeout_while(g, CADA_CONTEXTO, |_| d.hay_vista());
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
        let c = app.state::<Consumo>();
        let calculado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ciclo_detalle_con(&c, &d, ahora_s(), false, || !consumo_en_marcha())
        }));
        let mut archivado = false;
        if let Ok(Some((det, arch))) = calculado {
            *bloquear(&d.resultado) = Some(det);
            archivado = arch.escritos + arch.puenteados > 0;
        }
        *bloquear(&d.ultima_lectura) = Some(Instant::now());
        d.leyendo.store(false, Ordering::SeqCst);
        let _ = app.emit("consumo://detalle", ());
        if archivado {
            let _ = app.emit("consumo://archivo", ());
        }
        // Espacio de datos: como mucho cada 30 min.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| vigilancia::medir_disco(app)));
        espera = Duration::from_secs(CUBO as u64);
    }
}

/// Lo que se hace tras cada lectura (y solo entonces): leer lo nuevo de las transcripciones,
/// publicar los cubos propios si cambiaron, leer los de los demás equipos, repartir el % semanal
/// y archivar la semana (solo si cambió). None si no hay semana (sin `resetsAt`). `permitido`
/// (true = se puede escribir: Consumo.exe no está abierto) se pregunta solo si hay algo que
/// escribir, y una sola vez por ciclo.
#[cfg(test)]
fn ciclo_detalle(
    c: &Consumo,
    d: &Detalle,
    ahora: f64,
    permitido: impl FnMut() -> bool,
) -> Option<DetalleConsumo> {
    ciclo_detalle_con(c, d, ahora, false, permitido).map(|x| x.0)
}

fn ciclo_detalle_con(
    c: &Consumo,
    d: &Detalle,
    ahora: f64,
    forzar: bool,
    mut permitido: impl FnMut() -> bool,
) -> Option<(DetalleConsumo, ResultadoArchivo)> {
    let mut permiso: Option<bool> = None;
    let mut ok = || *permiso.get_or_insert_with(|| permitido());
    let propios_c: Vec<Cubo> = {
        let mut l = bloquear(&d.lector);
        l.actualizar(None);
        l.cubos()
    };
    let (yo, otros_det, otros_cons, carpetas, sin_escribir) = {
        let mut e = bloquear(&c.equipos);
        e.publicar_detalle(cubos_json(&propios_c), &mut ok);
        let carpetas = disco::carpetas_historial(&e.activas(), &c.carpeta.clone().unwrap_or_default());
        (e.yo.clone(), e.leer_detalle(), e.leer_consumo().0, carpetas, e.solo_lectura)
    };
    let propios: Vec<CuboEq> = cubos_propios(&propios_c, &yo);
    let mut listas: Vec<&[CuboEq]> = vec![&propios[..]];
    listas.extend(otros_det.iter().map(|o| &o.cubos[..]));
    let todos = unir_cubos(&listas);
    let (todas, reset, pct) = {
        let g = bloquear(&c.interno);
        let sev = renovar_ventanas(&g.uso, ahora).semanal;
        (g.todas.clone(), sev.as_ref().and_then(|v| v.resets_at), sev.map(|v| v.pct))
    };
    let reset = reset?;
    let otros_ts: Vec<(String, f64)> = otros_cons.iter().map(|o| (o.machine.clone(), o.ts)).collect();
    let det = calcular_detalle(&todas, reset, &todos, &otros_ts, pct, ahora);
    let mut arch = ResultadoArchivo::default();
    if !c.solo_lectura && !sin_escribir {
        let entrada = Entrada { yo: &yo, reset, pct, propios: &propios_c, todos: &todos, todas: &todas };
        let mut est = bloquear(&c.archivo);
        let mut lectura = bloquear(&c.lectura);
        let cache: &mut CacheArchivos = &mut lectura.cache;
        arch = archivar(
            &mut est,
            cache,
            &entrada,
            &carpetas,
            ahora,
            forzar,
            || {
                let mut l = bloquear(&d.lector);
                Medidas { costes: l.costes(), cache: l.cache(), tiempos: l.tiempos() }
            },
            &mut ok,
        );
    }
    Some((det, arch))
}

/// Guarda ya en el archivo histórico lo de ahora (sin esperar a los 5 min ni a que cambie la
/// firma) para que un informe lleve lo último. Respeta la comprobación de Consumo.exe: con él
/// abierto no escribe, y el informe sale con los archivos que ya hay. Solo a petición del usuario.
pub(crate) fn forzar_archivo(app: &AppHandle) {
    let (c, d) = (app.state::<Consumo>(), app.state::<Detalle>());
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ciclo_detalle_con(&c, &d, ahora_s(), true, || !consumo_en_marcha())));
    if let Ok(Some((_, arch))) = r {
        if arch.escritos + arch.puenteados > 0 {
            let _ = app.emit("consumo://archivo", ());
        }
    }
}

/// Último reparto calculado (no calcula: se rehace tras cada lectura de transcripciones, cada
/// 10 min). None hasta la primera lectura o si aún no hay semana.
#[tauri::command]
pub fn detalle_consumo(detalle: State<'_, Detalle>) -> Option<DetalleConsumo> {
    bloquear(&detalle.resultado).clone()
}
