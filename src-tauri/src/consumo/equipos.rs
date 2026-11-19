//! Sincronización entre equipos, como Consumo: cada PC deja en las carpetas de sincronización
//! (por defecto `OneDrive\Claude-consumo`) un `<PC>.consumo.json` (uso y historial) y un
//! `<PC>.detalle.json` (sus cubos), y lee los de los demás.
//!
//! Gasto mínimo: los ficheros de otros equipos solo se vuelven a leer si cambió su fecha o tamaño,
//! y solo se escribe si cambió el contenido (OneDrive sube cada escritura). Un fichero roto o a
//! medias de otro equipo se descarta: nunca tumba la app.
//!
//! No se porta `puentear` (copiar de una carpeta a otra lo de los demás) ni la sincronización de
//! colores, disposición y ajustes de la interfaz de Consumo.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::limites::{limpiar_hist, Muestra, Uso, Ventana};
use super::reparto::CuboEq;
use super::rutas::nt_normcase;
use super::transcripciones::Cubo;

/// Nombre del equipo en los ficheros (el mismo valor que usa Consumo).
pub fn nombre_equipo() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
}

/// El nombre de este equipo, para marcarlo entre los de EQUIPOS.
#[tauri::command]
pub fn equipo_consumo() -> String {
    nombre_equipo()
}

fn es_num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

fn norm(ruta: &str) -> String {
    nt_normcase(ruta.trim_end_matches(['\\', '/']))
}

/// Las carpetas de sincronización elegidas, la principal primero: `fichero` (`consumo_sync`) lleva
/// una ruta por línea (la primera es la principal; las demás, extras, p. ej. la carpeta de otra
/// cuenta de OneDrive). Sin nada, `<onedrive>\Claude-consumo`.
pub fn carpetas_sync(fichero: &Path, onedrive: &Path) -> Vec<PathBuf> {
    let mut rutas: Vec<PathBuf> = Vec::new();
    if let Ok(texto) = fs::read_to_string(fichero) {
        for linea in texto.trim_start_matches('\u{feff}').lines() {
            let linea = linea.trim();
            if !linea.is_empty() && !rutas.iter().any(|r| norm(&r.to_string_lossy()) == norm(linea)) {
                rutas.push(PathBuf::from(linea));
            }
        }
    }
    if rutas.is_empty() {
        rutas.push(onedrive.join("Claude-consumo"));
    }
    rutas
}

/// Guarda las carpetas (la principal primero, sin repetir ni vacías) en `fichero`, una por línea.
/// Lista vacía: borra el fichero (vuelve la carpeta por defecto). True si el contenido cambió.
pub fn guardar_carpetas(fichero: &Path, carpetas: &[String]) -> io::Result<bool> {
    let mut rutas: Vec<&str> = Vec::new();
    for c in carpetas.iter().map(|c| c.trim()).filter(|c| !c.is_empty()) {
        if !rutas.iter().any(|r| norm(r) == norm(c)) {
            rutas.push(c);
        }
    }
    let antes = fs::read_to_string(fichero).ok();
    if rutas.is_empty() {
        if antes.is_some() {
            fs::remove_file(fichero)?;
            return Ok(true);
        }
        return Ok(false);
    }
    let texto = rutas.join("\n");
    if antes.as_deref().map(|t| t.trim_start_matches('\u{feff}')) == Some(texto.as_str()) {
        return Ok(false);
    }
    let carpeta = fichero.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(carpeta)?;
    let nombre = fichero.file_name().and_then(|n| n.to_str()).unwrap_or("consumo_sync");
    escribir_atomico(carpeta, nombre, &texto)?;
    Ok(true)
}

/// La base de OneDrive por defecto: `%OneDriveConsumer%` o `~/OneDrive`.
pub fn onedrive_defecto(home: &Path) -> PathBuf {
    std::env::var_os("OneDriveConsumer")
        .filter(|v| !v.is_empty())
        .map_or_else(|| home.join("OneDrive"), PathBuf::from)
}

/// Las carpetas que existen ahora, sin repetir (una que OneDrive aún no ha creado se salta).
pub fn carpetas_activas(todas: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for c in todas {
        if c.is_dir() && !out.iter().any(|o| norm(&o.to_string_lossy()) == norm(&c.to_string_lossy())) {
            out.push(c.clone());
        }
    }
    out
}

// ---------- lo que publican los demás ----------

/// Lo publicado por otro equipo tiene una marca de tiempo (para quedarse con lo más reciente).
pub trait Publicado {
    fn ts(&self) -> f64;
}

