//! Archivo semanal de Consumo (port de la parte de archivo de `informe.py` y de `archivar` de
//! `consumo.pyw`). Claude Code borra las transcripciones a los pocos días: cada equipo guarda, por
//! semana de Claude (de reset a reset), un fichero `<PC>.semana-AAAA-MM-DD.json` con el detalle de
//! lo que hizo (por día: % por hora, tokens por hora, lo no atribuido, % por hora y proyecto, la
//! caché y las filas proyecto/sesión/modelo/tarea). El formato es IDÉNTICO al de Consumo (los
//! equipos que aún lo usen y los archivos que ya existan siguen valiendo).
//!
//! Gasto mínimo: solo se escribe lo que cambia de contenido (OneDrive sube cada escritura, y la
//! hora de generación no cuenta), no se vuelve a repartir si no cambió nada de lo que entra en el
//! reparto, y los ficheros del historial se leen con caché por (ruta, fecha, tamaño).
//!
//! NO se porta la copia larga (`historico_largo`: respaldar_local, podar_sync, copiar_mas,
//! archivos_a_podar) ni `rellenar_tiempos_viejos`: son opciones de Consumo que aquí no existen.
//! Las funciones `rellenar_*` sí están por si se necesitan.

// Parte de la API (guardar_archivo, leer_archivos, rellenar_*) la usan las pruebas y los informes (8b/8c).
#![allow(dead_code)]

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, NaiveDate, TimeZone, Timelike};
use serde_json::{Map, Value};

use super::equipos::escribir_atomico;
use super::limites::{Muestra, SEMANA};
use super::reparto::{CuboEq, Fila, OTROS};
use super::transcripciones::{Clave, Cubo, CUBO};

/// Versión del método de reparto de lo «sin atribuir» con que se escribió el archivo (2 = cubos
/// que solapan el intervalo; sin `rep` = 1).
pub const REP: i64 = 2;
/// Tipos de resumen.
pub const TIPOS: [&str; 5] = ["semana", "mes", "trimestre", "semestre", "anio"];
const MITAD: f64 = CUBO as f64 / 2.0;

/// Nombre legible de un tipo de tarea.
pub fn nombre_tarea(k: &str) -> Option<&'static str> {
    Some(match k {
        "codigo" => "Escribir código",
        "comandos" => "Comandos",
        "explorar" => "Explorar",
        "web" => "Web",
        "herramientas" => "Otras herramientas",
        "mensajes" => "Mensajes",
        "pensar" => "Pensar",
        "" => "Sin clasificar",
        OTROS => "Sin atribuir",
        _ => return None,
    })
}

// ---------- fechas locales ----------

fn local(t: f64) -> chrono::DateTime<Local> {
    Local.timestamp_opt(t.floor() as i64, 0).earliest().unwrap_or_else(Local::now)
}

/// Fecha local de un instante (`date.fromtimestamp`).
pub fn fecha_de(t: f64) -> NaiveDate {
    local(t).date_naive()
}

pub fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// `AAAA-MM-DD` válida (exactamente `\d{4}-\d\d-\d\d`).
pub fn fecha_iso(s: &str) -> Option<NaiveDate> {
    let b = s.as_bytes();
    let forma = b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() });
    forma.then(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()).flatten()
}

fn dia_hora(t: f64) -> (String, usize) {
    let l = local(t);
    (iso(l.date_naive()), l.hour() as usize)
}

fn ahora_s() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// `round(x, n)` de Python (aproximado: x·10ⁿ redondeado).
fn redondear(x: f64, n: i32) -> f64 {
    let p = 10f64.powi(n);
    (x * p).round_ties_even() / p
}

// ---------- el archivo ----------

/// Una fila de un día: `[proyecto, sesión (8 letras), modelo, tarea, equipo, %, peso, tokens, $, segundos activos]`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FilaArch {
    pub proyecto: String,
    pub sesion: String,
    pub modelo: String,
    pub tarea: String,
    pub equipo: String,
    pub pct: f64,
    pub peso: f64,
    pub tokens: f64,
    pub coste: f64,
    pub seg: f64,
}

