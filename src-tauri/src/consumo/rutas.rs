//! Carpetas de trabajo (cwd) de las transcripciones: nombre de proyecto, repositorio git y lectura
//! del primer/último cwd de un fichero. Las rutas son de Windows (`C:\x\Repo`) pero también se
//! aceptan con `/`; las funciones `nt_*` imitan a `ntpath` de Python sin tocar el disco.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Proyecto de lo que no tiene carpeta (o es una carpeta temporal).
pub const SIN_CARPETA: &str = "(sin carpeta)";

/// Caché de `repo_de`: cwd -> nombre del repositorio (o None). Se mira el disco una vez por carpeta.
pub type CacheRepos = HashMap<String, Option<String>>;

fn es_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// Componentes no vacíos de una ruta (separa por `\` y `/`).
fn componentes(ruta: &str) -> impl Iterator<Item = &str> {
    ruta.split(es_sep).filter(|c| !c.is_empty())
}

/// Nombre del repositorio si la ruta es un worktree `<Repo>/.claude/worktrees/<x>`.
fn de_worktree(cwd: &str) -> Option<&str> {
    let c: Vec<&str> = componentes(cwd).collect();
    (1..c.len().saturating_sub(1))
        .find(|&i| c[i] == ".claude" && c[i + 1] == "worktrees")
        .map(|i| c[i - 1])
}

/// Nombre de la última carpeta del cwd; las carpetas temporales, sin carpeta; un worktree cuenta
/// como su repositorio.
pub fn proyecto_de(cwd: &str) -> String {
    if cwd.is_empty() || cwd.contains("scratch-workspaces") {
        return SIN_CARPETA.into();
    }
    if let Some(r) = de_worktree(cwd) {
        return r.into();
    }
    let ultimo = cwd.trim_end_matches(es_sep).rsplit(es_sep).next().unwrap_or("");
    if ultimo.is_empty() {
        SIN_CARPETA.into()
    } else {
        ultimo.into()
    }
}

/// Como `ntpath.normpath`: separadores `\`, sin `.` ni `..` ni barras repetidas.
pub fn nt_normpath(ruta: &str) -> String {
    let s = ruta.replace('/', "\\");
    let (unidad, resto) = if s.as_bytes().get(1) == Some(&b':') {
        s.split_at(2)
    } else {
        ("", s.as_str())
    };
    let absoluta = resto.starts_with('\\');
    let mut partes: Vec<&str> = Vec::new();
    for c in resto.split('\\') {
        match c {
            "" | "." => {}
            ".." => {
                if partes.last().is_some_and(|l| *l != "..") {
                    partes.pop();
                } else if !absoluta {
                    partes.push("..");
                }
            }
            _ => partes.push(c),
        }
    }
    let mut out = String::from(unidad);
    if absoluta {
        out.push('\\');
    }
    out.push_str(&partes.join("\\"));
    if out.is_empty() {
        ".".into()
    } else {
        out
    }
}

/// Como `ntpath.dirname` (`C:\x` -> `C:\`; la raíz es su propio padre).
pub fn nt_dirname(ruta: &str) -> String {
    let unidad = if ruta.as_bytes().get(1) == Some(&b':') { 2 } else { 0 };
    let i = ruta.rfind('\\').map_or(unidad, |i| i + 1);
    let cabeza = &ruta[..i];
    if cabeza[unidad..].chars().all(|c| c == '\\') {
        cabeza.into()
    } else {
        cabeza.trim_end_matches('\\').into()
    }
}

/// Como `ntpath.basename`.
pub fn nt_basename(ruta: &str) -> &str {
    let unidad = if ruta.as_bytes().get(1) == Some(&b':') { 2 } else { 0 };
    let i = ruta.rfind(['\\', '/']).map_or(unidad, |i| i + 1);
    &ruta[i..]
}

/// Como `ntpath.normcase`.
pub fn nt_normcase(ruta: &str) -> String {
    ruta.replace('/', "\\").to_lowercase()
}

/// La carpeta tiene `.git` (carpeta, o fichero en un worktree).
pub fn tiene_git(carpeta: &str) -> bool {
    Path::new(carpeta).join(".git").exists()
}

