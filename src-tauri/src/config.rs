//! Configuración persistente. Vive solo en `app_config_dir()/config.json`.
//! `#[serde(default)]` permite añadir campos sin romper archivos antiguos.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::estado::Estado;
use crate::ventanas;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Borde {
    Left,
    Right,
    Top,
}

impl Default for Borde {
    fn default() -> Self {
        Borde::Right
    }
}

/// Ciudad elegida para el tiempo meteorológico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ciudad {
    pub nombre: String,
    pub pais: String,
    pub latitud: f64,
    pub longitud: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub edge: Borde,
    pub autostart: bool,
    /// URL ICS del calendario. Es privada: nunca debe ir a logs ni a mensajes de error.
    pub ics_url: Option<String>,
    pub ciudad: Option<Ciudad>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            edge: Borde::Right,
            autostart: true,
            ics_url: None,
            ciudad: None,
        }
    }
}

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("config.json"))
}

/// Lee la configuración; si falta o está corrupta, usa los valores por defecto.
pub fn cargar(app: &AppHandle) -> Config {
    ruta(app)
        .and_then(|r| std::fs::read_to_string(r).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn guardar(app: &AppHandle, config: &Config) -> Result<(), String> {
    let r = ruta(app).ok_or("sin directorio de configuración")?;
    if let Some(dir) = r.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(r, json).map_err(|e| e.to_string())
}

/// Hace coincidir el registro de autoarranque con la configuración.
pub fn sincronizar_autoarranque(app: &AppHandle, activo: bool) {
    let gestor = app.autolaunch();
    let actual = gestor.is_enabled().unwrap_or(false);
    if activo && !actual {
        let _ = gestor.enable();
    } else if !activo && actual {
        let _ = gestor.disable();
    }
}

#[tauri::command]
pub fn get_config(estado: State<Estado>) -> Config {
    estado.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(app: AppHandle, estado: State<Estado>, config: Config) -> Result<(), String> {
    guardar(&app, &config)?;
    let anterior = std::mem::replace(&mut *estado.config.lock().unwrap(), config.clone());
    sincronizar_autoarranque(&app, config.autostart);
    ventanas::reposicionar(&app);
    let _ = app.emit("config://cambio", config.edge);
    if anterior.ics_url != config.ics_url {
        crate::calendario::sincronizar(&app);
    }
    if anterior.ciudad != config.ciudad {
        crate::meteo::actualizar(&app);
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn json_vacio_da_valores_por_defecto() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.edge, Borde::Right);
        assert!(c.autostart);
        assert!(c.ics_url.is_none() && c.ciudad.is_none());
    }

    #[test]
    fn campos_parciales_y_desconocidos() {
        let c: Config = serde_json::from_str(r#"{"edge":"top","futuro":1}"#).unwrap();
        assert_eq!(c.edge, Borde::Top);
        assert!(c.autostart);
    }
}