/// Un día del archivo: % por hora (`h`), miles de tokens por hora (`t`), % sin atribuir por hora
/// (`o`), % por hora y proyecto (`hp`), `k` = [tokens leídos de caché, $ que ahorra] (los archivos
/// antiguos no lo traen) y las filas.
#[derive(Clone, Debug, PartialEq)]
pub struct DiaArch {
    pub h: Vec<f64>,
    pub t: Vec<f64>,
    pub o: Vec<f64>,
    pub hp: BTreeMap<String, Vec<f64>>,
    pub k: Option<[f64; 2]>,
    pub f: Vec<FilaArch>,
}

impl DiaArch {
    fn nuevo(con_k: bool) -> DiaArch {
        DiaArch { h: vec![0.0; 24], t: vec![0.0; 24], o: vec![0.0; 24], hp: BTreeMap::new(), k: con_k.then_some([0.0, 0.0]), f: Vec::new() }
    }
}

/// El archivo de una semana de un equipo (ya limpio).
#[derive(Clone, Debug, PartialEq)]
pub struct Archivo {
    pub maquina: String,
    pub inicio: f64,
    pub reset: f64,
    pub pct: Option<f64>,
    pub generado: i64,
    pub rep: i64,
    pub dias: BTreeMap<String, DiaArch>,
}

impl AsRef<Archivo> for Archivo {
    fn as_ref(&self) -> &Archivo {
        self
    }
}

/// Cuánto lleva un archivo (días, filas, segundos): al repetirse en dos carpetas vale el que más lleva.
pub type Lleva = (usize, usize, f64);

impl Archivo {
    pub fn lleva(&self) -> Lleva {
        (self.dias.len(), self.dias.values().map(|d| d.f.len()).sum(), self.dias.values().flat_map(|d| d.f.iter()).map(|f| f.seg).sum())
    }
}

/// El archivo de la semana que acaba en `reset` para `maquina`. `filas` es lo que devuelve
/// `repartir` (de todos los equipos: aquí solo se toman las de `maquina` y lo no atribuido),
/// `cubos` los cubos de este equipo y `costes` lo que devuelve `Lector::costes`. Con `cache`
/// (`Lector::cache`) cada día lleva `k`; con `tiempos` (`Lector::tiempos`) las filas llevan los
/// segundos activos.
#[allow(clippy::too_many_arguments)]
pub fn construir_archivo(
    maquina: &str,
    reset: f64,
    filas: &[Fila],
    cubos: &[Cubo],
    costes: &HashMap<Clave, f64>,
    pct: Option<f64>,
    ahora: Option<f64>,
    cache: Option<&HashMap<Clave, (f64, f64)>>,
    tiempos: Option<&HashMap<Clave, f64>>,
) -> Archivo {
    let ini = reset - SEMANA;
    let mut pct_cubo: HashMap<(i64, &str, &str, &str, &str), f64> = HashMap::new();
    let mut otros: Vec<(f64, f64)> = Vec::new();
    for f in filas {
        if f.proyecto == OTROS {
            otros.push((f.t, f.pct));
        } else if f.equipo == maquina {
            let k = ((f.t - MITAD).round_ties_even() as i64, f.proyecto, f.sesion, f.modelo, f.tarea);
            *pct_cubo.entry(k).or_insert(0.0) += f.pct;
        }
    }
    let con_k = cache.is_some();
    let mut dias: BTreeMap<String, DiaArch> = BTreeMap::new();
    type Clave5 = (String, String, String, String, String);
    // [%, peso, tokens, $, segundos]
    let mut acc: BTreeMap<Clave5, [f64; 5]> = BTreeMap::new();
    for c in cubos {
        let t = c.inicio as f64;
        if !(ini <= t && t < reset) {
            continue;
        }
        let k = (c.inicio, c.proyecto.as_str(), c.sesion.as_str(), c.modelo, c.tarea);
        let (d, h) = dia_hora(t);
        let p = pct_cubo.get(&k).copied().unwrap_or(0.0);
        let ses8: String = c.sesion.chars().take(8).collect();
        let a = acc.entry((d.clone(), c.proyecto.clone(), ses8, c.modelo.to_string(), c.tarea.to_string())).or_insert([0.0; 5]);
        let ck: Clave = (c.inicio, c.proyecto.clone(), c.sesion.clone(), c.modelo, c.tarea);
        a[0] += p;
        a[1] += c.peso as f64;
        a[2] += c.tokens as f64;
        a[3] += costes.get(&ck).copied().unwrap_or(0.0);
        a[4] += tiempos.and_then(|m| m.get(&ck)).copied().unwrap_or(0.0);
        let dia = dias.entry(d).or_insert_with(|| DiaArch::nuevo(con_k));
        dia.h[h] += p;
        dia.t[h] += c.tokens as f64 / 1000.0;
        dia.hp.entry(c.proyecto.clone()).or_insert_with(|| vec![0.0; 24])[h] += p;
        if let (Some(k), Some(cache)) = (dia.k.as_mut(), cache) {
            let kc = cache.get(&ck).copied().unwrap_or((0.0, 0.0));
            k[0] += kc.0;
            k[1] += kc.1;
        }
    }
    for ((d, proy, ses, modelo, tarea), a) in acc {
        dias.entry(d).or_insert_with(|| DiaArch::nuevo(con_k)).f.push(FilaArch {
            proyecto: proy,
            sesion: ses,
            modelo,
            tarea,
            equipo: maquina.to_string(),
            pct: redondear(a[0], 3),
            peso: a[1].round_ties_even(),
            tokens: a[2],
            coste: redondear(a[3], 5),
            seg: a[4].round_ties_even(),
        });
    }
    for (t, p) in otros {
        if ini <= t && t < reset {
            let (d, h) = dia_hora(t);
            dias.entry(d).or_insert_with(|| DiaArch::nuevo(con_k)).o[h] += p;
        }
    }
    for dia in dias.values_mut() {
        dia.h.iter_mut().for_each(|x| *x = redondear(*x, 3));
        dia.t.iter_mut().for_each(|x| *x = x.round_ties_even());
        dia.o.iter_mut().for_each(|x| *x = redondear(*x, 3));
        dia.hp.retain(|_, v| v.iter().any(|x| *x != 0.0));
        for v in dia.hp.values_mut() {
            v.iter_mut().for_each(|x| *x = redondear(*x, 2));
        }
        if let Some(k) = dia.k.as_mut() {
            *k = [k[0].trunc(), redondear(k[1], 4)];
        }
    }
    Archivo { maquina: maquina.to_string(), inicio: ini.trunc(), reset: reset.trunc(), pct, generado: ahora.unwrap_or_else(ahora_s) as i64, rep: REP, dias }
}

