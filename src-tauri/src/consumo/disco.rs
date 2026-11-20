//! Espacio en disco de los datos de Consumo: lo que ocupan las transcripciones y las carpetas
//! `historial` de sincronización, el espacio libre y el texto/nivel del indicador. Port de
//! `tam_carpeta`, `medir_datos`, `espacio_libre` y `texto_datos` de Consumo.
//!
//! Recorrer carpetas cuesta: quien llame mide como mucho cada 30 min.

use std::path::{Path, PathBuf};

use serde::Serialize;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Bytes que ocupa una carpeta con todo lo de dentro (0 si no existe). No sigue enlaces a carpetas.
pub fn tam_carpeta(ruta: &Path) -> u64 {
    let mut total = 0u64;
    let mut pila = vec![ruta.to_path_buf()];
    while let Some(dir) = pila.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(t) = e.file_type() else {
                continue;
            };
            if t.is_dir() {
                pila.push(e.path());
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

/// Las carpetas `historial` dentro de cada carpeta de sincronización activa, o `<local>/historial`
/// si no hay ninguna (`carpetas_historial(local=False)` de Consumo).
pub fn carpetas_historial(sync_activas: &[PathBuf], local: &Path) -> Vec<PathBuf> {
    if sync_activas.is_empty() {
        vec![local.join("historial")]
    } else {
        sync_activas.iter().map(|c| c.join("historial")).collect()
    }
}

/// La carpeta Descargas de Windows (`FOLDERID_Downloads`: puede estar redirigida a OneDrive); si no
/// se encuentra, `~/Downloads`.
pub fn carpeta_descargas(home: &Path) -> PathBuf {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    // SAFETY: SHGetKnownFolderPath devuelve una cadena propia que se libera con CoTaskMemFree.
    let ruta = unsafe {
        SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok().and_then(|p| {
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            s
        })
    };
    match ruta {
        Some(r) if Path::new(&r).is_dir() => PathBuf::from(r),
        _ => home.join("Downloads"),
    }
}

/// GB que ocupan los datos de Consumo en este equipo: las transcripciones, las carpetas historial
/// de sincronización y, si se da, la copia local del histórico largo (`historico_pc`, del encargo 8).
pub fn medir_datos(proyectos: &Path, historiales: &[PathBuf], historico_pc: Option<&Path>) -> f64 {
    let mut n = tam_carpeta(proyectos);
    n += historiales.iter().map(|c| tam_carpeta(c)).sum::<u64>();
    if let Some(p) = historico_pc {
        n += tam_carpeta(p);
    }
    n as f64 / GB
}

/// Bytes libres en la unidad de `ruta` (o de la carpeta existente más cercana por encima); None
/// si no se puede saber.
pub fn espacio_libre(ruta: &Path) -> Option<u64> {
    let mut r = ruta;
    while !r.is_dir() {
        r = r.parent()?;
    }
    let mut ancha: Vec<u16> = r.as_os_str().to_string_lossy().encode_utf16().collect();
    ancha.push(0);
    let mut libres = 0u64;
    // SAFETY: `ancha` termina en 0 y vive durante la llamada; el puntero de salida es válido.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(ancha.as_ptr()), None, None, Some(&mut libres)).ok()? };
    Some(libres)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NivelDisco {
    Ok,
    /// Desde el 80 % del máximo.
    Aviso,
    /// Pasado el máximo.
    Lleno,
}

fn coma(v: f64, decimales: usize) -> String {
    format!("{v:.decimales$}").replace('.', ",")
}

/// `{:g}` de Python para el máximo: sin ceros sobrantes.
fn general(v: f64) -> String {
    let mut s = format!("{v:.6}");
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    s.replace('.', ",")
}

/// El indicador de espacio de datos: (texto, fracción 0-1 o None sin máximo, nivel). Coma decimal.
pub fn texto_datos(gb: f64, gb_max: f64) -> (String, Option<f64>, NivelDisco) {
    if !(gb_max > 0.0) {
        return (format!("◔ {} GB", coma(gb, 2)), None, NivelDisco::Ok);
    }
    let nivel = if gb > gb_max {
        NivelDisco::Lleno
    } else if gb >= 0.8 * gb_max {
        NivelDisco::Aviso
    } else {
        NivelDisco::Ok
    };
    (
        format!("◔ {} / {} GB", coma(gb, 2), general(gb_max)),
        Some((gb / gb_max).min(1.0)),
        nivel,
    )
}

/// Lo que ve la interfaz del espacio de datos.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatosDisco {
    /// GB de datos (None hasta la primera medida).
    pub gb: Option<f64>,
    /// Máximo que puso el usuario (0 = sin máximo).
    pub gb_max: f64,
    /// GB libres en la unidad de los datos (None si no se pudo saber o aún no se midió).
    pub libre: Option<f64>,
    pub texto: String,
    pub fraccion: Option<f64>,
    pub nivel: NivelDisco,
}

impl Default for DatosDisco {
    fn default() -> Self {
        DatosDisco { gb: None, gb_max: 0.0, libre: None, texto: String::new(), fraccion: None, nivel: NivelDisco::Ok }
    }
}

/// Compone lo que se enseña a partir de una medida.
pub fn datos_disco(gb: f64, gb_max: f64, libre: Option<u64>) -> DatosDisco {
    let (texto, fraccion, nivel) = texto_datos(gb, gb_max);
    DatosDisco { gb: Some(gb), gb_max, libre: libre.map(|b| b as f64 / GB), texto, fraccion, nivel }
}

/// `gb_max` de `consumo_ajustes.json` (solo lectura): un número finito >= 0 (tope 1e6); si no, 0.
pub fn leer_gb_max(fichero: &Path) -> f64 {
    let Ok(t) = std::fs::read_to_string(fichero) else {
        return 0.0;
    };
    let v: serde_json::Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap_or_default();
    match v.get("gb_max").and_then(serde_json::Value::as_f64) {
        Some(x) if x.is_finite() && x >= 0.0 => x.min(1e6),
        _ => 0.0,
    }
}
