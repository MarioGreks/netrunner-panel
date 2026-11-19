//! Lector de las transcripciones de Claude Code (`~/.claude/projects`): mensajes del modelo con su
//! peso, coste, tarea y proyecto, agrupados en cubos de 10 min. Portado de `detalle.py`.
//!
//! Gasto mínimo: lectura incremental (solo lo nuevo de cada fichero, línea a línea, nunca un
//! fichero entero a memoria), los mensajes retenidos son structs de ~60 bytes sin el texto de las
//! líneas, y lo que caduca se descarta en cada pasada.

// Parte de la API (costes, tiempos, cubos...) la usará el reparto por proyecto (encargo 3).
#![allow(dead_code)]

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::DateTime;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;

use super::rutas::{buscar_cwd, proyecto_repo, tiene_git, CacheRepos};

/// s: los mensajes se agrupan en cubos de 10 min (es lo que se publicará a los otros equipos).
pub const CUBO: i64 = 600;
/// s de historia que se conservan (35 días).
pub const RETENER: f64 = 35.0 * 86_400.0;
/// s: entre dos mensajes seguidos de una sesión, un hueco menor es tiempo activo; uno mayor, pausa.
pub const PAUSA_ACTIVA: f64 = 600.0;
/// Peso relativo de cada tipo de token (precios de Sonnet: entrada 1, escribir caché 1,25, leer
/// caché 0,1, salida 5).
pub const PESO_TOKEN: [f64; 4] = [1.0, 1.25, 0.1, 5.0];
/// Factores por familia de modelo (aproximados).
pub const PESO_MODELO: [(&str, f64); 2] = [("opus", 1.7), ("haiku", 0.33)];
/// Familias que se reconocen en el nombre del modelo; lo demás es `otro`.
const FAMILIAS: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];
/// $ por millón de tokens a precio de API: (entrada, salida, lectura de caché). Escribir en caché
/// cuesta 1,25x la entrada (5 min) o 2x (1 h). Con suscripción no se paga así: es una referencia.
pub const PRECIOS: [(&str, (f64, f64, f64)); 13] = [
    ("claude-fable-5-1", (10.0, 50.0, 0.25)),
    ("claude-fable-5", (10.0, 50.0, 1.0)),
    ("claude-mythos-5-1", (10.0, 50.0, 0.25)),
    ("claude-opus-5-5", (4.0, 20.0, 0.20)),
    ("claude-opus-5", (5.0, 25.0, 0.5)),
    ("claude-opus-4-8", (5.0, 25.0, 0.5)),
    ("claude-opus-4-7", (5.0, 25.0, 0.5)),
    ("claude-opus-4-6", (5.0, 25.0, 0.5)),
    ("claude-sonnet-5-5", (2.0, 10.0, 0.20)),
    ("claude-sonnet-5", (2.0, 10.0, 0.2)),
    ("claude-sonnet-4-6", (3.0, 15.0, 0.3)),
    ("claude-haiku-5-5", (0.1, 0.5, 0.01)),
    ("claude-haiku-4-5", (1.0, 5.0, 0.1)),
];
/// Precio de un modelo que no se conoce (como Opus 5.5).
const PRECIO_DEFECTO: (f64, f64, f64) = (4.0, 20.0, 0.20);

// --- Uso de tokens y bloques, tal como vienen en la línea ---

fn num<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(0.0))
}

/// Subdetalle de la escritura de caché.
#[derive(Deserialize, Default, Clone, Debug)]
pub struct CreacionCache {
    #[serde(default, deserialize_with = "num")]
    pub ephemeral_1h_input_tokens: f64,
}

/// El `usage` de un mensaje (lo que no se usa se ignora).
#[derive(Deserialize, Default, Clone, Debug)]
pub struct Uso {
    #[serde(default, deserialize_with = "num")]
    pub input_tokens: f64,
    #[serde(default, deserialize_with = "num")]
    pub cache_creation_input_tokens: f64,
    #[serde(default, deserialize_with = "num")]
    pub cache_read_input_tokens: f64,
    #[serde(default, deserialize_with = "num")]
    pub output_tokens: f64,
    #[serde(default)]
    pub cache_creation: Option<CreacionCache>,
}