/// Contenido limpio de `<PC>.consumo.json` de otro equipo.
#[derive(Clone, Debug, PartialEq)]
pub struct OtroConsumo {
    pub machine: String,
    pub ts: f64,
    pub uso: Uso,
    pub hist: Vec<Muestra>,
    pub hist5: Vec<Muestra>,
}

/// Contenido limpio de `<PC>.detalle.json` de otro equipo (cada cubo ya lleva su equipo).
#[derive(Clone, Debug, PartialEq)]
pub struct OtroDetalle {
    pub machine: String,
    pub ts: f64,
    pub cubos: Vec<CuboEq>,
}

impl Publicado for OtroConsumo {
    fn ts(&self) -> f64 {
        self.ts
    }
}

impl Publicado for OtroDetalle {
    fn ts(&self) -> f64 {
        self.ts
    }
}

fn maquina(o: &serde_json::Map<String, Value>) -> String {
    match o.get("machine") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => "?".into(),
    }
}

fn ventana(v: &Value) -> Option<Ventana> {
    let o = v.as_object()?;
    let pct = es_num(o.get("pct"))?;
    let resets_at = match o.get("resetsAt") {
        None | Some(Value::Null) => None,
        Some(r) => Some(r.as_f64()?),
    };
    Some(Ventana { pct, resets_at })
}

/// Lo que publica otro equipo (`<PC>.consumo.json`) solo con lo que tiene la forma esperada, o
/// None. Un fichero a medias, antiguo o roto no puede tirar la app.
pub fn limpiar_consumo(v: &Value) -> Option<OtroConsumo> {
    let o = v.as_object()?;
    let uso = o.get("uso").and_then(Value::as_object);
    let de = |k: &str| uso.and_then(|u| u.get(k)).and_then(ventana);
    Some(OtroConsumo {
        machine: maquina(o),
        ts: es_num(o.get("ts")).unwrap_or(0.0),
        uso: Uso {
            cinco_horas: de("fiveHour"),
            semanal: de("sevenDay"),
            ts: 0.0,
        },
        hist: o.get("hist").map(limpiar_hist).unwrap_or_default(),
        hist5: o.get("hist5").map(limpiar_hist).unwrap_or_default(),
    })
}

/// Un cubo `[inicio, proyecto, sesión, modelo, peso, tokens(, tarea)]` bien formado; el de un
/// equipo con Consumo antiguo no trae la tarea (queda ``).
fn cubo_de(c: &Value, equipo: &str) -> Option<CuboEq> {
    let a = c.as_array()?;
    if a.len() < 6 {
        return None;
    }
    let s = |i: usize| a[i].as_str().map(str::to_string);
    let tarea = match a.get(6) {
        None => String::new(),
        Some(t) => t.as_str()?.to_string(),
    };
    Some(CuboEq {
        inicio: a[0].as_f64()?,
        proyecto: s(1)?,
        sesion: s(2)?,
        modelo: s(3)?,
        peso: a[4].as_f64()?,
        tokens: a[5].as_f64()?,
        tarea,
        equipo: equipo.to_string(),
    })
}

/// Lo mismo para `<PC>.detalle.json`: los cubos bien formados, o None.
pub fn limpiar_detalle(v: &Value) -> Option<OtroDetalle> {
    let o = v.as_object()?;
    let lista = o.get("cubos")?.as_array()?;
    let machine = maquina(o);
    let cubos = lista.iter().filter_map(|c| cubo_de(c, &machine)).collect();
    Some(OtroDetalle {
        ts: es_num(o.get("ts")).unwrap_or(0.0),
        machine,
        cubos,
    })
}

/// Clave de un fichero para saber si cambió: (fecha de modificación, tamaño).
type ClaveFichero = (Option<SystemTime>, u64);

/// Caché de lo leído de los ficheros de otros equipos: solo se vuelve a leer si cambió la clave.
pub struct Cache<T> {
    m: HashMap<PathBuf, (ClaveFichero, Option<Arc<T>>)>,
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Cache { m: HashMap::new() }
    }
}

impl<T> Cache<T> {
    /// Un fichero ya limpio (de la caché si no cambió) y su clave; None si falta, está a medias o
    /// roto (y se vuelve a intentar en la siguiente).
    fn leer(
        &mut self,
        ruta: &Path,
        limpiar: fn(&Value) -> Option<T>,
    ) -> Option<(Option<Arc<T>>, ClaveFichero)> {
        let Ok(meta) = fs::metadata(ruta) else {
            self.m.remove(ruta);
            return None;
        };
        let clave = (meta.modified().ok(), meta.len());
        if let Some((k, dato)) = self.m.get(ruta) {
            if *k == clave {
                return Some((dato.clone(), clave));
            }
        }
        let dato = fs::read_to_string(ruta)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok());
        let Some(dato) = dato else {
            self.m.remove(ruta);
            return Some((None, clave));
        };
        let limpio = limpiar(&dato).map(Arc::new);
        self.m.insert(ruta.to_path_buf(), (clave, limpio.clone()));
        Some((limpio, clave))
    }
}

