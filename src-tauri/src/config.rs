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
    /// Carpeta de OneDrive para sincronizar las notas (None = solo local).
    pub carpeta_sync: Option<String>,
    /// Id del tema de color. Un valor desconocido se trata como «netrunner» en la interfaz.
    pub tema: String,
    /// Opacidad del fondo del panel, limitada a [OPACIDAD_MIN, OPACIDAD_MAX].
    pub opacidad: f64,
    /// Con `true` el borde de la pantalla no abre el panel (se activa desde la bandeja).
    pub borde_pausado: bool,
}

pub const OPACIDAD_MIN: f64 = 0.55;
pub const OPACIDAD_MAX: f64 = 1.0;
pub const OPACIDAD_DEFECTO: f64 = 0.94;

impl Default for Config {
    fn default() -> Self {
        Config {
            edge: Borde::Right,
            autostart: true,
            ics_url: None,
            ciudad: None,
            carpeta_sync: None,
            tema: "netrunner".to_string(),
            opacidad: OPACIDAD_DEFECTO,
            borde_pausado: false,
        }
    }
}

impl Config {
    /// Deja los valores en rango (opacidad no finita = valor por defecto).
    pub fn normalizar(&mut self) {
        self.opacidad = if self.opacidad.is_finite() {
            self.opacidad.clamp(OPACIDAD_MIN, OPACIDAD_MAX)
        } else {
            OPACIDAD_DEFECTO
        };
    }
}

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("config.json"))
}

/// Lee la configuración; si falta o está corrupta, usa los valores por defecto.
pub fn cargar(app: &AppHandle) -> Config {
    ruta(app)
        .and_then(|r| std::fs::read_to_string(r).ok())
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .map(|mut c| {
            c.normalizar();
            c
        })
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

/// Cambia solo `borde_pausado` (lo usa el menú de la bandeja) y lo guarda.
pub fn fijar_pausa(app: &AppHandle, pausado: bool) {
    let estado = app.state::<Estado>();
    let mut c = estado.config.lock().unwrap();
    c.borde_pausado = pausado;
    let _ = guardar(app, &c);
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
pub fn set_config(app: AppHandle, estado: State<Estado>, mut config: Config) -> Result<(), String> {
    config.normalizar();
    guardar(&app, &config)?;
    let anterior = std::mem::replace(&mut *estado.config.lock().unwrap(), config.clone());
    sincronizar_autoarranque(&app, config.autostart);
    ventanas::reposicionar(&app);
    let _ = app.emit("config://cambio", config.edge);
    if anterior.tema != config.tema || anterior.opacidad != config.opacidad {
        let _ = app.emit(
            "tema://cambio",
            serde_json::json!({ "tema": config.tema, "opacidad": config.opacidad }),
        );
    }
    if anterior.ics_url != config.ics_url {
        crate::calendario::sincronizar(&app);
    }
    if anterior.ciudad != config.ciudad {
        crate::meteo::actualizar(&app);
    }
    if anterior.carpeta_sync != config.carpeta_sync {
        crate::notas::sincronizar(&app);
    }
    Ok(())
}

/// Devuelve la ruta de los datos locales (`app_data_dir()`), creándola si falta.
/// Con `abrir` también la abre en el Explorador con ShellExecuteW; así la interfaz no
/// necesita ningún permiso de shell ni de rutas.
#[tauri::command]
pub fn abrir_carpeta_datos(app: AppHandle, abrir: bool) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if abrir {
        use windows::core::{w, HSTRING, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let h = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &HSTRING::from(dir.as_os_str()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // Éxito si el valor devuelto es mayor que 32
        if h.0 as isize <= 32 {
            return Err("No se pudo abrir la carpeta".into());
        }
    }
    Ok(dir.to_string_lossy().into_owned())
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
    fn json_viejo_sin_tema_ni_opacidad_usa_los_valores_por_defecto() {
        let c: Config = serde_json::from_str(r#"{"edge":"left","autostart":false}"#).unwrap();
        assert_eq!(c.tema, "netrunner");
        assert_eq!(c.opacidad, 0.94);
        assert_eq!(c.edge, Borde::Left);
    }

    #[test]
    fn opacidad_fuera_de_rango_se_limita() {
        let mut c: Config = serde_json::from_str(r#"{"opacidad":0.1}"#).unwrap();
        c.normalizar();
        assert_eq!(c.opacidad, 0.55);
        c.opacidad = 7.0;
        c.normalizar();
        assert_eq!(c.opacidad, 1.0);
        c.opacidad = f64::NAN;
        c.normalizar();
        assert_eq!(c.opacidad, 0.94);
        c.opacidad = 0.8;
        c.normalizar();
        assert_eq!(c.opacidad, 0.8);
    }

    #[test]
    fn borde_pausado_por_defecto_y_guardado() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert!(!c.borde_pausado);
        let c: Config = serde_json::from_str(r#"{"borde_pausado":true}"#).unwrap();
        assert!(c.borde_pausado);
        let ida: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(ida, c);
    }

    #[test]
    fn campos_parciales_y_desconocidos() {
        let c: Config = serde_json::from_str(r#"{"edge":"top","futuro":1}"#).unwrap();
        assert_eq!(c.edge, Borde::Top);
        assert!(c.autostart);
    }
}
