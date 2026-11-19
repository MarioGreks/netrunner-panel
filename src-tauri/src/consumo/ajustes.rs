//! Ajustes de Consumo que se editan desde AJUSTES de netrunner: contexto máximo, avisos y máximo
//! de espacio (`consumo_ajustes.json`, con muchas otras claves de Consumo que se conservan tal
//! cual) y carpetas de sincronización (`consumo_sync`). Mismo formato que la app Consumo.
//!
//! Los avisos ya dados (`consumo_avisos.json`) también viven aquí: lista de
//! `[ventana, reset, umbral]`; se descartan los de ventanas ya renovadas.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, State};

use super::contexto::CTX_MAX;
use super::equipos::escribir_atomico;
use super::historico::PRECIO_EUR;
use super::historico_vista::{limpiar_vista_historico, VistaHist};
use super::limites::{avisos_nuevos, renovar_ventanas, Aviso, ClaveAviso, Uso};
use super::vigilancia::Vigilancia;
use super::{bloquear, vuelta, Consumo};

pub const FICHERO_AJUSTES: &str = "consumo_ajustes.json";
pub const FICHERO_AVISOS: &str = "consumo_avisos.json";
/// Máximo de GB que se acepta (el mismo tope que `leer_gb_max`).
const GB_TOPE: f64 = 1e6;

/// Lo que ve y guarda la interfaz.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AjustesConsumo {
    /// Ventana de contexto en tokens (200000, 500000 o 1000000).
    pub ctx_max: u64,
    /// Avisos al 80 % y al 100 %.
    pub avisos: bool,
    /// Máximo de espacio de datos en GB (0 = sin máximo).
    pub gb_max: f64,
    /// Carpetas de sincronización, la principal primero (sin repetir).
    pub carpetas_sync: Vec<String>,
    /// Carpeta que se usa si no se elige ninguna (solo al leer; se ignora al guardar).
    #[serde(default)]
    pub carpeta_defecto: String,
}

fn leer_json(fichero: &Path) -> Value {
    let Ok(t) = fs::read_to_string(fichero) else {
        return Value::Null;
    };
    serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap_or(Value::Null)
}

/// `ctx_max` válido de un JSON de ajustes: un entero positivo; si no, el de por defecto.
pub fn ctx_max_de(v: &Value) -> u64 {
    v.get("ctx_max").and_then(Value::as_u64).filter(|c| *c > 0).unwrap_or(CTX_MAX)
}

/// (ctx_max, avisos, gb_max) de `consumo_ajustes.json`, con los valores por defecto de Consumo.
pub fn leer_ajustes(fichero: &Path) -> (u64, bool, f64) {
    let v = leer_json(fichero);
    let avisos = v.get("avisos").and_then(Value::as_bool).unwrap_or(true);
    let gb = match v.get("gb_max").and_then(Value::as_f64) {
        Some(x) if x.is_finite() && x >= 0.0 => x.min(GB_TOPE),
        _ => 0.0,
    };
    (ctx_max_de(&v), avisos, gb)
}

/// `suscripcion_eur` de `consumo_ajustes.json` (solo lectura): los € al mes de la suscripción, con
/// los que el Histórico pasa el % del límite a € de coste real; un valor que no sirve (no es un
/// número, <= 0 o > 5.000) vale el de por defecto.
pub fn leer_suscripcion(fichero: &Path) -> f64 {
    let v = leer_json(fichero);
    match v.get("suscripcion_eur") {
        Some(x) if super::historico_vista::precio_valido(x) => x.as_f64().unwrap_or(PRECIO_EUR),
        _ => PRECIO_EUR,
    }
}

/// `suscripcion` de `consumo_ajustes.json` (solo lectura): los $ al mes con los que el resumen
/// compara el coste a precio de API; un valor que no sirve (no es un número o < 0) vale 20.
pub fn leer_suscripcion_usd(fichero: &Path) -> f64 {
    let v = leer_json(fichero);
    match v.get("suscripcion").and_then(Value::as_f64) {
        Some(x) if x.is_finite() && x >= 0.0 => x,
        _ => 20.0,
    }
}

