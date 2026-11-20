//! Contexto por sesión de Claude Code: cuántos tokens lleva cada sesión viva (la entrada del último
//! mensaje del modelo), con casillas estables, su proyecto y el chat de la app de Claude.
//! Portado de `consumo.pyw` (contexto_de, sesiones_vivas, chats_escritorio, asignar_casillas).
//!
//! Gasto mínimo: solo se lee la cola de cada transcripción y solo si cambió (tamaño y fecha); la
//! lista de chats de la app de Claude se rehace como mucho cada `REFRESCO_CHATS`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::mtime_s;
use super::rutas::{cwd_de, leer_cola, primer_cwd, proyecto_repo, tiene_git, CacheRepos};
use super::transcripciones::{carpeta_home, raiz_proyectos, LineaCruda};

/// Ventana de contexto por defecto (tokens). El ajuste del usuario vendrá más adelante.
pub const CTX_MAX: u64 = 1_000_000;
/// Casillas (sesiones que se enseñan a la vez).
pub const CTX_N: usize = 10;
/// s sin actividad para que una sesión deje de estar viva.
pub const CTX_VIVA: f64 = 1800.0;
/// Cola de la transcripción que se mira para el contexto.
const COLA_CONTEXTO: u64 = 400_000;
/// Cola y cabeza que se miran para sacar el cwd de la sesión.
const COLA_CWD: u64 = 300_000;
/// Cada cuánto como mucho se vuelve a listar la carpeta de chats de la app de Claude.
const REFRESCO_CHATS: Duration = Duration::from_secs(15);

/// Lo lleno que está el contexto: bien (< 50 %), medio (< 80 %) o alto.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NivelContexto {
    Bien,
    Medio,
    Alto,
}

/// Nivel según la fracción llena del contexto (0-1); umbrales 50 % y 80 %.
pub fn nivel_ctx(f: f64) -> NivelContexto {
    let f = f.clamp(0.0, 1.0);
    if f < 0.5 {
        NivelContexto::Bien
    } else if f < 0.8 {
        NivelContexto::Medio
    } else {
        NivelContexto::Alto
    }
}

/// Una sesión viva con su contexto (lo que ve la interfaz).
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SesionContexto {
    /// Id de la sesión (nombre del .jsonl).
    pub id: String,
    pub proyecto: String,
    pub tokens: u64,
    /// Tokens sobre la ventana máxima, de 0 a 1.
    pub fraccion: f64,
    pub nivel: NivelContexto,
    /// Título del chat en la app de Claude; si no hay, el proyecto.
    pub nombre: String,
    /// La sesión tiene chat en la app de Claude (se puede abrir).
    pub tiene_chat: bool,
}

/// Tokens que ocupa ahora el contexto de una sesión: la entrada (con caché) del último mensaje
/// del modelo, mirando solo los últimos `cola` bytes. 0 si no hay.
pub fn contexto_de(ruta: &Path, cola: u64) -> u64 {
    let Some(texto) = leer_cola(ruta, cola) else {
        return 0;
    };
    for linea in texto.lines().rev() {
        if !linea.contains("\"usage\"") {
            continue;
        }
        let Ok(l) = serde_json::from_str::<LineaCruda>(linea) else {
            continue; // línea cortada por el principio del trozo
        };
        let Some(m) = l.message else { continue };
        let Some(u) = m.usage else { continue };
        if m.model.unwrap_or_default().starts_with('<') {
            continue; // <synthetic> no es del modelo
        }
        return (u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens) as u64;
    }
    0
}

/// [(id, ruta)] de las sesiones con actividad en los últimos `viva` s, la más reciente primero.
pub fn sesiones_vivas(ahora: f64, raiz: &Path, viva: f64) -> Vec<(String, PathBuf)> {
    let mut filas: Vec<(f64, String, PathBuf)> = Vec::new();
    let Ok(proyectos) = std::fs::read_dir(raiz) else {
        return Vec::new();
    };
    for d in proyectos.flatten() {
        if !d.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(dentro) = std::fs::read_dir(d.path()) else {
            continue;
        };
        for f in dentro.flatten() {
            let ruta = f.path();
            if ruta.extension().is_none_or(|x| x != "jsonl") {
                continue;
            }
            let Ok(m) = f.metadata() else { continue };
            let t = mtime_s(&m);
            if ahora - t < viva {
                let id = ruta.file_stem().map(|s| s.to_string_lossy().into_owned());
                if let Some(id) = id {
                    filas.push((t, id, ruta));
                }
            }
        }
    }
    filas.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    filas.into_iter().map(|(_, i, r)| (i, r)).collect()
}