const B_ESCRIBIR: u8 = 1;
const B_COMANDOS: u8 = 2;
const B_EXPLORAR: u8 = 4;
const B_WEB: u8 = 8;
const B_OTRA: u8 = 16;
const B_TEXTO: u8 = 32;
const B_PENSAR: u8 = 64;

/// Qué bloques tuvo una llamada, como bits (no se guardan los nombres de herramienta, solo su clase).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Bloques(pub u8);

impl fmt::Debug for Bloques {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bloques({:#09b})", self.0)
    }
}

fn bit_herramienta(nombre: &str) -> u8 {
    match nombre {
        "Edit" | "Write" | "NotebookEdit" | "MultiEdit" => B_ESCRIBIR,
        "Bash" | "PowerShell" => B_COMANDOS,
        "Read" | "Grep" | "Glob" | "LS" => B_EXPLORAR,
        "WebSearch" | "WebFetch" => B_WEB,
        _ => B_OTRA,
    }
}

impl Bloques {
    /// Desde los nombres de Consumo (`text`, `thinking`, `tool:<nombre>`).
    pub fn de_nombres(nombres: &[&str]) -> Bloques {
        let mut b = 0;
        for n in nombres {
            b |= match *n {
                "text" => B_TEXTO,
                "thinking" => B_PENSAR,
                _ => n.strip_prefix("tool:").map_or(0, bit_herramienta),
            };
        }
        Bloques(b)
    }

    /// Tipo de tarea de una llamada, de más a menos específico; "" si no se sabe.
    pub fn tarea(self) -> &'static str {
        let b = self.0;
        if b & B_ESCRIBIR != 0 {
            "codigo"
        } else if b & B_COMANDOS != 0 {
            "comandos"
        } else if b & B_EXPLORAR != 0 {
            "explorar"
        } else if b & B_WEB != 0 {
            "web"
        } else if b & B_OTRA != 0 {
            "herramientas"
        } else if b & B_TEXTO != 0 {
            "mensajes"
        } else if b & B_PENSAR != 0 {
            "pensar"
        } else {
            ""
        }
    }
}

/// Tipo de tarea a partir de los nombres de bloque de Consumo.
pub fn tarea_de(bloques: &[&str]) -> &'static str {
    Bloques::de_nombres(bloques).tarea()
}

/// Un elemento de `content`: solo importa su tipo (y el nombre si es una herramienta).
struct Elemento(u8);

struct VisElemento;

