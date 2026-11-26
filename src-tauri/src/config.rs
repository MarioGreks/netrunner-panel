//! Configuración persistente. Vive solo en `app_config_dir()/config.json`.
//! `#[serde(default)]` permite añadir campos sin romper archivos antiguos.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// Cuenta Microsoft conectada. Aquí NUNCA hay tokens: el refresh_token vive en el
/// Administrador de credenciales de Windows y el access_token solo en memoria.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CuentaMs {
    /// `oid.tid` del id_token.
    pub id: String,
    pub correo: String,
    pub nombre: String,
    /// Editable por el usuario (UAH, PERSONAL…).
    pub etiqueta: String,
    pub tenant: String,
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
    /// Ocultar la miniventana y el borde mientras haya algo a pantalla completa.
    pub ocultar_pantalla_completa: bool,
    /// «Id. de aplicación (cliente)» del registro de la app en Entra. No es secreto, pero
    /// lo pone el usuario y no va en el código.
    pub ms_client_id: Option<String>,
    /// Cuentas Microsoft conectadas (sin tokens).
    pub cuentas_ms: Vec<CuentaMs>,
    /// Lanzar el Outlook clásico minimizado al arrancar si no está abierto.
    pub outlook_auto: bool,
    /// Remitentes VIP: dirección exacta (`ana@example.com`) o dominio (`@example.com`, `example.com`).
    pub correo_vip: Vec<String>,
    /// Palabras clave del asunto o del remitente (sin distinguir mayúsculas ni acentos).
    pub correo_palabras: Vec<String>,
    /// Etiqueta elegida para cada cuenta de Outlook (de `DisplayName` a etiqueta).
    pub etiquetas_cuentas_correo: HashMap<String, String>,
    /// Conexión móvil (servidor HTTP en la red local). Solo local: no se sincroniza entre equipos.
    pub movil_activo: bool,
    pub movil_puerto: u16,
    /// Token del móvil emparejado. Secreto: nunca a logs ni a errores.
    pub movil_token: Option<String>,
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
            ocultar_pantalla_completa: true,
            ms_client_id: None,
            cuentas_ms: Vec::new(),
            outlook_auto: true,
            correo_vip: Vec::new(),
            correo_palabras: Vec::new(),
            etiquetas_cuentas_correo: HashMap::new(),
            movil_activo: false,
            movil_puerto: crate::movil::PUERTO_DEFECTO,
            movil_token: None,
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
        limpiar_lista(&mut self.correo_vip);
        limpiar_lista(&mut self.correo_palabras);
        limpiar_etiquetas(&mut self.etiquetas_cuentas_correo);
    }
}

/// Recorta las etiquetas y descarta las entradas con clave o etiqueta vacía.
pub(crate) fn limpiar_etiquetas(e: &mut HashMap<String, String>) {
    e.retain(|k, v| {
        *v = v.trim().to_string();
        !k.trim().is_empty() && !v.is_empty()
    });
}

/// Quita espacios y vacíos y elimina repetidos (sin distinguir mayúsculas), conservando el orden.
pub(crate) fn limpiar_lista(lista: &mut Vec<String>) {
    let mut vistos: Vec<String> = Vec::new();
    lista.retain_mut(|t| {
        *t = t.trim().to_string();
        let clave = t.to_lowercase();
        if t.is_empty() || vistos.contains(&clave) {
            return false;
        }
        vistos.push(clave);
        true
    });
}

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("config.json"))
}

/// Lee la configuración; si falta, usa los valores por defecto; si está corrupta, se
/// aparta como `config.corrupto.json` y se usan los valores por defecto.
pub fn cargar(app: &AppHandle) -> Config {
    ruta(app)
        .and_then(|r| crate::sync::cargar_json::<Config>(&r))
        .map(|mut c| {
            c.normalizar();
            c
        })
        .unwrap_or_default()
}

pub(crate) fn guardar(app: &AppHandle, config: &Config) -> Result<(), String> {
    let r = ruta(app).ok_or("sin directorio de configuración")?;
    crate::sync::escribir_atomico(&r, config)
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
pub fn set_config(app: AppHandle, config: Config) -> Result<(), String> {
    aplicar(&app, Origen::Usuario, move |_| config)
}

/// Quién cambia la configuración.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Origen {
    /// El formulario de AJUSTES (o la bandeja): se aplica todo y se anotan las marcas de
    /// tiempo de los ajustes compartidos.
    Usuario,
    /// Valores llegados de otro equipo por la sincronización: solo se aplican los efectos
    /// de los ajustes compartidos y no se programa otra sincronización (evita bucles).
    Remoto,
}