// ---------- a y desde JSON (el formato de Consumo) ----------

/// Un entero de Python (`round(x)`) en JSON: sin parte decimal.
fn entero(x: f64) -> Value {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 9e15 {
        Value::from(x as i64)
    } else {
        Value::from(x)
    }
}

fn reales(v: &[f64]) -> Value {
    Value::Array(v.iter().map(|x| Value::from(*x)).collect())
}

/// El archivo como JSON de Consumo (`dias[d] = {h, t, o, hp, f, k?}`).
pub fn a_json(a: &Archivo) -> Value {
    let mut dias = Map::new();
    for (d, dia) in &a.dias {
        let mut o = Map::new();
        o.insert("h".into(), reales(&dia.h));
        o.insert("t".into(), Value::Array(dia.t.iter().map(|x| entero(*x)).collect()));
        o.insert("o".into(), reales(&dia.o));
        o.insert("hp".into(), Value::Object(dia.hp.iter().map(|(k, v)| (k.clone(), reales(v))).collect()));
        if let Some(k) = dia.k {
            o.insert("k".into(), Value::Array(vec![entero(k[0]), Value::from(k[1])]));
        }
        let filas = dia
            .f
            .iter()
            .map(|f| {
                Value::Array(vec![
                    f.proyecto.clone().into(),
                    f.sesion.clone().into(),
                    f.modelo.clone().into(),
                    f.tarea.clone().into(),
                    f.equipo.clone().into(),
                    Value::from(f.pct),
                    entero(f.peso),
                    entero(f.tokens),
                    Value::from(f.coste),
                    entero(f.seg),
                ])
            })
            .collect();
        o.insert("f".into(), Value::Array(filas));
        dias.insert(d.clone(), Value::Object(o));
    }
    let mut o = Map::new();
    o.insert("v".into(), 1.into());
    o.insert("maquina".into(), a.maquina.clone().into());
    o.insert("inicio".into(), entero(a.inicio));
    o.insert("reset".into(), entero(a.reset));
    o.insert("pct".into(), a.pct.map_or(Value::Null, Value::from));
    o.insert("generado".into(), a.generado.into());
    o.insert("rep".into(), a.rep.into());
    o.insert("dias".into(), Value::Object(dias));
    Value::Object(o)
}

