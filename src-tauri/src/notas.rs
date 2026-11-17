//! Módulo NOTAS: tareas y notas rápidas guardadas en local (`app_data_dir()/notas.json`)
//! con sincronización opcional mediante una carpeta de OneDrive.
//!
//! El archivo compartido es `<carpeta>/netrunner-panel/notas.json`. La sincronización
//! fusiona elemento a elemento (por id) en lugar de quedarse con el archivo más nuevo,
//! de modo que cambios distintos hechos en dos equipos no se pisan. Los borrados se
//! guardan como marcas (`borrada`) durante 30 días para que se propaguen.
//!
//! Este módulo también aloja el **planificador único** de la sincronización entre
//! equipos: cada ejecución sincroniza notas y tareas, cuentas atrás, interfaz y
//! ajustes compartidos (ver `sincronizar`). Los disparadores son el cambio de datos
//! (2 s después), mostrar el panel, un temporizador de 5 min y el comando manual.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration as Espera, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::estado::Estado;
use crate::sync::{self, ahora_ms, escribir_atomico, fusionar_lista, VIDA_MARCAS_MS};

const INTERVALO_SYNC: Espera = Espera::from_secs(300);
const RETARDO_CAMBIO: Espera = Espera::from_secs(2);
const EDAD_MAX_AL_MOSTRAR: Espera = Espera::from_secs(30);
const ARCHIVO: &str = "notas.json";