/// Único camino para cambiar la configuración. `cambio` recibe la configuración actual y
/// devuelve la nueva; se ejecuta con el bloqueo tomado, así que ningún otro cambio se
/// pierde entre la lectura y la escritura. Después reacciona a lo que cambió.
pub fn aplicar(app: &AppHandle, origen: Origen, cambio: impl FnOnce(&Config) -> Config) -> Result<(), String> {
    let estado = app.state::<Estado>();
    let (anterior, config) = {
        let mut actual = estado.config.lock().unwrap();
        let mut config = cambio(&actual);
        config.normalizar();
        // Las cuentas las gestiona solo el módulo Microsoft: un formulario con una copia
        // antigua de la configuración no debe pisarlas.
        config.cuentas_ms = actual.cuentas_ms.clone();
        // El token del móvil lo gestiona solo el módulo movil (emparejar/desvincular).
        config.movil_token = actual.movil_token.clone();
        guardar(app, &config)?;
        (std::mem::replace(&mut *actual, config.clone()), config)
    };
    if origen == Origen::Usuario {
        sincronizar_autoarranque(app, config.autostart);
        ventanas::reposicionar(app);
        let _ = app.emit("config://cambio", config.edge);
        if anterior.tema != config.tema || anterior.opacidad != config.opacidad {
            let _ = app.emit(
                "tema://cambio",
                serde_json::json!({ "tema": config.tema, "opacidad": config.opacidad }),
            );
        }
    }
    if anterior.ics_url != config.ics_url {
        crate::calendario::sincronizar(app);
    }
    if anterior.ciudad != config.ciudad {
        crate::meteo::actualizar(app);
    }
    if origen == Origen::Usuario {
        if anterior.carpeta_sync != config.carpeta_sync {
            crate::notas::sincronizar(app);
        }
        if !anterior.outlook_auto && config.outlook_auto {
            crate::outlook::activar_auto(app);
        }
        // Anota la marca de tiempo de los ajustes compartidos que cambiaron y programa
        // la sincronización
        crate::ajustes::cambios_locales(app, &anterior, &config);
    }
    if anterior.correo_vip != config.correo_vip
        || anterior.correo_palabras != config.correo_palabras
        || anterior.etiquetas_cuentas_correo != config.etiquetas_cuentas_correo
    {
        crate::outlook::refiltrar(app);
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
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        if !crate::accesos::shell_execute(&windows::core::HSTRING::from(dir.as_os_str()), SW_SHOWNORMAL) {
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
    fn ocultar_pantalla_completa_por_defecto_y_antigua() {
        // Config antigua sin el campo: activado
        let c: Config = serde_json::from_str(r#"{"edge":"left"}"#).unwrap();
        assert!(c.ocultar_pantalla_completa);
        let c: Config = serde_json::from_str(r#"{"ocultar_pantalla_completa":false}"#).unwrap();
        assert!(!c.ocultar_pantalla_completa);
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
    fn config_antigua_sin_campos_microsoft() {
        let c: Config = serde_json::from_str(r#"{"edge":"left","tema":"netrunner"}"#).unwrap();
        assert!(c.ms_client_id.is_none());
        assert!(c.cuentas_ms.is_empty());
        let c: Config = serde_json::from_str(
            r#"{"ms_client_id":"00000000-0000-0000-0000-000000000000","cuentas_ms":[{"id":"a.b","correo":"x@example.com"}]}"#,
        )
        .unwrap();
        assert_eq!(c.cuentas_ms[0].correo, "x@example.com");
        assert_eq!(c.cuentas_ms[0].etiqueta, "");
        let json = serde_json::to_string(&c).unwrap();
        // (movil_token es el del móvil, no de Microsoft)
        assert!(!json.contains("refresh_token") && !json.contains("access_token"));
    }

    #[test]
    fn config_antigua_sin_campos_de_correo() {
        let c: Config = serde_json::from_str(r#"{"edge":"left","ics_url":"https://example.com/a.ics"}"#).unwrap();
        assert!(c.outlook_auto);
        assert!(c.correo_vip.is_empty() && c.correo_palabras.is_empty());
        assert!(c.etiquetas_cuentas_correo.is_empty());
        let c: Config = serde_json::from_str(r#"{"outlook_auto":false,"correo_vip":["@example.com"],"etiquetas_cuentas_correo":{"Cuenta A":"UNI"}}"#).unwrap();
        assert!(!c.outlook_auto);
        assert_eq!(c.correo_vip, vec!["@example.com"]);
        assert_eq!(c.etiquetas_cuentas_correo["Cuenta A"], "UNI");
    }

    #[test]
    fn normalizar_limpia_listas_de_correo() {
        let mut c = Config::default();
        c.correo_vip = vec![" A@example.com ".into(), "a@EXAMPLE.com".into(), "".into(), "example.org".into()];
        c.etiquetas_cuentas_correo.insert("X".into(), "  ".into());
        c.etiquetas_cuentas_correo.insert("Y".into(), " UNI ".into());
        c.normalizar();
        assert_eq!(c.correo_vip, vec!["A@example.com", "example.org"]);
        assert_eq!(c.etiquetas_cuentas_correo.len(), 1);
        assert_eq!(c.etiquetas_cuentas_correo["Y"], "UNI");
    }

    #[test]
    fn config_antigua_sin_campos_moviles() {
        let c: Config = serde_json::from_str(r#"{"edge":"left"}"#).unwrap();
        assert!(!c.movil_activo);
        assert_eq!(c.movil_puerto, 47821);
        assert!(c.movil_token.is_none());
    }

    #[test]
    fn campos_parciales_y_desconocidos() {
        let c: Config = serde_json::from_str(r#"{"edge":"top","futuro":1}"#).unwrap();
        assert_eq!(c.edge, Borde::Top);
        assert!(c.autostart);
    }
}