/// Dos JSON son el mismo contenido (números por valor, sin distinguir 1 de 1.0).
fn iguales(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| iguales(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| iguales(v, w))),
        _ => a == b,
    }
}

/// `_contenido`: lo mismo sin la hora de generación.
fn mismo_contenido(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let sin = |m: &Map<String, Value>| m.iter().filter(|(k, _)| *k != "generado").count();
            sin(x) == sin(y) && x.iter().filter(|(k, _)| *k != "generado").all(|(k, v)| y.get(k).is_some_and(|w| iguales(v, w)))
        }
        _ => false,
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64(),
        _ => None,
    }
}

fn horas(v: Option<&Value>) -> Vec<f64> {
    let mut out: Vec<f64> = match v {
        Some(Value::Array(a)) => a.iter().take(24).map(|x| num(Some(x)).unwrap_or(0.0)).collect(),
        _ => Vec::new(),
    };
    out.resize(24, 0.0);
    out
}

fn fila_de(f: &Value) -> Option<FilaArch> {
    let a = f.as_array()?;
    if !(a.len() == 9 || a.len() == 10) {
        return None;
    }
    let s = |i: usize| a[i].as_str().map(str::to_string);
    let n = |i: usize| num(a.get(i));
    Some(FilaArch {
        proyecto: s(0)?,
        sesion: s(1)?,
        modelo: s(2)?,
        tarea: s(3)?,
        equipo: s(4)?,
        pct: n(5)?,
        peso: n(6)?,
        tokens: n(7)?,
        coste: n(8)?,
        seg: if a.len() == 10 { n(9)? } else { 0.0 },
    })
}

/// Un archivo leído de disco (o de otro equipo) solo con lo que tiene la forma esperada, o None:
/// uno roto no puede tirar nada.
pub fn limpiar_archivo(v: &Value) -> Option<Archivo> {
    let o = v.as_object()?;
    let dias_v = o.get("dias")?.as_object()?;
    let inicio = num(o.get("inicio"))?;
    let mut dias = BTreeMap::new();
    for (d, dv) in dias_v {
        let Some(dia_v) = dv.as_object() else {
            continue;
        };
        if fecha_iso(d).is_none() {
            continue;
        }
        let f: Vec<FilaArch> = match dia_v.get("f") {
            Some(Value::Array(l)) => l.iter().filter_map(fila_de).collect(),
            _ => Vec::new(),
        };
        let hp: BTreeMap<String, Vec<f64>> = match dia_v.get("hp") {
            Some(Value::Object(m)) => m.iter().take(60).map(|(k, v)| (k.clone(), horas(Some(v)))).collect(),
            _ => BTreeMap::new(),
        };
        let k = match dia_v.get("k") {
            Some(Value::Array(k)) if k.len() == 2 => Some([num(k.first())?, num(k.get(1))?]),
            _ => None,
        };
        dias.insert(d.clone(), DiaArch { h: horas(dia_v.get("h")), t: horas(dia_v.get("t")), o: horas(dia_v.get("o")), hp, k, f });
    }
    let maquina = match o.get("maquina") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => "?".into(),
    };
    Some(Archivo {
        maquina,
        inicio,
        reset: num(o.get("reset")).unwrap_or(inicio + SEMANA),
        pct: num(o.get("pct")),
        generado: num(o.get("generado")).unwrap_or(0.0) as i64,
        rep: o.get("rep").and_then(Value::as_i64).filter(|r| *r >= 1).unwrap_or(1),
        dias,
    })
}

pub fn nombre_archivo(a: &Archivo) -> String {
    format!("{}.semana-{}.json", a.maquina, iso(fecha_de(a.inicio)))
}

// ---------- guardar ----------

/// (días, filas) de un archivo en bruto: con lo que se compara si uno nuevo empeora a otro guardado.
fn dias_filas_json(v: &Value) -> Option<(usize, usize)> {
    let d = v.get("dias")?.as_object()?;
    let mut filas = 0;
    for dia in d.values() {
        filas += dia.get("f")?.as_array()?.len();
    }
    Some((d.len(), filas))
}