/// Guarda las tres claves conservando todo lo demás del fichero. Solo escribe si algo cambia
/// (True) y de forma atómica. Un fichero que existe pero no es un objeto JSON no se toca.
pub fn guardar_ajustes(fichero: &Path, ctx_max: u64, avisos: bool, gb_max: f64) -> io::Result<bool> {
    let existe = fichero.exists();
    let mut mapa = match leer_json(fichero) {
        Value::Object(m) => m,
        Value::Null if !existe => Map::new(),
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "consumo_ajustes.json no es válido")),
    };
    let gb = if gb_max.is_finite() && gb_max >= 0.0 { (gb_max.min(GB_TOPE) * 100.0).round() / 100.0 } else { 0.0 };
    let antes = mapa.clone();
    mapa.insert("ctx_max".into(), ctx_max.into());
    mapa.insert("avisos".into(), avisos.into());
    mapa.insert("gb_max".into(), serde_json::Number::from_f64(gb).map_or(0.into(), Value::Number));
    if existe && mapa == antes {
        return Ok(false);
    }
    let carpeta = fichero.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(carpeta)?;
    let nombre = fichero.file_name().and_then(|n| n.to_str()).unwrap_or(FICHERO_AJUSTES);
    escribir_atomico(carpeta, nombre, &Value::Object(mapa).to_string())?;
    Ok(true)
}

/// El ajuste `historico_vista` (lo último que se vio en el Histórico), limpio y completo; sin
/// fichero o roto vale lo de la primera vez.
pub fn leer_vista_historico(fichero: &Path) -> VistaHist {
    limpiar_vista_historico(leer_json(fichero).get("historico_vista").unwrap_or(&Value::Null))
}

/// Guarda la vista del Histórico en la clave `historico_vista`: lo que llega se mezcla sobre lo
/// guardado (así se conservan las casillas, la geometría... de Consumo), se sanea y solo se
/// escribe, de forma atómica, si cambia algo. Las demás claves del fichero no se tocan. Un fichero
/// que existe pero no es un objeto JSON no se toca.
pub fn guardar_vista_historico(fichero: &Path, vista: &Value) -> io::Result<bool> {
    let existe = fichero.exists();
    let mut mapa = match leer_json(fichero) {
        Value::Object(m) => m,
        Value::Null if !existe => Map::new(),
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "consumo_ajustes.json no es válido")),
    };
    let actual = limpiar_vista_historico(mapa.get("historico_vista").unwrap_or(&Value::Null));
    let mut mezcla = match mapa.get("historico_vista") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    if let Value::Object(nuevo) = vista {
        for (k, v) in nuevo {
            mezcla.insert(k.clone(), v.clone());
        }
    }
    let limpia = limpiar_vista_historico(&Value::Object(mezcla));
    if existe && limpia == actual && mapa.contains_key("historico_vista") {
        return Ok(false);
    }
    mapa.insert("historico_vista".into(), serde_json::to_value(&limpia).map_err(io::Error::other)?);
    let carpeta = fichero.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(carpeta)?;
    let nombre = fichero.file_name().and_then(|n| n.to_str()).unwrap_or(FICHERO_AJUSTES);
    escribir_atomico(carpeta, nombre, &Value::Object(mapa).to_string())?;
    Ok(true)
}

// ---------- avisos ya dados ----------

/// Los avisos ya dados que aún valen (su ventana no se ha renovado).
pub fn cargar_hechos(ruta: &Path, ahora: f64) -> HashSet<ClaveAviso> {
    let Value::Array(lista) = leer_json(ruta) else {
        return HashSet::new();
    };
    lista
        .iter()
        .filter_map(|h| {
            let a = h.as_array()?;
            let (v, r, u) = (a.first()?.as_str()?, a.get(1)?.as_f64()?, a.get(2)?.as_u64()?);
            (r > ahora).then(|| (v.to_string(), r as i64, u as u32))
        })
        .collect()
}

pub fn guardar_hechos(ruta: &Path, hechos: &HashSet<ClaveAviso>) -> io::Result<()> {
    let mut v: Vec<&ClaveAviso> = hechos.iter().collect();
    v.sort();
    let carpeta = ruta.parent().unwrap_or(Path::new("."));
    let nombre = ruta.file_name().and_then(|n| n.to_str()).unwrap_or(FICHERO_AVISOS);
    escribir_atomico(carpeta, nombre, &serde_json::to_string(&v).unwrap_or_else(|_| "[]".into()))
}

/// Avisos dados en memoria; el fichero solo se vuelve a leer si cambió su fecha (Consumo también
/// lo escribe).
#[derive(Default)]
pub struct Hechos {
    pub hechos: HashSet<ClaveAviso>,
    mtime: Option<SystemTime>,
    leido: bool,
}

impl Hechos {
    fn recargar(&mut self, ruta: &Path, ahora: f64) {
        let m = fs::metadata(ruta).and_then(|m| m.modified()).ok();
        if !self.leido || m != self.mtime {
            self.hechos = cargar_hechos(ruta, ahora);
            self.mtime = m;
            self.leido = true;
        }
    }
}