/// Nombre del repositorio git que contiene el cwd (sube hasta una carpeta con `.git`, sin pasar de
/// la raíz de la unidad), o None. Un worktree `<Repo>/.claude/worktrees/<x>` es `<Repo>` (se mira
/// antes que `.git`); el repo cuya raíz es la carpeta de usuario y las carpetas temporales no
/// cuentan. `es_repo` es inyectable; `cache` evita mirar el disco otra vez por el mismo cwd.
pub fn repo_de(
    cwd: &str,
    es_repo: &dyn Fn(&str) -> bool,
    cache: &mut CacheRepos,
    home: &str,
) -> Option<String> {
    if cwd.is_empty() || cwd.contains("scratch-workspaces") {
        return None;
    }
    if let Some(r) = cache.get(cwd) {
        return r.clone();
    }
    let res = match de_worktree(cwd) {
        Some(w) => Some(w.to_string()),
        None => {
            let mut carpeta = nt_normpath(cwd);
            let casa = nt_normcase(&nt_normpath(if home.is_empty() { "?" } else { home }));
            loop {
                let es_casa = nt_normcase(&carpeta) == casa;
                if !es_casa && es_repo(&carpeta) {
                    let b = nt_basename(&carpeta);
                    break (!b.is_empty()).then(|| b.to_string());
                }
                let padre = nt_dirname(&carpeta);
                if padre.is_empty() || padre == carpeta || es_casa {
                    break None;
                }
                carpeta = padre;
            }
        }
    };
    cache.insert(cwd.to_string(), res.clone());
    res
}

/// Proyecto de un mensaje: el repo git de su cwd; si no está en ningún repo, el repo del primer
/// cwd de su sesión; si tampoco, la última carpeta del cwd.
pub fn proyecto_repo(
    cwd: &str,
    cwd_ses: &str,
    es_repo: &dyn Fn(&str) -> bool,
    cache: &mut CacheRepos,
    home: &str,
) -> String {
    repo_de(cwd, es_repo, cache, home)
        .or_else(|| repo_de(cwd_ses, es_repo, cache, home))
        .unwrap_or_else(|| proyecto_de(cwd))
}

/// Primer `"cwd": "..."` de un texto (el valor ya decodificado), como `CWD_RE` de Consumo. Si
/// el primero que encaja no es un JSON válido, None.
pub fn buscar_cwd(texto: &str) -> Option<String> {
    const CLAVE: &str = "\"cwd\":";
    let mut desde = 0;
    while let Some(i) = texto[desde..].find(CLAVE) {
        let inicio = desde + i;
        if let Some(cuerpo) = texto[inicio + CLAVE.len()..].trim_start().strip_prefix('"') {
            let b = cuerpo.as_bytes();
            let mut j = 0;
            let mut cierre = None;
            while j < b.len() {
                match b[j] {
                    b'\\' if j + 1 < b.len() && b[j + 1] != b'\n' => j += 2,
                    b'\\' => break,
                    b'"' => {
                        cierre = Some(j);
                        break;
                    }
                    _ => j += 1,
                }
            }
            if let Some(j) = cierre {
                return serde_json::from_str::<String>(&format!("\"{}\"", &cuerpo[..j])).ok();
            }
        }
        desde = inicio + 1;
    }
    None
}

/// Lee como mucho `n` bytes del principio (como texto; lo ilegible se sustituye).
fn leer_cabeza(ruta: &Path, n: u64) -> Option<String> {
    let mut buf = Vec::new();
    File::open(ruta).ok()?.take(n).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Lee los últimos `n` bytes de un fichero (como texto). None si no se puede abrir.
pub fn leer_cola(ruta: &Path, n: u64) -> Option<String> {
    let mut f = File::open(ruta).ok()?;
    let tam = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(tam.saturating_sub(n))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Primera carpeta de trabajo de una transcripción, mirando solo el principio; "" si no hay.
pub fn primer_cwd(ruta: &Path, cabeza: u64) -> String {
    leer_cabeza(ruta, cabeza)
        .and_then(|t| buscar_cwd(&t))
        .unwrap_or_default()
}

/// Última carpeta de trabajo de una transcripción, mirando solo el final (una sesión puede
/// cambiar de carpeta); "" si no hay.
pub fn cwd_de(ruta: &Path, cola: u64) -> String {
    #[derive(serde::Deserialize)]
    struct Solo {
        cwd: Option<String>,
    }
    let Some(texto) = leer_cola(ruta, cola) else {
        return String::new();
    };
    for linea in texto.lines().rev() {
        if !linea.contains("\"cwd\"") {
            continue;
        }
        if let Ok(s) = serde_json::from_str::<Solo>(linea) {
            return s.cwd.unwrap_or_default(); // si falla, la línea está cortada por el principio
        }
    }
    String::new()
}
