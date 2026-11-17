//! Lectura y fusión del config.json de la app (%APPDATA%\com.netrunner.panel).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}};

pub const IDENTIFICADOR_APP: &str = "com.netrunner.panel";
const TEMAS: [&str; 6] = ["netrunner", "neon", "terminal", "sangre", "ambar", "hielo"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Borde {
    Left,
    Top,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opciones {
    pub tema: String,
    pub edge: Borde,
    pub autostart: bool,
}

impl Default for Opciones {
    fn default() -> Self {
        Self { tema: "netrunner".into(), edge: Borde::Right, autostart: true }
    }
}

impl Opciones {
    /// Un tema desconocido se trata como «netrunner».
    pub fn normalizada(mut self) -> Self {
        if !TEMAS.contains(&self.tema.as_str()) {
            self.tema = "netrunner".into();
        }
        self
    }
}

/// Carpeta de datos de la app (no la del instalador, que tiene otro identificador).
pub fn carpeta_config_app() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join(IDENTIFICADOR_APP))
}

/// Opciones guardadas en un config.json; valores por defecto si falta o es ilegible.
pub fn leer_opciones(ruta: &Path) -> Opciones {
    let Some(v) = fs::read_to_string(ruta).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
        return Opciones::default();
    };
    let d = Opciones::default();
    let edge = v.get("edge").and_then(|e| serde_json::from_value(e.clone()).ok()).unwrap_or(d.edge);
    Opciones {
        tema: v.get("tema").and_then(Value::as_str).unwrap_or(&d.tema).to_string(),
        edge,
        autostart: v.get("autostart").and_then(Value::as_bool).unwrap_or(d.autostart),
    }
    .normalizada()
}

/// Fusiona SOLO tema, edge y autostart en el config.json de `ruta`, conservando el resto.
/// Crea carpeta y fichero si faltan. Escritura segura: temporal + rename.
pub fn fusionar_config(ruta: &Path, op: &Opciones) -> Result<(), String> {
    let op = op.clone().normalizada();
    let mut raiz = match fs::read_to_string(ruta) {
        Ok(t) => match serde_json::from_str::<Value>(&t) {
            Ok(v @ Value::Object(_)) => v,
            _ => {
                // Ilegible: se guarda copia y se empieza de cero.
                let _ = fs::copy(ruta, ruta.with_extension("json.corrupto"));
                json!({})
            }
        },
        Err(_) => json!({}),
    };
    let obj = raiz.as_object_mut().expect("raíz objeto");
    obj.insert("tema".into(), json!(op.tema));
    obj.insert("edge".into(), serde_json::to_value(op.edge).map_err(|e| e.to_string())?);
    obj.insert("autostart".into(), json!(op.autostart));

    if let Some(dir) = ruta.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear la carpeta de ajustes: {e}"))?;
    }
    let tmp = ruta.with_extension("json.tmp");
    let texto = serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?;
    fs::write(&tmp, texto).map_err(|e| format!("No se pudo escribir los ajustes: {e}"))?;
    fs::rename(&tmp, ruta).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("No se pudo guardar los ajustes: {e}")
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn dir_temporal(nombre: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("netrunner-test-{nombre}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn conserva_campos_ajenos_y_cambia_los_tres() {
        let d = dir_temporal("fusion");
        fs::create_dir_all(&d).unwrap();
        let ruta = d.join("config.json");
        fs::write(&ruta, r#"{"tema":"neon","edge":"left","autostart":false,"opacidad":0.8,"ics_url":"x"}"#).unwrap();
        let op = Opciones { tema: "hielo".into(), edge: Borde::Top, autostart: true };
        fusionar_config(&ruta, &op).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&ruta).unwrap()).unwrap();
        assert_eq!(v["tema"], "hielo");
        assert_eq!(v["edge"], "top");
        assert_eq!(v["autostart"], true);
        assert_eq!(v["opacidad"], 0.8);
        assert_eq!(v["ics_url"], "x");
        assert!(!ruta.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn crea_el_fichero_y_la_carpeta_si_no_existen() {
        let d = dir_temporal("nuevo");
        let ruta = d.join("sub").join("config.json");
        fusionar_config(&ruta, &Opciones { tema: "desconocido".into(), edge: Borde::Right, autostart: false }).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&ruta).unwrap()).unwrap();
        assert_eq!(v["tema"], "netrunner");
        assert_eq!(v["edge"], "right");
        assert_eq!(v["autostart"], false);
        let _ = fs::remove_dir_all(&d);
    }
}