// ---------------------------------------------------------------------------
// Modelo
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Tarea {
    pub id: String,
    pub texto: String,
    pub hecha: bool,
    pub orden: i64,
    pub creada: i64,
    pub modificada: i64,
    pub borrada: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Nota {
    pub id: String,
    pub titulo: String,
    pub cuerpo: String,
    pub creada: i64,
    pub modificada: i64,
    pub borrada: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Datos {
    pub tareas: Vec<Tarea>,
    pub notas: Vec<Nota>,
}

/// Lo que ve la interfaz: sin elementos borrados.
#[derive(Serialize, Clone, Debug, Default)]
pub struct EstadoNotas {
    pub tareas: Vec<Tarea>,
    pub notas: Vec<Nota>,
    /// Milisegundos UTC de la última sincronización correcta.
    pub ultima_sync: Option<i64>,
    pub sincronizando: bool,
    pub error: Option<String>,
    pub carpeta_sync: Option<String>,
}

#[derive(Default)]
struct Interno {
    datos: Datos,
    ultima_sync: Option<i64>,
    sincronizando: bool,
    /// Se pidió otra sincronización mientras había una en curso.
    pendiente: bool,
    error: Option<String>,
}

pub struct Notas {
    interno: Mutex<Interno>,
    ultimo_intento: Mutex<Option<Instant>>,
    /// Generación del último cambio, para esperar 2 s sin cambios antes de sincronizar.
    generacion: AtomicU64,
}

// ---------------------------------------------------------------------------
// Lógica pura: fusión y purga
// ---------------------------------------------------------------------------

/// Fusiona dos conjuntos de datos por id y elemento a elemento.
pub fn fusionar(local: &Datos, remoto: &Datos) -> Datos {
    Datos {
        tareas: fusionar_lista(&local.tareas, &remoto.tareas, |t| &t.id, |t| (t.modificada, t.borrada)),
        notas: fusionar_lista(&local.notas, &remoto.notas, |n| &n.id, |n| (n.modificada, n.borrada)),
    }
}

/// Elimina las marcas de borrado con más de 30 días.
pub fn purgar(datos: &mut Datos, ahora: i64) {
    let limite = ahora - VIDA_MARCAS_MS;
    datos.tareas.retain(|t| !(t.borrada && t.modificada < limite));
    datos.notas.retain(|n| !(n.borrada && n.modificada < limite));
}

/// Lee el remoto, fusiona con `local`, purga y escribe el resultado en el remoto.
/// Si el remoto no se puede leer, no se escribe nada y se devuelve el error.
pub fn sincronizar_con(local: &Datos, remoto: &Path, ahora: i64) -> Result<Datos, String> {
    sync::sincronizar_con(local, remoto, ahora, fusionar, purgar)
}

fn ruta_remota(carpeta: &str) -> PathBuf {
    sync::ruta_remota(carpeta, ARCHIVO)
}

// ---------------------------------------------------------------------------
// Almacén local y estado
// ---------------------------------------------------------------------------

fn ruta_local(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(ARCHIVO))
}

/// Carga el almacén local. Si está corrupto, lo aparta como `notas.corrupto.json`.
fn cargar_local(app: &AppHandle) -> Datos {
    let Some(r) = ruta_local(app) else { return Datos::default() };
    let Ok(texto) = std::fs::read_to_string(&r) else { return Datos::default() };
    match serde_json::from_str::<Datos>(&texto) {
        Ok(d) => d,
        Err(_) => {
            let _ = std::fs::rename(&r, r.with_file_name("notas.corrupto.json"));
            Datos::default()
        }
    }
}

fn guardar_local(app: &AppHandle, datos: &Datos) -> Result<(), String> {
    let r = ruta_local(app).ok_or("sin directorio de datos")?;
    escribir_atomico(&r, datos)
}

fn carpeta_configurada(app: &AppHandle) -> Option<String> {
    app.state::<Estado>()
        .config
        .lock()
        .unwrap()
        .carpeta_sync
        .clone()
        .filter(|c| !c.trim().is_empty())
}

fn instantanea(app: &AppHandle) -> EstadoNotas {
    let carpeta = carpeta_configurada(app);
    let n = app.state::<Notas>();
    let i = n.interno.lock().unwrap();
    let mut tareas: Vec<Tarea> = i.datos.tareas.iter().filter(|t| !t.borrada).cloned().collect();
    tareas.sort_by_key(|t| (t.orden, t.creada));
    let mut notas: Vec<Nota> = i.datos.notas.iter().filter(|n| !n.borrada).cloned().collect();
    notas.sort_by_key(|n| std::cmp::Reverse(n.modificada));
    EstadoNotas {
        tareas,
        notas,
        ultima_sync: i.ultima_sync,
        sincronizando: i.sincronizando,
        error: i.error.clone(),
        carpeta_sync: carpeta,
    }
}

fn emitir(app: &AppHandle) {
    let _ = app.emit("notas://estado", instantanea(app));
}

/// Aplica un cambio a los datos, lo guarda en local, emite el estado y programa la
/// sincronización 2 s después del último cambio.
fn modificar(app: &AppHandle, cambio: impl FnOnce(&mut Datos)) -> Result<(), String> {
    let n = app.state::<Notas>();
    {
        let mut i = n.interno.lock().unwrap();
        let mut nuevos = i.datos.clone();
        cambio(&mut nuevos);
        guardar_local(app, &nuevos)?;
        i.datos = nuevos;
    }
    emitir(app);
    programar_sync(app);
    Ok(())
}

/// Programa una sincronización 2 s después del último cambio (de cualquier módulo).
pub(crate) fn programar_sync(app: &AppHandle) {
    if carpeta_configurada(app).is_none() {
        return;
    }
    // Aún no registrado si el cambio llega durante el arranque (la sincronización
    // periódica se encargará)
    let Some(n) = app.try_state::<Notas>() else { return };
    let gen = n.generacion.fetch_add(1, Ordering::SeqCst) + 1;
    let h = app.clone();
    thread::spawn(move || {
        thread::sleep(RETARDO_CAMBIO);
        if h.state::<Notas>().generacion.load(Ordering::SeqCst) == gen {
            sincronizar(&h);
        }
    });
}

/// Paso de notas y tareas: fusiona con el remoto y guarda el resultado en local.
fn sincronizar_paso(app: &AppHandle, carpeta: &str) -> Result<(), String> {
    let n = app.state::<Notas>();
    let local = n.interno.lock().unwrap().datos.clone();
    let fusionado = sincronizar_con(&local, &ruta_remota(carpeta), ahora_ms())?;
    let mut i = n.interno.lock().unwrap();
    // Puede haber cambios locales hechos durante la sincronización: se fusionan de
    // nuevo (es idempotente) y se purga.
    let mut datos = fusionar(&i.datos, &fusionado);
    purgar(&mut datos, ahora_ms());
    guardar_local(app, &datos)?;
    i.datos = datos;
    Ok(())
}

/// Lanza una sincronización en segundo plano. Si ya hay una en curso, solo anota que
/// hay que repetirla al terminar.
pub fn sincronizar(app: &AppHandle) {
    let n = app.state::<Notas>();
    *n.ultimo_intento.lock().unwrap() = Some(Instant::now());
    let Some(carpeta) = carpeta_configurada(app) else {
        let hubo = {
            let mut i = n.interno.lock().unwrap();
            let hubo = i.ultima_sync.is_some() || i.error.is_some();
            i.ultima_sync = None;
            i.error = None;
            hubo
        };
        if hubo {
            emitir(app);
        }
        return;
    };
    {
        let mut i = n.interno.lock().unwrap();
        if i.sincronizando {
            i.pendiente = true;
            return;
        }
        i.sincronizando = true;
        i.pendiente = false;
    }
    emitir(app);

    let app = app.clone();
    thread::spawn(move || {
        // Cada archivo es un paso independiente: si uno falla (remoto corrupto o
        // bloqueado) se anota un error breve y los demás se sincronizan igualmente.
        let pasos: Vec<sync::Paso> = vec![
            ("notas", Box::new(|| sincronizar_paso(&app, &carpeta))),
            ("cuentas", Box::new(|| crate::cuentas::sincronizar(&app, &carpeta))),
            ("interfaz", Box::new(|| crate::interfaz::sincronizar(&app, &carpeta))),
            ("ajustes", Box::new(|| crate::ajustes::sincronizar(&app, &carpeta))),
        ];
        let (bien, errores) = sync::ejecutar_pasos(pasos);
        let n = app.state::<Notas>();
        let repetir = {
            let mut i = n.interno.lock().unwrap();
            i.sincronizando = false;
            if bien > 0 {
                i.ultima_sync = Some(ahora_ms());
            }
            i.error = if errores.is_empty() { None } else { Some(errores.join(" · ")) };
            std::mem::take(&mut i.pendiente)
        };
        emitir(&app);
        // La carpeta cambió o hubo peticiones durante la sincronización
        if repetir || carpeta_configurada(&app).as_deref() != Some(carpeta.as_str()) {
            sincronizar(&app);
        }
    });
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn estado_notas(app: AppHandle) -> EstadoNotas {
    instantanea(&app)
}

#[tauri::command]
pub fn guardar_tarea(app: AppHandle, mut tarea: Tarea) -> Result<(), String> {
    let ahora = ahora_ms();
    if tarea.id.trim().is_empty() {
        tarea.id = uuid::Uuid::new_v4().to_string();
    }
    tarea.modificada = ahora;
    tarea.borrada = false;
    modificar(&app, |d| match d.tareas.iter_mut().find(|t| t.id == tarea.id) {
        Some(t) => {
            tarea.creada = t.creada;
            *t = tarea;
        }
        None => {
            tarea.creada = ahora;
            d.tareas.push(tarea);
        }
    })
}

#[tauri::command]
pub fn guardar_nota(app: AppHandle, mut nota: Nota) -> Result<(), String> {
    let ahora = ahora_ms();
    if nota.id.trim().is_empty() {
        nota.id = uuid::Uuid::new_v4().to_string();
    }
    nota.modificada = ahora;
    nota.borrada = false;
    modificar(&app, |d| match d.notas.iter_mut().find(|n| n.id == nota.id) {
        Some(n) => {
            nota.creada = n.creada;
            *n = nota;
        }
        None => {
            nota.creada = ahora;
            d.notas.push(nota);
        }
    })
}

/// Marca la tarea como borrada (se conserva para propagar el borrado) y vacía su texto.
#[tauri::command]
pub fn borrar_tarea(app: AppHandle, id: String) -> Result<(), String> {
    let ahora = ahora_ms();
    modificar(&app, |d| {
        if let Some(t) = d.tareas.iter_mut().find(|t| t.id == id) {
            t.borrada = true;
            t.texto.clear();
            t.modificada = ahora;
        }
    })
}

#[tauri::command]
pub fn borrar_nota(app: AppHandle, id: String) -> Result<(), String> {
    let ahora = ahora_ms();
    modificar(&app, |d| {
        if let Some(n) = d.notas.iter_mut().find(|n| n.id == id) {
            n.borrada = true;
            n.titulo.clear();
            n.cuerpo.clear();
            n.modificada = ahora;
        }
    })
}

/// Carpeta de OneDrive del usuario (variable `OneDrive`), si existe, como punto de
/// partida del selector.
#[tauri::command]
pub fn carpeta_onedrive() -> Option<String> {
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|p| Path::new(p).is_dir())
}

#[tauri::command]
pub fn sincronizar_notas(app: AppHandle) {
    sincronizar(&app);
}

pub fn iniciar(app: &AppHandle) {
    let mut datos = cargar_local(app);
    purgar(&mut datos, ahora_ms());
    app.manage(Notas {
        interno: Mutex::new(Interno { datos, ..Default::default() }),
        ultimo_intento: Mutex::new(None),
        generacion: AtomicU64::new(0),
    });

    // Al mostrar el panel, sincroniza si la última tiene más de 30 s
    let h = app.clone();
    app.listen("panel://mostrar", move |_| {
        let vieja = h
            .state::<Notas>()
            .ultimo_intento
            .lock()
            .unwrap()
            .map_or(true, |t| t.elapsed() > EDAD_MAX_AL_MOSTRAR);
        if vieja && carpeta_configurada(&h).is_some() {
            sincronizar(&h);
        }
    });

    // Sincronización periódica (la primera, nada más arrancar)
    let h = app.clone();
    thread::spawn(move || loop {
        if carpeta_configurada(&h).is_some() {
            sincronizar(&h);
        }
        thread::sleep(INTERVALO_SYNC);
    });
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::sync::leer_remoto;

    fn tarea(id: &str, texto: &str, modificada: i64, borrada: bool) -> Tarea {
        Tarea { id: id.into(), texto: texto.into(), modificada, borrada, ..Default::default() }
    }

    fn nota(id: &str, titulo: &str, modificada: i64, borrada: bool) -> Nota {
        Nota { id: id.into(), titulo: titulo.into(), modificada, borrada, ..Default::default() }
    }

    fn datos_t(ts: Vec<Tarea>) -> Datos {
        Datos { tareas: ts, notas: vec![] }
    }

    fn dir_temporal(nombre: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("netrunner-notas-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn cambios_distintos_en_cada_lado_se_conservan() {
        let local = Datos { tareas: vec![tarea("a", "local", 10, false)], notas: vec![nota("n1", "solo local", 10, false)] };
        let remoto = Datos { tareas: vec![tarea("b", "remota", 20, false)], notas: vec![nota("n2", "solo remota", 20, false)] };
        let r = fusionar(&local, &remoto);
        assert_eq!(r.tareas.len(), 2);
        assert_eq!(r.notas.len(), 2);
        assert!(r.tareas.iter().any(|t| t.id == "a") && r.tareas.iter().any(|t| t.id == "b"));
    }

    #[test]
    fn misma_tarea_editada_gana_la_mas_reciente() {
        let local = datos_t(vec![tarea("a", "vieja", 10, false)]);
        let remoto = datos_t(vec![tarea("a", "nueva", 20, false)]);
        assert_eq!(fusionar(&local, &remoto).tareas[0].texto, "nueva");
        assert_eq!(fusionar(&remoto, &local).tareas[0].texto, "nueva");
    }

    #[test]
    fn borrado_gana_a_edicion_mas_antigua() {
        let local = datos_t(vec![tarea("a", "editada", 10, false)]);
        let remoto = datos_t(vec![tarea("a", "", 20, true)]);
        assert!(fusionar(&local, &remoto).tareas[0].borrada);
        assert!(fusionar(&remoto, &local).tareas[0].borrada);
    }

    #[test]
    fn edicion_mas_nueva_gana_a_borrado_antiguo() {
        let local = datos_t(vec![tarea("a", "revivida", 30, false)]);
        let remoto = datos_t(vec![tarea("a", "", 20, true)]);
        assert!(!fusionar(&local, &remoto).tareas[0].borrada);
    }

    #[test]
    fn empate_gana_el_borrado() {
        let local = datos_t(vec![tarea("a", "texto", 10, false)]);
        let remoto = datos_t(vec![tarea("a", "", 10, true)]);
        assert!(fusionar(&local, &remoto).tareas[0].borrada);
        assert!(fusionar(&remoto, &local).tareas[0].borrada);
        let ln = Datos { tareas: vec![], notas: vec![nota("n", "t", 5, false)] };
        let rn = Datos { tareas: vec![], notas: vec![nota("n", "", 5, true)] };
        assert!(fusionar(&ln, &rn).notas[0].borrada);
    }

    #[test]
    fn purga_marcas_de_mas_de_30_dias() {
        let ahora = 100 * 24 * 3600 * 1000;
        let dia = 24 * 3600 * 1000;
        let mut d = Datos {
            tareas: vec![
                tarea("vieja", "", ahora - 31 * dia, true),
                tarea("reciente", "", ahora - 29 * dia, true),
                tarea("viva_antigua", "x", ahora - 90 * dia, false),
            ],
            notas: vec![nota("n_vieja", "", ahora - 40 * dia, true), nota("n_viva", "x", ahora - 40 * dia, false)],
        };
        purgar(&mut d, ahora);
        let ids: Vec<_> = d.tareas.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["reciente", "viva_antigua"]);
        assert_eq!(d.notas.len(), 1);
        assert_eq!(d.notas[0].id, "n_viva");
    }

    #[test]
    fn escritura_atomica_y_relectura() {
        let dir = dir_temporal("atomica");
        let ruta = dir.join("sub").join("notas.json");
        let datos = datos_t(vec![tarea("a", "hola ñandú", 10, false)]);
        escribir_atomico(&ruta, &datos).unwrap();
        escribir_atomico(&ruta, &datos).unwrap(); // sobrescribir también funciona
        assert_eq!(leer_remoto::<Datos>(&ruta).unwrap(), Some(datos));
        // No quedan temporales
        let restos = std::fs::read_dir(ruta.parent().unwrap()).unwrap().count();
        assert_eq!(restos, 1);
        assert_eq!(leer_remoto::<Datos>(&dir.join("no_existe.json")).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remoto_corrupto_da_error_y_no_se_sobrescribe() {
        let dir = dir_temporal("corrupto");
        let ruta = dir.join("notas.json");
        std::fs::write(&ruta, "{ esto no es json").unwrap();
        let local = datos_t(vec![tarea("a", "local", 10, false)]);
        assert!(sincronizar_con(&local, &ruta, 1000).is_err());
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "{ esto no es json");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sincronizar_fusiona_y_escribe_el_remoto() {
        let dir = dir_temporal("sync");
        let ruta = dir.join(sync::SUBCARPETA).join(ARCHIVO);
        escribir_atomico(&ruta, &datos_t(vec![tarea("b", "remota", 20, false)])).unwrap();
        let local = datos_t(vec![tarea("a", "local", 10, false)]);
        let res = sincronizar_con(&local, &ruta, 1000).unwrap();
        assert_eq!(res.tareas.len(), 2);
        assert_eq!(leer_remoto::<Datos>(&ruta).unwrap().unwrap().tareas.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