/// Calcula los avisos nuevos con el uso dado y los marca como hechos (se guardan en `ruta`).
/// Devuelve los que hay que enseñar: con los avisos desactivados se dan por hechos sin mostrar
/// nada, para que al reactivarlos no salgan de golpe. `permitido` se pregunta solo si hay algo
/// nuevo (con Consumo.exe abierto avisa él: no se avisa ni se escribe).
pub fn procesar_avisos(
    estado: &mut Hechos,
    ruta: &Path,
    uso: &Uso,
    ahora: f64,
    activos: bool,
    permitido: impl FnOnce() -> bool,
) -> Vec<Aviso> {
    estado.recargar(ruta, ahora);
    let nuevos = avisos_nuevos(&renovar_ventanas(uso, ahora), &estado.hechos, ahora);
    if nuevos.is_empty() || !permitido() {
        return Vec::new();
    }
    for a in &nuevos {
        estado.hechos.extend(a.claves.iter().cloned());
    }
    if guardar_hechos(ruta, &estado.hechos).is_ok() {
        estado.mtime = fs::metadata(ruta).and_then(|m| m.modified()).ok();
    }
    if activos {
        nuevos
    } else {
        Vec::new()
    }
}

// ---------- comandos ----------

fn ruta_ajustes(c: &Consumo) -> Option<PathBuf> {
    c.carpeta.as_ref().map(|d| d.join(FICHERO_AJUSTES))
}

fn leer_todo(c: &Consumo) -> AjustesConsumo {
    let (ctx_max, avisos, gb_max) = ruta_ajustes(c).map_or((CTX_MAX, true, 0.0), |r| leer_ajustes(&r));
    let eq = bloquear(&c.equipos);
    AjustesConsumo {
        ctx_max,
        avisos,
        gb_max,
        carpetas_sync: eq.lista_sync().iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        carpeta_defecto: eq.carpeta_defecto().to_string_lossy().into_owned(),
    }
}

/// Ajustes actuales (los de disco: Consumo también puede haberlos cambiado).
#[tauri::command]
pub fn ajustes_consumo(c: State<'_, Consumo>) -> AjustesConsumo {
    leer_todo(&c)
}

/// Guarda los ajustes y los aplica sin reiniciar: el contexto usa el nuevo máximo, el indicador
/// de datos el nuevo tope y las carpetas de sincronización se releen (cachés invalidadas).
#[tauri::command]
pub fn guardar_ajustes_consumo(
    ajustes: AjustesConsumo,
    app: AppHandle,
    c: State<'_, Consumo>,
    vig: State<'_, Vigilancia>,
) -> Result<AjustesConsumo, String> {
    if ![200_000, 500_000, 1_000_000].contains(&ajustes.ctx_max) {
        return Err("Contexto máximo no válido".into());
    }
    let ruta = ruta_ajustes(&c).ok_or("Sin carpeta de ajustes")?;
    guardar_ajustes(&ruta, ajustes.ctx_max, ajustes.avisos, ajustes.gb_max)
        .map_err(|e| format!("No se pudo guardar: {e}"))?;
    c.ctx_max.store(ajustes.ctx_max, Ordering::SeqCst);
    vig.aplicar_gb_max(ajustes.gb_max);
    let cambio_sync = {
        let mut eq = bloquear(&c.equipos);
        let cambio = eq
            .guardar_sync(&ajustes.carpetas_sync)
            .map_err(|e| format!("No se pudieron guardar las carpetas: {e}"))?;
        if cambio {
            eq.invalidar();
        }
        cambio
    };
    if cambio_sync {
        bloquear(&c.interno).firma_otros = None; // fuerza releer y publicar en la vuelta
        vuelta(&app, &c);
    }
    let _ = app.emit("consumo://ajustes", ());
    Ok(leer_todo(&c))
}

/// La vista del Histórico que se recuerda (la de la primera vez si no hay nada guardado).
#[tauri::command]
pub fn vista_historico(c: State<'_, Consumo>) -> VistaHist {
    ruta_ajustes(&c).map_or_else(|| limpiar_vista_historico(&Value::Null), |r| leer_vista_historico(&r))
}

/// Recuerda la vista del Histórico (periodo, grupo, unidad, modo, paso...): solo escribe si cambia.
/// Devuelve la vista ya saneada.
#[tauri::command]
pub fn guardar_vista_historico_consumo(vista: Value, c: State<'_, Consumo>) -> Result<VistaHist, String> {
    let ruta = ruta_ajustes(&c).ok_or("Sin carpeta de ajustes")?;
    guardar_vista_historico(&ruta, &vista).map_err(|e| format!("No se pudo guardar la vista: {e}"))?;
    Ok(leer_vista_historico(&ruta))
}