fn leer_json(ruta: &Path) -> Option<Value> {
    let t = fs::read_to_string(ruta).ok()?;
    serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()
}

/// True si `nuevo` hay que escribirlo en `carpeta`: no está o cambió lo que lleva (la hora de
/// generación no cuenta). Con `proteger`, tampoco si el que ya hay lleva más (más días o más filas:
/// lo guardó antes un equipo con más datos, o las transcripciones ya se borraron).
pub fn debe_guardar(carpeta: &Path, nombre: &str, nuevo: &Value, proteger: bool) -> bool {
    let Some(viejo) = leer_json(&carpeta.join(nombre)) else {
        return true;
    };
    if mismo_contenido(&viejo, nuevo) {
        return false;
    }
    if proteger {
        if let (Some((dv, fv)), Some((dn, fnn))) = (dias_filas_json(&viejo), dias_filas_json(nuevo)) {
            if dv > dn || fv > fnn {
                return false;
            }
        }
    }
    true
}

/// Escribe el archivo (a uno aparte y luego se cambia: si cierran la app a medias no queda cortado).
pub fn escribir_json(carpeta: &Path, nombre: &str, nuevo: &Value) -> io::Result<()> {
    fs::create_dir_all(carpeta)?;
    escribir_atomico(carpeta, nombre, &nuevo.to_string())
}

/// Guarda el archivo de la semana solo si cambió lo que lleva. True si se escribió.
pub fn guardar_archivo(carpeta: &Path, arch: &Archivo, proteger: bool) -> io::Result<bool> {
    let nuevo = a_json(arch);
    let nombre = nombre_archivo(arch);
    if !debe_guardar(carpeta, &nombre, &nuevo, proteger) {
        return Ok(false);
    }
    escribir_json(carpeta, &nombre, &nuevo)?;
    Ok(true)
}

// ---------- qué semanas archivar ----------

/// Los resets de las semanas de Claude MÁS VIEJAS que la anterior a la de `reset` (esas dos se
/// archivan siempre) que aún se pueden archivar: salen de las muestras del historial del %; un
/// reset es un grupo de muestras con el mismo reset ±10 min (vale el mayor) y la semana tiene que
/// empezar dentro de lo que retienen los cubos. De más nueva a más vieja.
pub fn resets_archivables(muestras: &[Muestra], reset: f64, ahora: f64, retener: f64) -> Vec<f64> {
    let limite = ahora - retener;
    let mut ws: Vec<f64> = muestras.iter().map(|m| m.1).collect();
    ws.sort_by(|a, b| b.total_cmp(a));
    ws.dedup();
    let mut out: Vec<f64> = Vec::new();
    for w in ws {
        if w > reset - SEMANA - 600.0 || w - SEMANA < limite || out.iter().any(|o| (w - o).abs() < 600.0) {
            continue;
        }
        out.push(w);
    }
    out
}

/// Algo que cambia cuando cambia lo que entra en el reparto de la semana que acaba en `reset`:
/// los cubos (de todos los equipos) y las muestras del % de esa semana.
pub type FirmaSemana = (usize, i64, usize, u64, u64);

pub fn firma_semana(reset: f64, cubos: &[&CuboEq], muestras: &[Muestra]) -> FirmaSemana {
    let ini = reset - SEMANA;
    let cs: Vec<&&CuboEq> = cubos.iter().filter(|c| ini <= c.inicio && c.inicio < reset).collect();
    let ms: Vec<&Muestra> = muestras.iter().filter(|m| (m.1 - reset).abs() < 600.0).collect();
    (
        cs.len(),
        cs.iter().map(|c| c.peso).sum::<f64>().round_ties_even() as i64,
        ms.len(),
        ms.iter().map(|m| m.0).fold(0.0, f64::max).to_bits(),
        ms.iter().map(|m| m.2).fold(0.0, f64::max).to_bits(),
    )
}

// ---------- leer, con caché ----------

type ClaveFichero = (Option<SystemTime>, u64);

/// Lo leído de los ficheros semanales: solo se vuelve a leer y limpiar el que cambió de fecha o
/// tamaño. Un fichero roto o a medias se descarta (y se vuelve a intentar si cambia).
#[derive(Default)]
pub struct CacheArchivos {
    m: HashMap<PathBuf, (ClaveFichero, Option<Arc<Archivo>>)>,
    /// Cuántos ficheros se han parseado de verdad (para comprobar la caché).
    pub lecturas: usize,
}

