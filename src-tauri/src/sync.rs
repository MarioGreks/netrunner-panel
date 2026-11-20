//! Piezas comunes de la sincronización entre equipos por una carpeta de OneDrive.
//!
//! Todos los archivos viven en `<carpeta>/netrunner-panel/`. Cada módulo (notas,
//! cuentas atrás, interfaz y ajustes) aporta su fusión; aquí están la escritura
//! atómica, la lectura segura del remoto, las reglas de desempate y la ejecución de
//! los pasos de una sincronización (un paso que falla no impide los demás).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

pub const SUBCARPETA: &str = "netrunner-panel";
/// Las marcas de borrado se conservan 30 días para que se propaguen.
pub const VIDA_MARCAS_MS: i64 = 30 * 24 * 3600 * 1000;

pub fn ahora_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn ruta_remota(carpeta: &str, archivo: &str) -> PathBuf {
    Path::new(carpeta).join(SUBCARPETA).join(archivo)
}

/// Elige entre dos versiones del mismo elemento: gana la `modificada` más reciente;
/// si empatan, gana la versión borrada; si siguen empatadas, se conserva la local.
pub fn gana_remota(l_mod: i64, l_borr: bool, r_mod: i64, r_borr: bool) -> bool {
    if r_mod != l_mod {
        r_mod > l_mod
    } else {
        r_borr && !l_borr
    }
}

pub fn fusionar_lista<T: Clone>(
    local: &[T],
    remoto: &[T],
    id: impl Fn(&T) -> &str,
    info: impl Fn(&T) -> (i64, bool),
) -> Vec<T> {
    let mut resultado: Vec<T> = local.to_vec();
    let mut indice: HashMap<String, usize> = HashMap::new();
    for (i, e) in resultado.iter().enumerate() {
        indice.insert(id(e).to_string(), i);
    }
    for r in remoto {
        match indice.get(id(r)) {
            Some(&i) => {
                let (lm, lb) = info(&resultado[i]);
                let (rm, rb) = info(r);
                if gana_remota(lm, lb, rm, rb) {
                    resultado[i] = r.clone();
                }
            }
            None => {
                indice.insert(id(r).to_string(), resultado.len());
                resultado.push(r.clone());
            }
        }
    }
    resultado
}

// Rutas locales que no se pudieron leer al arrancar: no se sobrescriben en esta ejecución.
static BLOQUEADAS: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Carga un JSON local. No existe: `None`. Ilegible tras 3 intentos: `None` y la ruta
/// queda bloqueada para escritura. Corrupto: se aparta como `<nombre>.corrupto.json` y `None`.
pub fn cargar_json<T: DeserializeOwned>(ruta: &Path) -> Option<T> {
    let mut intento = 1;
    let texto = loop {
        match std::fs::read_to_string(ruta) {
            Ok(t) => break t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) if intento < 3 => {
                intento += 1;
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(_) => {
                BLOQUEADAS.lock().unwrap().push(ruta.to_path_buf());
                eprintln!("No se pudo leer {}", ruta.display());
                return None;
            }
        }
    };
    match serde_json::from_str(&texto) {
        Ok(v) => Some(v),
        Err(_) => {
            let _ = std::fs::rename(ruta, ruta.with_extension("corrupto.json"));
            None
        }
    }
}

/// Escribe en un archivo temporal contiguo y lo renombra sobre el destino.
pub fn escribir_atomico<T: Serialize + ?Sized>(ruta: &Path, datos: &T) -> Result<(), String> {
    if BLOQUEADAS.lock().unwrap().iter().any(|b| b == ruta) {
        let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return Err(format!("no se pudo leer {nombre} al arrancar; reinicia la app para no perder datos"));
    }
    let dir = ruta.parent().ok_or("ruta sin carpeta")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("no se pudo crear la carpeta: {e}"))?;
    let json = serde_json::to_string_pretty(datos).map_err(|e| e.to_string())?;
    let tmp = ruta.with_extension(format!("tmp{}", std::process::id()));
    let res = std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, ruta));
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("no se pudo escribir: {e}"));
    }
    Ok(())
}

/// Lee el archivo remoto. `Ok(None)` si no existe; error si está bloqueado o corrupto.
/// Los mensajes no incluyen el contenido (puede haber datos privados).
pub fn leer_remoto<T: DeserializeOwned>(ruta: &Path) -> Result<Option<T>, String> {
    let texto = match std::fs::read_to_string(ruta) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Archivo remoto no accesible".into()),
    };
    serde_json::from_str::<T>(&texto)
        .map(Some)
        .map_err(|_| "Archivo remoto corrupto: no se sobrescribe".to_string())
}

/// Lee el remoto, fusiona con `local`, purga y escribe el resultado en el remoto (solo si
/// cambia, para no mover el archivo en OneDrive sin necesidad). Si el remoto no se puede
/// leer, no se escribe nada y se devuelve el error.
pub fn sincronizar_con<T: Serialize + DeserializeOwned + Clone + PartialEq>(
    local: &T,
    remoto: &Path,
    ahora: i64,
    fusionar: impl Fn(&T, &T) -> T,
    purgar: impl Fn(&mut T, i64),
) -> Result<T, String> {
    let existente = leer_remoto::<T>(remoto)?;
    let mut fusionado = match &existente {
        Some(r) => fusionar(local, r),
        None => local.clone(),
    };
    purgar(&mut fusionado, ahora);
    if existente.as_ref() != Some(&fusionado) {
        escribir_atomico(remoto, &fusionado)?;
    }
    Ok(fusionado)
}

/// Un paso de la sincronización: nombre corto y la acción.
pub type Paso<'a> = (&'static str, Box<dyn FnOnce() -> Result<(), String> + 'a>);

/// Ejecuta todos los pasos aunque alguno falle. Devuelve `(correctos, errores)`; cada
/// error lleva el nombre del paso delante.
pub fn ejecutar_pasos(pasos: Vec<Paso>) -> (usize, Vec<String>) {
    let mut bien = 0;
    let mut errores = Vec::new();
    for (nombre, accion) in pasos {
        match accion() {
            Ok(()) => bien += 1,
            Err(e) => errores.push(format!("{nombre}: {e}")),
        }
    }
    (bien, errores)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use serde_json::{json, Value};

    fn carpeta(nombre: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nr_sync_{nombre}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn cargar_json_inexistente_no_bloquea() {
        let d = carpeta("noexiste");
        let r = d.join("a.json");
        assert!(cargar_json::<Value>(&r).is_none());
        escribir_atomico(&r, &json!({"a": 1})).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cargar_json_corrupto_se_aparta() {
        let d = carpeta("corrupto");
        let r = d.join("b.json");
        std::fs::write(&r, "{ roto").unwrap();
        assert!(cargar_json::<Value>(&r).is_none());
        assert!(d.join("b.corrupto.json").exists());
        assert!(!r.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cargar_json_ilegible_bloquea_escritura() {
        let d = carpeta("ilegible");
        let r = d.join("x.json");
        std::fs::create_dir(&r).unwrap();
        assert!(cargar_json::<Value>(&r).is_none());
        let e = escribir_atomico(&r, &json!({})).unwrap_err();
        assert!(e.contains("reinicia"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cargar_json_valido() {
        let d = carpeta("valido");
        let r = d.join("c.json");
        std::fs::write(&r, r#"{"a":1}"#).unwrap();
        assert_eq!(cargar_json::<Value>(&r), Some(json!({"a": 1})));
        let _ = std::fs::remove_dir_all(&d);
    }
}