fn hash<H: Hash + ?Sized>(x: &H) -> u64 {
    let mut h = DefaultHasher::new();
    x.hash(&mut h);
    h.finish()
}

/// Lo que han publicado los demás equipos en las carpetas de sincronización (`<PC><sufijo>`). Un
/// equipo que sale en más de una carpeta cuenta una vez: el de `ts` más reciente. Devuelve también
/// una firma de los ficheros vistos (cambia si cambia cualquiera) para no recalcular sin motivo.
pub fn leer_otros<T: Publicado>(
    cache: &mut Cache<T>,
    carpetas: &[PathBuf],
    sufijo: &str,
    yo: &str,
    limpiar: fn(&Value) -> Option<T>,
) -> (Vec<Arc<T>>, u64) {
    let propio = format!("{yo}{sufijo}");
    let mut mejores: Vec<(String, Arc<T>)> = Vec::new();
    let mut vistos: HashSet<PathBuf> = HashSet::new();
    let mut firma = 0u64;
    for carpeta in carpetas {
        let Ok(rd) = fs::read_dir(carpeta) else {
            continue;
        };
        for e in rd.flatten() {
            let nombre = e.file_name().to_string_lossy().into_owned();
            if !nombre.ends_with(sufijo) || nombre.eq_ignore_ascii_case(&propio) {
                continue;
            }
            let ruta = e.path();
            vistos.insert(ruta.clone());
            let Some((dato, clave)) = cache.leer(&ruta, limpiar) else {
                continue;
            };
            firma = firma.wrapping_add(hash(&(&ruta, clave)));
            let Some(dato) = dato else {
                continue;
            };
            match mejores.iter_mut().find(|(n, _)| *n == nombre) {
                Some(m) => {
                    if dato.ts() > m.1.ts() {
                        m.1 = dato;
                    }
                }
                None => mejores.push((nombre, dato)),
            }
        }
    }
    cache.m.retain(|k, _| vistos.contains(k));
    (mejores.into_iter().map(|m| m.1).collect(), firma)
}

// ---------- fusionar lo de todos los equipos ----------

/// De dos lecturas de la misma ventana de equipos distintos, la que vale. Sin depender de que los
/// relojes coincidan: manda la ventana más nueva (el reset más lejano); dentro de la misma
/// (reset ±10 min), el % más alto, porque solo sube. Sin reset en alguna de las dos, la leída más
/// tarde.
fn mejor_ventana(a: &Ventana, b: &Ventana, ts_a: f64, ts_b: f64) -> bool {
    let (ra, rb) = (a.resets_at.unwrap_or(0.0), b.resets_at.unwrap_or(0.0));
    if ra != 0.0 && rb != 0.0 {
        if (ra - rb).abs() > 600.0 {
            return ra > rb;
        }
        return a.pct >= b.pct;
    }
    ts_a >= ts_b
}

/// El uso de la cuenta (el límite es de la cuenta, no del equipo) a partir de lo que ve este
/// equipo y lo que publican los demás. Conserva el `ts` del local.
pub fn fusionar_uso(local: &Uso, otros: &[Arc<OtroConsumo>]) -> Uso {
    let mut out = local.clone();
    let (mut ts5, mut ts7) = (local.ts, local.ts);
    let mut orden: Vec<&Arc<OtroConsumo>> = otros.iter().collect();
    orden.sort_by(|a, b| a.ts.total_cmp(&b.ts));
    for o in orden {
        for (v, actual, ts) in [
            (&o.uso.cinco_horas, &mut out.cinco_horas, &mut ts5),
            (&o.uso.semanal, &mut out.semanal, &mut ts7),
        ] {
            let Some(v) = v else {
                continue;
            };
            let gana = match actual {
                None => true,
                Some(a) => !mejor_ventana(a, v, *ts, o.ts),
            };
            if gana {
                *actual = Some(v.clone());
                *ts = o.ts;
            }
        }
    }
    out
}