/// Los ficheros `*.semana-*.json` de una carpeta, ordenados.
pub fn listar_archivos(carpeta: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(carpeta) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.contains(".semana-") && n.ends_with(".json")
        })
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

impl CacheArchivos {
    /// Un fichero ya limpio (de la caché si no cambió); None si falta o está roto.
    pub fn leer(&mut self, ruta: &Path) -> Option<Arc<Archivo>> {
        let Ok(meta) = fs::metadata(ruta) else {
            self.m.remove(ruta);
            return None;
        };
        let clave = (meta.modified().ok(), meta.len());
        if let Some((k, dato)) = self.m.get(ruta) {
            if *k == clave {
                return dato.clone();
            }
        }
        self.lecturas += 1;
        let dato = leer_json(ruta).and_then(|v| limpiar_archivo(&v)).map(Arc::new);
        self.m.insert(ruta.to_path_buf(), (clave, dato.clone()));
        dato
    }

    /// Todos los archivos semanales de la carpeta (los de todos los equipos), ya limpios.
    pub fn leer_archivos(&mut self, carpeta: &Path) -> Vec<Arc<Archivo>> {
        listar_archivos(carpeta).iter().filter_map(|r| self.leer(r)).collect()
    }

    /// Los de varias carpetas (un equipo puede estar en más de una, p. ej. en dos cuentas de
    /// OneDrive): uno por equipo y semana, el que más lleva. Olvida lo de ficheros que ya no están.
    pub fn leer_archivos_en(&mut self, carpetas: &[PathBuf]) -> Vec<Arc<Archivo>> {
        let mut mejores: BTreeMap<(String, u64), Arc<Archivo>> = BTreeMap::new();
        let mut vistos: HashSet<PathBuf> = HashSet::new();
        for c in carpetas {
            for ruta in listar_archivos(c) {
                vistos.insert(ruta.clone());
                let Some(a) = self.leer(&ruta) else {
                    continue;
                };
                let k = (a.maquina.clone(), a.inicio.to_bits());
                if mejores.get(&k).is_none_or(|m| a.lleva() > m.lleva()) {
                    mejores.insert(k, a);
                }
            }
        }
        self.m.retain(|k, _| vistos.contains(k) || !carpetas.iter().any(|c| k.starts_with(c)));
        mejores.into_values().collect()
    }

    /// Una firma de los ficheros semanales de las carpetas (solo fecha y tamaño: no lee nada); cambia
    /// si cambia cualquiera.
    pub fn firma(&self, carpetas: &[PathBuf]) -> u64 {
        let mut h = DefaultHasher::new();
        for c in carpetas {
            c.hash(&mut h);
            for ruta in listar_archivos(c) {
                if let Ok(m) = fs::metadata(&ruta) {
                    (ruta, m.len(), m.modified().ok()).hash(&mut h);
                }
            }
        }
        h.finish()
    }
}

// ---------- puentear y rellenar ----------

/// Con más de una carpeta, pasa de una a otra los archivos semanales de los demás equipos (nunca
/// los de `yo`, que ya se guardan en todas): copia solo si en la otra falta o lleva menos.
/// Devuelve (cuántos ficheros copió, no se permitió escribir). `permitido` solo se pregunta si hay
/// algo que copiar.
pub fn puentear_archivos(
    cache: &mut CacheArchivos,
    carpetas: &[PathBuf],
    yo: &str,
    permitido: &mut dyn FnMut() -> bool,
) -> (usize, bool) {
    if carpetas.len() < 2 {
        return (0, false);
    }
    let propio = format!("{yo}.");
    let mut valor: BTreeMap<String, BTreeMap<usize, Lleva>> = BTreeMap::new();
    for (i, c) in carpetas.iter().enumerate() {
        for ruta in listar_archivos(c) {
            let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if nombre.starts_with(&propio) {
                continue;
            }
            if let Some(a) = cache.leer(&ruta) {
                valor.entry(nombre).or_default().insert(i, a.lleva());
            }
        }
    }
    let mut copiados = 0;
    let mut dado_permiso = false;
    for (nombre, por) in valor {
        let mut fuente: Option<(usize, Lleva)> = None;
        for (i, l) in &por {
            if fuente.is_none_or(|(_, m)| *l > m) {
                fuente = Some((*i, *l));
            }
        }
        let Some((fuente, mejor)) = fuente else {
            continue;
        };
        for (i, c) in carpetas.iter().enumerate() {
            if i == fuente || por.get(&i).is_some_and(|l| *l >= mejor) {
                continue;
            }
            if !dado_permiso {
                if !permitido() {
                    return (copiados, true);
                }
                dado_permiso = true;
            }
            let destino = c.join(&nombre);
            let tmp = c.join(format!("{nombre}.tmp"));
            let hecho = fs::create_dir_all(c)
                .and_then(|_| fs::copy(carpetas[fuente].join(&nombre), &tmp))
                .and_then(|_| fs::rename(&tmp, &destino));
            match hecho {
                Ok(_) => copiados += 1,
                Err(_) => {
                    let _ = fs::remove_file(&tmp);
                }
            }
        }
    }
    (copiados, false)
}