impl<'de> Visitor<'de> for VisElemento {
    type Value = Elemento;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("un bloque de contenido")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Elemento, A::Error> {
        let (mut tipo, mut nombre) = (None, None);
        while let Some(k) = m.next_key::<String>()? {
            match k.as_str() {
                "type" => tipo = m.next_value::<serde_json::Value>()?.as_str().map(String::from),
                "name" => nombre = m.next_value::<serde_json::Value>()?.as_str().map(String::from),
                _ => {
                    m.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Elemento(match tipo.as_deref() {
            Some("tool_use") => bit_herramienta(nombre.as_deref().unwrap_or("None")),
            Some("text") => B_TEXTO,
            Some("thinking") => B_PENSAR,
            _ => 0,
        }))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Elemento, A::Error> {
        while s.next_element::<IgnoredAny>()?.is_some() {}
        Ok(Elemento(0))
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Elemento, E> {
        Ok(Elemento(0))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Elemento, E> {
        Ok(Elemento(0))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Elemento, E> {
        Ok(Elemento(0))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Elemento, E> {
        Ok(Elemento(0))
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Elemento, E> {
        Ok(Elemento(0))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Elemento, E> {
        Ok(Elemento(0))
    }
}

impl<'de> Deserialize<'de> for Elemento {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(VisElemento)
    }
}

struct VisBloques;

impl<'de> Visitor<'de> for VisBloques {
    type Value = Bloques;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("el contenido de un mensaje")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Bloques, A::Error> {
        let mut b = 0;
        while let Some(e) = s.next_element::<Elemento>()? {
            b |= e.0;
        }
        Ok(Bloques(b))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Bloques, A::Error> {
        while m.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(Bloques(0))
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Bloques, E> {
        Ok(Bloques(0))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Bloques, E> {
        Ok(Bloques(0))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Bloques, E> {
        Ok(Bloques(0))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Bloques, E> {
        Ok(Bloques(0))
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Bloques, E> {
        Ok(Bloques(0))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Bloques, E> {
        Ok(Bloques(0))
    }
}

impl<'de> Deserialize<'de> for Bloques {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(VisBloques)
    }
}

/// `message` de una línea de transcripción.
#[derive(Deserialize)]
pub struct MensajeCrudo {
    pub id: Option<String>,
    pub model: Option<String>,
    pub usage: Option<Uso>,
    pub content: Option<Bloques>,
}

/// Una línea de transcripción (solo los campos que interesan; el resto no se copia).
#[derive(Deserialize)]
pub struct LineaCruda {
    #[serde(rename = "type")]
    pub tipo: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    pub uuid: Option<String>,
    pub message: Option<MensajeCrudo>,
}

// --- Coste, peso y caché de un mensaje ---

fn precio(modelo: &str) -> (f64, f64, f64) {
    PRECIOS
        .iter()
        .find(|(k, _)| modelo.starts_with(k))
        .map_or(PRECIO_DEFECTO, |(_, v)| *v)
}

/// $ que costaría un mensaje a precio de API (el modelo desconocido, como Opus 5.5).
pub fn coste_mensaje(modelo: &str, u: &Uso) -> f64 {
    let (ent, sal, leer) = precio(modelo);
    let hora = u.cache_creation.as_ref().map_or(0.0, |c| c.ephemeral_1h_input_tokens);
    let escribir = u.cache_creation_input_tokens - hora;
    (u.input_tokens * ent
        + u.output_tokens * sal
        + u.cache_read_input_tokens * leer
        + escribir * ent * 1.25
        + hora * ent * 2.0)
        / 1e6
}

/// (tokens leídos de la caché, $ que ahorra): lo que costaría leerlos como entrada normal menos
/// lo que cuesta leer la caché.
pub fn cache_mensaje(modelo: &str, u: &Uso) -> (f64, f64) {
    let (ent, _, leer) = precio(modelo);
    let n = u.cache_read_input_tokens;
    (n, n * (ent - leer) / 1e6)
}

/// Peso de un mensaje: tokens ponderados por tipo y por familia de modelo.
pub fn peso_mensaje(modelo: &str, u: &Uso) -> f64 {
    let p = u.input_tokens * PESO_TOKEN[0]
        + u.cache_creation_input_tokens * PESO_TOKEN[1]
        + u.cache_read_input_tokens * PESO_TOKEN[2]
        + u.output_tokens * PESO_TOKEN[3];
    let m = modelo.to_lowercase();
    p * PESO_MODELO.iter().find(|(n, _)| m.contains(n)).map_or(1.0, |(_, f)| *f)
}

/// Familia del modelo (la que aparece antes en el nombre) o `otro`.
fn familia(modelo: &str) -> &'static str {
    let m = modelo.to_lowercase();
    FAMILIAS
        .iter()
        .filter_map(|f| m.find(f).map(|i| (i, *f)))
        .min_by_key(|(i, _)| *i)
        .map_or("otro", |(_, f)| f)
}

/// Segundos de tiempo activo de cada mensaje de UNA sesión (instantes ordenados): el hueco con el
/// mensaje anterior si es menor que `pausa`; si no (una pausa), o si es el primero, 0.
pub fn segundos_activos(ts_ordenados: &[f64], pausa: f64) -> Vec<f64> {
    let mut ant: Option<f64> = None;
    ts_ordenados
        .iter()
        .map(|&t| {
            let s = match ant {
                Some(a) if (0.0..pausa).contains(&(t - a)) => t - a,
                _ => 0.0,
            };
            ant = Some(t);
            s
        })
        .collect()
}

/// Lo que se saca de una línea de transcripción del modelo.
#[derive(Debug)]
pub struct Leida {
    /// Id del mensaje (varias líneas con el mismo id son trozos del mismo mensaje).
    pub clave: String,
    pub t: f64,
    pub cwd: String,
    pub ses: String,
    pub modelo: &'static str,
    pub peso: f64,
    pub tokens: u64,
    pub bloques: Bloques,
    pub coste: f64,
    /// (tokens leídos de la caché, $ que ahorra).
    pub cache: (f64, f64),
}

/// Interpreta una línea de transcripción (un mensaje del modelo con `usage`), o None.
pub fn leer_linea(linea: &str) -> Option<Leida> {
    if !linea.contains("\"usage\"") || !linea.contains("\"assistant\"") {
        return None;
    }
    let d: LineaCruda = serde_json::from_str(linea).ok()?;
    let m = d.message?;
    let u = m.usage?;
    let modelo = m.model.unwrap_or_default();
    if d.tipo.as_deref() != Some("assistant") || modelo.starts_with('<') {
        return None; // <synthetic> no es del modelo
    }
    let t = DateTime::parse_from_rfc3339(&d.timestamp?).ok()?.timestamp_millis() as f64 / 1000.0;
    let clave = [m.id, d.request_id, d.uuid]
        .into_iter()
        .flatten()
        .find(|s| !s.is_empty())?;
    let tokens = u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens + u.output_tokens;
    Some(Leida {
        clave,
        t,
        cwd: d.cwd.unwrap_or_default(),
        ses: d.session_id.unwrap_or_default(),
        modelo: familia(&modelo),
        peso: peso_mensaje(&modelo, &u),
        tokens: tokens as u64,
        bloques: m.content.unwrap_or_default(),
        coste: coste_mensaje(&modelo, &u),
        cache: cache_mensaje(&modelo, &u),
    })
}

// --- El lector ---

fn ahora_s() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

fn mtime_s(m: &std::fs::Metadata) -> f64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Carpeta de usuario (para no contar como repo la que tiene `.git` en el home).
pub fn carpeta_home() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default()
}

/// Carpeta de las transcripciones de Claude Code.
pub fn raiz_proyectos() -> PathBuf {
    Path::new(&carpeta_home()).join(".claude").join("projects")
}

/// Un mensaje retenido: compacto y sin texto (cwd y sesión son índices en `Interner`).
#[derive(Clone, Copy)]
struct Mensaje {
    t: f64,
    peso: f64,
    coste: f64,
    ahorro: f64,
    tokens: u32,
    cache_leida: u32,
    cwd: u32,
    ses: u32,
    /// Índice en `MODELOS`.
    modelo: u8,
    bloques: Bloques,
}

const MODELOS: [&str; 5] = ["opus", "sonnet", "haiku", "fable", "otro"];

fn idx_modelo(f: &str) -> u8 {
    MODELOS.iter().position(|m| *m == f).unwrap_or(4) as u8
}

/// Cadenas repetidas (cwd, sesión) guardadas una sola vez.
#[derive(Default)]
struct Interner {
    v: Vec<String>,
    m: HashMap<String, u32>,
}

impl Interner {
    fn id(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.m.get(s) {
            return i;
        }
        let i = self.v.len() as u32;
        self.v.push(s.to_string());
        self.m.insert(s.to_string(), i);
        i
    }
    fn get(&self, i: u32) -> &str {
        &self.v[i as usize]
    }
}

/// Un fichero de transcripción encontrado.
struct Fichero {
    ruta: PathBuf,
    tam: u64,
    mtime: f64,
}

/// Clave de un cubo: (inicio del cubo, proyecto, sesión, modelo, tarea).
pub type Clave = (i64, String, String, &'static str, &'static str);

/// Cubo de 10 min: los mensajes sumados por proyecto, sesión, modelo y tarea.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Cubo {
    pub inicio: i64,
    pub proyecto: String,
    pub sesion: String,
    pub modelo: &'static str,
    pub peso: i64,
    pub tokens: u64,
    pub tarea: &'static str,
}

fn hash_id(id: &str) -> u64 {
    let mut h = DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

fn lista_jsonl(dir: &Path, fuera: &mut Vec<Fichero>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        if e.path().extension().is_some_and(|x| x == "jsonl") {
            if let Ok(m) = e.metadata() {
                if m.is_file() {
                    fuera.push(Fichero {
                        ruta: e.path(),
                        tam: m.len(),
                        mtime: mtime_s(&m),
                    });
                }
            }
        }
    }
}

/// Lee solo lo nuevo de cada transcripción (son ficheros que crecen por el final) y se queda con los
/// mensajes de los últimos `retener` s. Un mensaje sale varias veces (por trozos, o copiado al
/// reanudar una sesión): se cuenta una vez por su id.
pub struct Lector {
    raiz: PathBuf,
    extras: Vec<PathBuf>,
    retener: f64,
    home: String,
    es_repo: Box<dyn Fn(&str) -> bool + Send + Sync>,
    /// Posición leída de cada fichero.
    pos: HashMap<PathBuf, u64>,
    /// Mensajes retenidos por hash de su id.
    msgs: HashMap<u64, Mensaje>,
    cwds: Interner,
    sesiones: Interner,
    /// sessionId -> primer cwd de su transcripción principal.
    cwd_ses: HashMap<String, String>,
    repos: CacheRepos,
}

/// Clave interna de un cubo (proyecto como índice de `Claves`).
type ClaveInt = (i64, u32, u32, u8, &'static str);

/// Proyectos resueltos durante un recorrido: (sesión, cwd) -> proyecto.
#[derive(Default)]
struct Claves {
    memo: HashMap<(u32, u32), u32>,
    nombres: Vec<String>,
    idx: HashMap<String, u32>,
}

impl Lector {
    /// Lector de la carpeta de transcripciones `raiz`, con la retención de 35 días y sin extras.
    pub fn new(raiz: PathBuf) -> Lector {
        Lector {
            raiz,
            extras: Vec::new(),
            retener: RETENER,
            home: carpeta_home(),
            es_repo: Box::new(tiene_git),
            pos: HashMap::new(),
            msgs: HashMap::new(),
            cwds: Interner::default(),
            sesiones: Interner::default(),
            cwd_ses: HashMap::new(),
            repos: HashMap::new(),
        }
    }

    /// Otras carpetas con copias de transcripciones (histórico largo); hoy no se usa.
    pub fn con_extras(mut self, extras: Vec<PathBuf>) -> Lector {
        self.extras = extras.into_iter().filter(|e| !e.as_os_str().is_empty()).collect();
        self
    }

    pub fn con_retener(mut self, s: f64) -> Lector {
        self.retener = s;
        self
    }

    pub fn con_home(mut self, home: &str) -> Lector {
        self.home = home.to_string();
        self
    }

    /// Sustituye la búsqueda de `.git` (para pruebas).
    pub fn con_es_repo(mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Lector {
        self.es_repo = Box::new(f);
        self
    }

    /// Mensajes retenidos.
    pub fn num_mensajes(&self) -> usize {
        self.msgs.len()
    }

    /// Transcripciones encontradas ahora en disco (para la prueba de humo).
    #[allow(dead_code)]
    pub fn contar_ficheros(&self) -> usize {
        self.ficheros().len()
    }

    /// Primer cwd de la transcripción principal de una sesión.
    pub fn cwd_sesion(&self, ses: &str) -> Option<&str> {
        self.cwd_ses.get(ses).map(String::as_str)
    }

    /// Transcripciones de la carpeta de Claude y de las copias extra (`*/*.jsonl` y
    /// `*/*/subagents/*.jsonl`); la misma ruta relativa en dos sitios sale una vez: la más grande.
    fn ficheros(&self) -> Vec<Fichero> {
        let mut mejores: HashMap<String, Fichero> = HashMap::new();
        let raices = std::iter::once(&self.raiz).chain(self.extras.iter());
        for raiz in raices {
            let mut hallados = Vec::new();
            let Ok(proyectos) = std::fs::read_dir(raiz) else {
                continue;
            };
            for p in proyectos.flatten() {
                let Ok(tipo) = p.file_type() else { continue };
                if !tipo.is_dir() {
                    continue;
                }
                let Ok(dentro) = std::fs::read_dir(p.path()) else {
                    continue;
                };
                for e in dentro.flatten() {
                    let ruta = e.path();
                    let Ok(tipo) = e.file_type() else { continue };
                    if tipo.is_dir() {
                        lista_jsonl(&ruta.join("subagents"), &mut hallados);
                    } else if ruta.extension().is_some_and(|x| x == "jsonl") {
                        if let Ok(m) = e.metadata() {
                            hallados.push(Fichero {
                                ruta,
                                tam: m.len(),
                                mtime: mtime_s(&m),
                            });
                        }
                    }
                }
            }
            for f in hallados {
                let rel = f.ruta.strip_prefix(raiz).unwrap_or(&f.ruta);
                let k = rel.to_string_lossy().to_lowercase();
                if mejores.get(&k).is_none_or(|m| f.tam > m.tam) {
                    mejores.insert(k, f);
                }
            }
        }
        mejores.into_values().collect()
    }

    /// Lee lo nuevo de las transcripciones. `ahora` en s desde la época (None: la hora actual).
    pub fn actualizar(&mut self, ahora: Option<f64>) {
        let ahora = ahora.unwrap_or_else(ahora_s);
        let desde = ahora - self.retener;
        let ficheros = self.ficheros();
        let presentes: HashSet<&Path> = ficheros.iter().map(|f| f.ruta.as_path()).collect();
        self.pos.retain(|r, _| presentes.contains(r.as_path()));
        for f in &ficheros {
            let pos0 = self.pos.get(&f.ruta).copied().unwrap_or(0);
            let pos = if f.tam >= pos0 { pos0 } else { 0 }; // si el fichero encoge, otra vez desde el principio
            if f.mtime < desde || f.tam == pos {
                continue;
            }
            self.leer_fichero(&f.ruta, pos, desde);
        }
        self.msgs.retain(|_, m| m.t >= desde);
    }

    /// Lee las líneas completas nuevas de un fichero (la última puede estar escribiéndose).
    fn leer_fichero(&mut self, ruta: &Path, pos: u64, desde: f64) {
        let Ok(mut fh) = File::open(ruta) else { return };
        if fh.seek(SeekFrom::Start(pos)).is_err() {
            return;
        }
        let ses = ruta
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let principal = ruta
            .parent()
            .and_then(|p| p.file_name())
            .is_none_or(|n| n != "subagents");
        let mut lector = BufReader::with_capacity(1 << 18, fh);
        let mut buf = Vec::new();
        let mut avance = 0u64;
        loop {
            buf.clear();
            match lector.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf.last() != Some(&b'\n') {
                        break; // línea a medias: se lee la próxima vez
                    }
                    avance += n as u64;
                }
            }
            let texto = String::from_utf8_lossy(&buf);
            if principal && !self.cwd_ses.contains_key(&ses) && texto.contains("\"cwd\"") {
                if let Some(c) = buscar_cwd(&texto) {
                    self.cwd_ses.insert(ses.clone(), c);
                }
            }
            if let Some(l) = leer_linea(&texto) {
                if l.t >= desde {
                    self.guardar(l);
                }
            }
        }
        if avance > 0 {
            self.pos.insert(ruta.to_path_buf(), pos + avance);
        }
    }

    /// Guarda un mensaje (el último gana; los bloques de todas sus líneas se juntan).
    fn guardar(&mut self, l: Leida) {
        let nuevo = Mensaje {
            t: l.t,
            peso: l.peso,
            coste: l.coste,
            ahorro: l.cache.1,
            tokens: l.tokens.min(u32::MAX as u64) as u32,
            cache_leida: l.cache.0.min(u32::MAX as f64) as u32,
            cwd: self.cwds.id(&l.cwd),
            ses: self.sesiones.id(&l.ses),
            modelo: idx_modelo(l.modelo),
            bloques: l.bloques,
        };
        let k = hash_id(&l.clave);
        match self.msgs.get_mut(&k) {
            Some(m) => {
                let b = m.bloques.0 | nuevo.bloques.0;
                *m = nuevo;
                m.bloques = Bloques(b);
            }
            None => {
                self.msgs.insert(k, nuevo);
            }
        }
    }

    /// Proyecto con el que cuenta un mensaje (`proyecto_repo`). Se resuelve al sacar los datos, no
    /// al leer: el orden de los ficheros no importa (un subagente puede leerse antes que su principal).
    pub fn proyecto(&mut self, ses: &str, cwd: &str) -> String {
        let cwd_ses = if ses.is_empty() {
            ""
        } else {
            self.cwd_ses.get(ses).map_or("", String::as_str)
        };
        proyecto_repo(cwd, cwd_ses, &*self.es_repo, &mut self.repos, &self.home)
    }

    fn clave_int(&mut self, m: &Mensaje, c: &mut Claves) -> ClaveInt {
        let p = match c.memo.get(&(m.ses, m.cwd)) {
            Some(&p) => p,
            None => {
                let (ses, cwd) = (self.sesiones.get(m.ses).to_string(), self.cwds.get(m.cwd).to_string());
                let nombre = self.proyecto(&ses, &cwd);
                let n = c.nombres.len() as u32;
                let p = *c.idx.entry(nombre.clone()).or_insert(n);
                if p == n {
                    c.nombres.push(nombre);
                }
                c.memo.insert((m.ses, m.cwd), p);
                p
            }
        };
        ((m.t / CUBO as f64).floor() as i64 * CUBO, p, m.ses, m.modelo, m.bloques.tarea())
    }

    fn externa(&self, k: ClaveInt, c: &Claves) -> Clave {
        (
            k.0,
            c.nombres[k.1 as usize].clone(),
            self.sesiones.get(k.2).to_string(),
            MODELOS[k.3 as usize],
            k.4,
        )
    }

    /// Suma `valor` de cada mensaje en su cubo. Los mensajes se sacan un momento del lector para
    /// poder resolver proyectos (que usa la caché de repos) sin copiarlos.
    fn agregar<T: Default>(&mut self, mut sumar: impl FnMut(&mut T, &Mensaje)) -> HashMap<Clave, T> {
        let msgs = std::mem::take(&mut self.msgs);
        let mut claves = Claves::default();
        let mut acc: HashMap<ClaveInt, T> = HashMap::new();
        for m in msgs.values() {
            let k = self.clave_int(m, &mut claves);
            sumar(acc.entry(k).or_default(), m);
        }
        self.msgs = msgs;
        acc.into_iter().map(|(k, v)| (self.externa(k, &claves), v)).collect()
    }

    /// $ a precio de API de cada cubo (se queda en este equipo).
    pub fn costes(&mut self) -> HashMap<Clave, f64> {
        self.agregar(|a: &mut f64, m| *a += m.coste)
    }

    /// (tokens leídos de la caché, $ que ahorra) de cada cubo.
    pub fn cache(&mut self) -> HashMap<Clave, (f64, f64)> {
        self.agregar(|a: &mut (f64, f64), m| {
            a.0 += m.cache_leida as f64;
            a.1 += m.ahorro;
        })
    }

    /// Segundos activos de cada cubo: dentro de cada sesión (subagentes incluidos) cada mensaje suma
    /// el hueco con el anterior si es menor que `PAUSA_ACTIVA`.
    pub fn tiempos(&mut self) -> HashMap<Clave, f64> {
        let msgs = std::mem::take(&mut self.msgs);
        let mut por_ses: HashMap<u32, Vec<&Mensaje>> = HashMap::new();
        for m in msgs.values() {
            por_ses.entry(m.ses).or_default().push(m);
        }
        let mut claves = Claves::default();
        let mut acc: HashMap<ClaveInt, f64> = HashMap::new();
        for lista in por_ses.values_mut() {
            lista.sort_by(|a, b| a.t.total_cmp(&b.t));
            let ts: Vec<f64> = lista.iter().map(|m| m.t).collect();
            for (m, seg) in lista.iter().zip(segundos_activos(&ts, PAUSA_ACTIVA)) {
                if seg > 0.0 {
                    let k = self.clave_int(m, &mut claves);
                    *acc.entry(k).or_default() += seg;
                }
            }
        }
        drop(por_ses);
        self.msgs = msgs;
        acc.into_iter().map(|(k, v)| (self.externa(k, &claves), v)).collect()
    }

    /// Mensajes sumados por cada 10 min, proyecto, sesión, modelo y tarea, ordenados.
    pub fn cubos(&mut self) -> Vec<Cubo> {
        let acc = self.agregar(|a: &mut (f64, u64), m| {
            a.0 += m.peso;
            a.1 += m.tokens as u64;
        });
        let mut v: Vec<Cubo> = acc
            .into_iter()
            .map(|(k, (peso, tokens))| Cubo {
                inicio: k.0,
                proyecto: k.1,
                sesion: k.2,
                modelo: k.3,
                peso: peso.round_ties_even() as i64,
                tokens,
                tarea: k.4,
            })
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v
    }
}