/// Historial propio unido al de los demás, ordenado y sin repetidos.
pub fn fusionar_hist<'a>(
    propio: &[Muestra],
    otros: impl Iterator<Item = &'a [Muestra]>,
) -> Vec<Muestra> {
    let mut v: Vec<Muestra> = propio.to_vec();
    for o in otros {
        v.extend_from_slice(o);
    }
    v.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    v.dedup();
    v
}

// ---------- publicar lo propio ----------

/// Escribe `nombre` en `carpeta` a un `.tmp` y lo renombra (no queda cortado si se cierra a medias).
pub fn escribir_atomico(carpeta: &Path, nombre: &str, texto: &str) -> io::Result<()> {
    let tmp = carpeta.join(format!("{nombre}.tmp"));
    fs::write(&tmp, texto)?;
    fs::rename(&tmp, carpeta.join(nombre)).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Qué pasó al intentar publicar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Publicacion {
    /// Carpetas en las que se escribió.
    pub escritas: usize,
    /// Quedan carpetas por escribir (no se permitió, o falló alguna: se reintenta).
    pub pendiente: bool,
}

/// Recuerda qué contenido se publicó ya y en qué carpetas: solo se escribe lo que cambió.
#[derive(Default)]
pub struct Publicador {
    firma: Option<u64>,
    hechas: HashSet<PathBuf>,
}

impl Publicador {
    /// Deja `nombre` con el texto en cada carpeta donde aún no esté ese contenido (`firma`). El
    /// texto solo se construye si hay algo que escribir, y `permitido` solo se pregunta entonces
    /// (con él se comprueba que no esté abierta la app Consumo, que enumera procesos).
    pub fn publicar(
        &mut self,
        nombre: &str,
        firma: u64,
        texto: impl FnOnce() -> String,
        carpetas: &[PathBuf],
        permitido: impl FnOnce() -> bool,
    ) -> Publicacion {
        if self.firma != Some(firma) {
            self.firma = Some(firma);
            self.hechas.clear();
        }
        let faltan: Vec<&PathBuf> = carpetas.iter().filter(|c| !self.hechas.contains(*c)).collect();
        if faltan.is_empty() {
            return Publicacion::default();
        }
        if !permitido() {
            return Publicacion {
                escritas: 0,
                pendiente: true,
            };
        }
        let texto = texto();
        let mut p = Publicacion::default();
        for c in faltan {
            if escribir_atomico(c, nombre, &texto).is_ok() {
                self.hechas.insert(c.clone());
                p.escritas += 1;
            } else {
                p.pendiente = true; // OneDrive a medias: se reintenta en la siguiente vuelta
            }
        }
        p
    }
}

/// Texto de `<PC>.consumo.json`: el uso de este equipo y su historial propio.
pub fn texto_consumo(yo: &str, local: &Uso, hist: &[Muestra], hist5: &[Muestra]) -> String {
    let vent = |v: &Ventana| match v.resets_at {
        Some(r) => json!({"pct": v.pct, "resetsAt": r}),
        None => json!({"pct": v.pct}),
    };
    let mut uso = serde_json::Map::new();
    if let Some(v) = &local.cinco_horas {
        uso.insert("fiveHour".into(), vent(v));
    }
    if let Some(v) = &local.semanal {
        uso.insert("sevenDay".into(), vent(v));
    }
    json!({"machine": yo, "ts": local.ts, "uso": uso, "hist": hist, "hist5": hist5}).to_string()
}

/// Los cubos propios en el formato del fichero: `[inicio, proyecto, sesión, modelo, peso, tokens, tarea]`.
pub fn cubos_json(cubos: &[Cubo]) -> String {
    let v: Vec<(i64, &str, &str, &str, i64, u64, &str)> = cubos
        .iter()
        .map(|c| (c.inicio, &*c.proyecto, &*c.sesion, c.modelo, c.peso, c.tokens, c.tarea))
        .collect();
    serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
}

/// Los cubos propios como los ve el reparto (con este equipo como autor).
pub fn cubos_propios(cubos: &[Cubo], yo: &str) -> Vec<CuboEq> {
    cubos
        .iter()
        .map(|c| CuboEq {
            inicio: c.inicio as f64,
            proyecto: c.proyecto.clone(),
            sesion: c.sesion.clone(),
            modelo: c.modelo.to_string(),
            peso: c.peso as f64,
            tokens: c.tokens as f64,
            tarea: c.tarea.to_string(),
            equipo: yo.to_string(),
        })
        .collect()
}

/// Todo el estado de la sincronización de este equipo.
pub struct Equipos {
    pub yo: String,
    /// Fichero `consumo_sync` con las carpetas elegidas.
    fichero: PathBuf,
    onedrive: PathBuf,
    cache_consumo: Cache<OtroConsumo>,
    cache_detalle: Cache<OtroDetalle>,
    pub_consumo: Publicador,
    pub_detalle: Publicador,
    /// Desactiva toda escritura (la prueba de humo, que es solo de lectura).
    pub solo_lectura: bool,
}

impl Equipos {
    pub fn nuevo(yo: String, fichero: PathBuf, onedrive: PathBuf) -> Equipos {
        Equipos {
            yo,
            fichero,
            onedrive,
            cache_consumo: Cache::default(),
            cache_detalle: Cache::default(),
            pub_consumo: Publicador::default(),
            pub_detalle: Publicador::default(),
            solo_lectura: false,
        }
    }

    /// Con el nombre de este PC y las carpetas de `<statusbar>/consumo_sync` (o la de OneDrive).
    pub fn real(statusbar: &Path, home: &Path) -> Equipos {
        Equipos::nuevo(nombre_equipo(), statusbar.join("consumo_sync"), onedrive_defecto(home))
    }

    /// Las carpetas de sincronización que existen ahora.
    /// Todas las carpetas elegidas (existan o no), la principal primero.
    pub fn lista_sync(&self) -> Vec<PathBuf> {
        carpetas_sync(&self.fichero, &self.onedrive)
    }

    /// La carpeta que se usa si no se elige ninguna.
    pub fn carpeta_defecto(&self) -> PathBuf {
        self.onedrive.join("Claude-consumo")
    }

    /// Guarda las carpetas elegidas; True si cambiaron.
    pub fn guardar_sync(&mut self, carpetas: &[String]) -> io::Result<bool> {
        guardar_carpetas(&self.fichero, carpetas)
    }

    /// Olvida lo leído y lo publicado: tras cambiar las carpetas se vuelve a leer y publicar todo.
    pub fn invalidar(&mut self) {
        self.cache_consumo = Cache::default();
        self.cache_detalle = Cache::default();
        self.pub_consumo = Publicador::default();
        self.pub_detalle = Publicador::default();
    }

    pub fn activas(&self) -> Vec<PathBuf> {
        carpetas_activas(&carpetas_sync(&self.fichero, &self.onedrive))
    }

    /// Lo que publicaron los demás en `consumo.json` y una firma de esos ficheros y de las carpetas.
    pub fn leer_consumo(&mut self) -> (Vec<Arc<OtroConsumo>>, u64) {
        let carpetas = self.activas();
        let (v, f) = leer_otros(&mut self.cache_consumo, &carpetas, ".consumo.json", &self.yo, limpiar_consumo);
        (v, f.wrapping_add(hash(&carpetas)))
    }

    /// Lo que publicaron los demás en `detalle.json`.
    pub fn leer_detalle(&mut self) -> Vec<Arc<OtroDetalle>> {
        let carpetas = self.activas();
        leer_otros(&mut self.cache_detalle, &carpetas, ".detalle.json", &self.yo, limpiar_detalle).0
    }

    /// Publica `<YO>.consumo.json` (solo si cambió su contenido y hay `ts`).
    pub fn publicar_consumo(
        &mut self,
        local: &Uso,
        hist: &[Muestra],
        hist5: &[Muestra],
        permitido: impl FnOnce() -> bool,
    ) -> Publicacion {
        if self.solo_lectura || local.ts <= 0.0 {
            return Publicacion::default();
        }
        let texto = texto_consumo(&self.yo, local, hist, hist5);
        let carpetas = self.activas();
        let nombre = format!("{}.consumo.json", self.yo);
        self.pub_consumo
            .publicar(&nombre, hash(&texto), || texto, &carpetas, permitido)
    }

    /// Publica `<YO>.detalle.json` (`cubos` es el JSON de los cubos: solo si cambiaron).
    pub fn publicar_detalle(
        &mut self,
        cubos: String,
        permitido: impl FnOnce() -> bool,
    ) -> Publicacion {
        if self.solo_lectura {
            return Publicacion::default();
        }
        let carpetas = self.activas();
        let nombre = format!("{}.detalle.json", self.yo);
        let yo = self.yo.clone();
        let firma = hash(&cubos);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.pub_detalle.publicar(
            &nombre,
            firma,
            || {
                format!(
                    "{{\"machine\":{},\"ts\":{ts},\"cubos\":{cubos}}}",
                    serde_json::to_string(&yo).unwrap_or_else(|_| "\"?\"".into())
                )
            },
            &carpetas,
            permitido,
        )
    }
}