/// Rellena el 10.º campo (segundos activos) de las filas de un archivo semanal (JSON en bruto, en
/// el sitio) con `tiempos`: solo las filas que aún tienen 0 y dentro de la semana del archivo.
/// Todas las filas quedan con 10 campos si cambió algo. True si cambió algún valor.
pub fn rellenar_tiempos(arch: &mut Value, tiempos: &HashMap<Clave, f64>) -> bool {
    let Some(ini) = num(arch.get("inicio")) else {
        return false;
    };
    let fin = num(arch.get("reset")).unwrap_or(ini + SEMANA);
    let mut por_dia: HashMap<(String, String, String, String, String), f64> = HashMap::new();
    for ((t, proy, ses, modelo, tarea), seg) in tiempos {
        let t = *t as f64;
        if ini <= t && t < fin && *seg != 0.0 {
            let k = (dia_hora(t).0, proy.clone(), ses.chars().take(8).collect(), modelo.to_string(), tarea.to_string());
            *por_dia.entry(k).or_insert(0.0) += seg;
        }
    }
    let mut cambio = false;
    let Some(dias) = arch.get_mut("dias").and_then(Value::as_object_mut) else {
        return false;
    };
    for (d, dia_v) in dias.iter_mut() {
        let Some(filas) = dia_v.get_mut("f").and_then(Value::as_array_mut) else {
            continue;
        };
        for f in filas.iter_mut() {
            let Some(f) = f.as_array_mut() else {
                continue;
            };
            let s = |i: usize| f.get(i).and_then(Value::as_str).unwrap_or("").to_string();
            let k = (d.clone(), s(0), s(1).chars().take(8).collect(), s(2), s(3));
            if let Some(seg) = por_dia.get(&k) {
                let vacio = f.get(9).is_none_or(|v| v.as_f64().unwrap_or(0.0) == 0.0);
                if *seg != 0.0 && vacio {
                    f.truncate(9);
                    f.push(entero(seg.round_ties_even()));
                    cambio = true;
                }
            }
        }
    }
    if cambio {
        for dia_v in dias.values_mut() {
            if let Some(filas) = dia_v.get_mut("f").and_then(Value::as_array_mut) {
                for f in filas.iter_mut().filter_map(Value::as_array_mut) {
                    if f.len() < 10 {
                        f.push(0.into());
                    }
                }
            }
        }
    }
    cambio
}

/// Rellena los archivos semanales de `maquina` de cada carpeta (leyéndolos tal cual y guardando
/// solo los que cambian). Devuelve cuántos.
pub fn rellenar_carpetas(carpetas: &[PathBuf], tiempos: &HashMap<Clave, f64>, maquina: &str, permitido: &mut dyn FnMut() -> bool) -> usize {
    let prefijo = format!("{maquina}.semana-");
    let mut n = 0;
    for carpeta in carpetas {
        for ruta in listar_archivos(carpeta) {
            let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !nombre.starts_with(&prefijo) {
                continue;
            }
            let Some(mut a) = leer_json(&ruta) else {
                continue;
            };
            if limpiar_archivo(&a).is_none() || !rellenar_tiempos(&mut a, tiempos) || !permitido() {
                continue;
            }
            if escribir_json(carpeta, &nombre, &a).is_ok() {
                n += 1;
            }
        }
    }
    n
}