/// Cada sesión conserva su casilla mientras viva; la nueva coge la primera libre (si hay).
pub fn asignar_casillas(casillas: &[Option<String>], vivas: &[String]) -> Vec<Option<String>> {
    let mut c: Vec<Option<String>> = casillas
        .iter()
        .map(|x| x.clone().filter(|i| vivas.contains(i)))
        .collect();
    for i in vivas {
        if !c.iter().any(|x| x.as_ref() == Some(i)) {
            if let Some(libre) = c.iter_mut().find(|x| x.is_none()) {
                *libre = Some(i.clone());
            }
        }
    }
    c
}

/// Chat de la app de Claude de una sesión.
#[derive(Clone, Debug, PartialEq)]
pub struct Chat {
    /// Id del chat (nombre del fichero `local_*`), para `claude://claude.ai/epitaxy/<id>`.
    pub id: String,
    pub archivado: bool,
    pub titulo: String,
}

#[derive(Deserialize)]
struct ChatCrudo {
    #[serde(rename = "cliSessionId")]
    cli_session_id: Option<String>,
    #[serde(rename = "isArchived")]
    archivado: Option<bool>,
    title: Option<String>,
}

/// Dónde guarda la app de Claude sus chats (`Claude-Data` local y `Claude` en roaming).
pub fn bases_escritorio() -> Vec<PathBuf> {
    let var = |n: &str| std::env::var_os(n).filter(|v| !v.is_empty()).map(PathBuf::from);
    [
        var("LOCALAPPDATA").map(|p| p.join("Claude-Data")),
        var("APPDATA").map(|p| p.join("Claude")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

/// Ficheros `local_*.json` de `base/claude-code-sessions/*/*/` y `base/*/claude-code-sessions/*/*/`.
fn ficheros_chats(base: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let mut raices = vec![base.join("claude-code-sessions")];
    raices.extend(subdirs(base).into_iter().map(|d| d.join("claude-code-sessions")));
    let mut out = Vec::new();
    for r in raices {
        for a in subdirs(&r) {
            for b in subdirs(&a) {
                let Ok(rd) = std::fs::read_dir(&b) else { continue };
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().into_owned();
                    if !(n.starts_with("local_") && n.ends_with(".json")) {
                        continue;
                    }
                    if let Ok(m) = e.metadata() {
                        out.push((e.path(), m.len(), m.modified().unwrap_or(UNIX_EPOCH)));
                    }
                }
            }
        }
    }
    out
}

/// {id de sesión -> chat}. Solo se relee el JSON de los ficheros que cambiaron y la carpeta se
/// vuelve a listar como mucho cada `refresco`.
pub struct Chats {
    bases: Vec<PathBuf>,
    refresco: Duration,
    ultima: Option<Instant>,
    /// Por fichero: (tamaño, fecha) y lo que dijo.
    ficheros: HashMap<PathBuf, ((u64, SystemTime), Option<(String, Chat)>)>,
    mapa: HashMap<String, Chat>,
}

impl Chats {
    pub fn new(bases: Vec<PathBuf>, refresco: Duration) -> Chats {
        Chats {
            bases,
            refresco,
            ultima: None,
            ficheros: HashMap::new(),
            mapa: HashMap::new(),
        }
    }

    pub fn refrescar(&mut self) {
        if self.ultima.is_some_and(|u| u.elapsed() < self.refresco) {
            return;
        }
        self.ultima = Some(Instant::now());
        let mut nuevos = HashMap::new();
        for base in &self.bases {
            for (ruta, tam, mt) in ficheros_chats(base) {
                let firma = (tam, mt);
                let dato = match self.ficheros.remove(&ruta) {
                    Some((f, d)) if f == firma => d,
                    _ => leer_chat(&ruta),
                };
                nuevos.insert(ruta, (firma, dato));
            }
        }
        self.ficheros = nuevos;
        self.mapa = self
            .ficheros
            .values()
            .filter_map(|(_, d)| d.clone())
            .collect();
    }

    pub fn de(&self, sesion: &str) -> Option<&Chat> {
        self.mapa.get(sesion)
    }
}

fn leer_chat(ruta: &Path) -> Option<(String, Chat)> {
    let c: ChatCrudo = serde_json::from_slice(&std::fs::read(ruta).ok()?).ok()?;
    let sesion = c.cli_session_id.filter(|s| !s.is_empty())?;
    let id = ruta.file_stem()?.to_string_lossy().into_owned();
    Some((
        sesion,
        Chat {
            id,
            archivado: c.archivado.unwrap_or(false),
            titulo: c.title.unwrap_or_default(),
        },
    ))
}

/// Calcula el contexto de las sesiones vivas con casillas estables y cachés.
pub struct Contexto {
    raiz: PathBuf,
    home: String,
    es_repo: Box<dyn Fn(&str) -> bool + Send + Sync>,
    chats: Chats,
    casillas: Vec<Option<String>>,
    /// Por transcripción: (tamaño, fecha) y los tokens que se sacaron.
    tokens: HashMap<PathBuf, ((u64, SystemTime), u64)>,
    /// Proyecto de cada sesión con casilla (se calcula una vez).
    proyectos: HashMap<String, String>,
    repos: CacheRepos,
}

impl Contexto {
    pub fn new(raiz: PathBuf) -> Contexto {
        Contexto {
            raiz,
            home: carpeta_home(),
            es_repo: Box::new(tiene_git),
            chats: Chats::new(bases_escritorio(), REFRESCO_CHATS),
            casillas: vec![None; CTX_N],
            tokens: HashMap::new(),
            proyectos: HashMap::new(),
            repos: HashMap::new(),
        }
    }

    /// El contexto de las sesiones de la carpeta de transcripciones de Claude.
    pub fn real() -> Contexto {
        Contexto::new(raiz_proyectos())
    }

    #[cfg(test)]
    pub fn con_chats(mut self, chats: Chats) -> Contexto {
        self.chats = chats;
        self
    }

    #[cfg(test)]
    pub fn con_home(mut self, home: &str) -> Contexto {
        self.home = home.to_string();
        self
    }

    #[cfg(test)]
    pub fn con_es_repo(mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Contexto {
        self.es_repo = Box::new(f);
        self
    }

    /// Id del chat de la app de Claude de una sesión (para abrirlo).
    pub fn id_chat(&self, sesion: &str) -> Option<String> {
        self.chats.de(sesion).map(|c| c.id.clone())
    }

    /// Sesiones vivas no archivadas, en sus casillas estables, con su contexto sobre `ctx_max`.
    pub fn calcular(&mut self, ahora: f64, ctx_max: u64) -> Vec<SesionContexto> {
        self.chats.refrescar();
        let vivas: Vec<(String, PathBuf)> = sesiones_vivas(ahora, &self.raiz, CTX_VIVA)
            .into_iter()
            .filter(|(i, _)| !self.chats.de(i).is_some_and(|c| c.archivado))
            .collect();
        let ids: Vec<String> = vivas.iter().map(|(i, _)| i.clone()).collect();
        self.casillas = asignar_casillas(&self.casillas, &ids);
        let rutas: HashMap<&str, &PathBuf> = vivas.iter().map(|(i, r)| (i.as_str(), r)).collect();

        let mut salida = Vec::new();
        let mut en_uso: HashSet<PathBuf> = HashSet::new();
        for id in self.casillas.clone().into_iter().flatten() {
            let Some(&ruta) = rutas.get(id.as_str()) else { continue };
            en_uso.insert(ruta.clone());
            let tokens = self.tokens_de(ruta);
            let proyecto = self.proyecto_de_sesion(&id, ruta);
            let fraccion = if ctx_max == 0 {
                0.0
            } else {
                (tokens as f64 / ctx_max as f64).min(1.0)
            };
            let chat = self.chats.de(&id);
            let nombre = chat
                .map(|c| c.titulo.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| proyecto.clone());
            salida.push(SesionContexto {
                tiene_chat: chat.is_some(),
                nivel: nivel_ctx(fraccion),
                id,
                proyecto,
                tokens,
                fraccion,
                nombre,
            });
        }
        self.tokens.retain(|r, _| en_uso.contains(r));
        let activas: HashSet<&String> = self.casillas.iter().flatten().collect();
        self.proyectos.retain(|i, _| activas.contains(i));
        salida
    }

    /// Tokens de contexto de una transcripción; no se relee si no cambió su tamaño ni su fecha.
    fn tokens_de(&mut self, ruta: &Path) -> u64 {
        let Ok(m) = std::fs::metadata(ruta) else { return 0 };
        let firma = (m.len(), m.modified().unwrap_or(UNIX_EPOCH));
        if let Some((f, t)) = self.tokens.get(ruta) {
            if *f == firma {
                return *t;
            }
        }
        let t = contexto_de(ruta, COLA_CONTEXTO);
        self.tokens.insert(ruta.to_path_buf(), (firma, t));
        t
    }

    /// Proyecto de una sesión: el repo del último cwd; si no hay, el del primero.
    fn proyecto_de_sesion(&mut self, id: &str, ruta: &Path) -> String {
        if let Some(p) = self.proyectos.get(id) {
            return p.clone();
        }
        let p = proyecto_repo(
            &cwd_de(ruta, COLA_CWD),
            &primer_cwd(ruta, COLA_CWD),
            &*self.es_repo,
            &mut self.repos,
            &self.home,
        );
        self.proyectos.insert(id.to_string(), p.clone());
        p
    }
}
