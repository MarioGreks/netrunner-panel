//! Módulo METEO: tiempo actual y previsión de 14 días con Open-Meteo (sin clave).
//!
//! Refresca cada 30 min y al guardar una ciudad nueva. El último resultado bueno
//! se guarda en `app_data_dir()/meteo.json` y se emite `meteo://estado`.

use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::config::Ciudad;
use crate::estado::Estado;

const INTERVALO: Duration = Duration::from_secs(30 * 60);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Actual {
    pub temperatura: f64,
    pub sensacion: f64,
    pub humedad: f64,
    /// km/h
    pub viento: f64,
    pub codigo: i32,
    pub es_de_dia: bool,
    pub texto: String,
    pub icono: String,
    /// Grados (0 = norte, 90 = este): de dónde sopla el viento
    #[serde(default)]
    pub direccion_viento: Option<f64>,
    #[serde(default)]
    pub uv: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Dia {
    /// AAAA-MM-DD
    pub fecha: String,
    pub codigo: i32,
    pub maxima: Option<f64>,
    pub minima: Option<f64>,
    pub prob_lluvia: Option<f64>,
    pub texto: String,
    pub icono: String,
    /// AAAA-MM-DDTHH:MM, hora local de la ciudad
    #[serde(default)]
    pub salida_sol: Option<String>,
    #[serde(default)]
    pub puesta_sol: Option<String>,
    #[serde(default)]
    pub uv_max: Option<f64>,
}

/// Una hora de la previsión (las próximas 48 desde la actual).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hora {
    /// AAAA-MM-DDTHH:MM, en la hora local de la ciudad
    pub hora: String,
    pub temperatura: Option<f64>,
    pub prob_lluvia: Option<f64>,
    pub codigo: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EstadoMeteo {
    pub ciudad: Option<Ciudad>,
    pub actual: Option<Actual>,
    pub dias: Vec<Dia>,
    /// Próximas 48 horas (el panel usa solo las 24 primeras); vacío en la caché antigua.
    #[serde(default)]
    pub por_horas: Vec<Hora>,
    pub actualizado: Option<DateTime<Utc>>,
    pub cargando: bool,
    pub error: Option<String>,
}

pub struct Meteo(Mutex<EstadoMeteo>);

/// Código WMO a (texto en español, nombre de icono). Iconos: `sol`, `luna`, `nubes`,
/// `lluvia`, `nieve`, `tormenta`, `niebla`.
pub fn describir_codigo(codigo: i32, es_de_dia: bool) -> (&'static str, &'static str) {
    let despejado = if es_de_dia { "sol" } else { "luna" };
    match codigo {
        0 => ("Despejado", despejado),
        1 => ("Mayormente despejado", despejado),
        2 => ("Parcialmente nublado", "nubes"),
        3 => ("Nublado", "nubes"),
        45 => ("Niebla", "niebla"),
        48 => ("Niebla con escarcha", "niebla"),
        51 => ("Llovizna ligera", "lluvia"),
        53 => ("Llovizna moderada", "lluvia"),
        55 => ("Llovizna intensa", "lluvia"),
        56 | 57 => ("Llovizna helada", "lluvia"),
        61 => ("Lluvia ligera", "lluvia"),
        63 => ("Lluvia moderada", "lluvia"),
        65 => ("Lluvia fuerte", "lluvia"),
        66 | 67 => ("Lluvia helada", "lluvia"),
        71 => ("Nieve ligera", "nieve"),
        73 => ("Nieve moderada", "nieve"),
        75 => ("Nieve fuerte", "nieve"),
        77 => ("Granizo fino", "nieve"),
        80 => ("Chubascos ligeros", "lluvia"),
        81 => ("Chubascos moderados", "lluvia"),
        82 => ("Chubascos violentos", "lluvia"),
        85 => ("Chubascos de nieve", "nieve"),
        86 => ("Chubascos de nieve fuertes", "nieve"),
        95 => ("Tormenta", "tormenta"),
        96 | 99 => ("Tormenta con granizo", "tormenta"),
        _ => ("Desconocido", "nubes"),
    }
}

// --- Lectura de las respuestas de Open-Meteo ---

#[derive(Deserialize)]
struct RespPrevision {
    current: Option<RespActual>,
    daily: Option<RespDiario>,
    hourly: Option<RespHoras>,
}

#[derive(Deserialize)]
struct RespHoras {
    time: Vec<String>,
    #[serde(default)]
    temperature_2m: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability: Vec<Option<f64>>,
    #[serde(default)]
    weather_code: Vec<Option<i32>>,
}

#[derive(Deserialize)]
struct RespActual {
    #[serde(default)]
    time: Option<String>,
    temperature_2m: f64,
    apparent_temperature: f64,
    relative_humidity_2m: f64,
    weather_code: i32,
    wind_speed_10m: f64,
    is_day: i32,
    #[serde(default)]
    wind_direction_10m: Option<f64>,
    #[serde(default)]
    uv_index: Option<f64>,
}

#[derive(Deserialize)]
struct RespDiario {
    time: Vec<String>,
    weather_code: Vec<Option<i32>>,
    temperature_2m_max: Vec<Option<f64>>,
    temperature_2m_min: Vec<Option<f64>>,
    precipitation_probability_max: Vec<Option<f64>>,
    #[serde(default)]
    sunrise: Vec<Option<String>>,
    #[serde(default)]
    sunset: Vec<Option<String>>,
    #[serde(default)]
    uv_index_max: Vec<Option<f64>>,
}

/// Interpreta la respuesta JSON de `/v1/forecast`.
pub fn interpretar_prevision(json: &str) -> Result<(Actual, Vec<Dia>), String> {
    let r: RespPrevision =
        serde_json::from_str(json).map_err(|_| "Respuesta de previsión no válida".to_string())?;
    let c = r.current.ok_or("La previsión no incluye el tiempo actual")?;
    let es_de_dia = c.is_day != 0;
    let (texto, icono) = describir_codigo(c.weather_code, es_de_dia);
    let actual = Actual {
        temperatura: c.temperature_2m,
        sensacion: c.apparent_temperature,
        humedad: c.relative_humidity_2m,
        viento: c.wind_speed_10m,
        codigo: c.weather_code,
        es_de_dia,
        texto: texto.to_string(),
        icono: icono.to_string(),
        direccion_viento: c.wind_direction_10m,
        uv: c.uv_index,
    };
    let mut dias = Vec::new();
    if let Some(d) = r.daily {
        for (i, fecha) in d.time.iter().enumerate() {
            let codigo = d.weather_code.get(i).copied().flatten().unwrap_or(-1);
            let (texto, icono) = describir_codigo(codigo, true);
            dias.push(Dia {
                fecha: fecha.clone(),
                codigo,
                maxima: d.temperature_2m_max.get(i).copied().flatten(),
                minima: d.temperature_2m_min.get(i).copied().flatten(),
                prob_lluvia: d.precipitation_probability_max.get(i).copied().flatten(),
                texto: texto.to_string(),
                icono: icono.to_string(),
                salida_sol: d.sunrise.get(i).cloned().flatten(),
                puesta_sol: d.sunset.get(i).cloned().flatten(),
                uv_max: d.uv_index_max.get(i).copied().flatten(),
            });
        }
    }
    Ok((actual, dias))
}

/// Las próximas 48 horas desde la hora actual (`current.time`, local de la ciudad).
/// Sin `hourly` devuelve una lista vacía; sin hora actual parte del principio.
pub fn interpretar_horas(json: &str) -> Vec<Hora> {
    let Ok(r) = serde_json::from_str::<RespPrevision>(json) else {
        return vec![];
    };
    let Some(h) = r.hourly else {
        return vec![];
    };
    // «2026-11-16T12:15» -> «2026-11-16T12»: las horas se comparan como texto ISO
    let desde = r
        .current
        .and_then(|c| c.time)
        .map(|t| t.chars().take(13).collect::<String>())
        .unwrap_or_default();
    h.time
        .iter()
        .enumerate()
        .filter(|(_, t)| t.chars().take(13).collect::<String>() >= desde)
        .take(48)
        .map(|(i, t)| Hora {
            hora: t.clone(),
            temperatura: h.temperature_2m.get(i).copied().flatten(),
            prob_lluvia: h.precipitation_probability.get(i).copied().flatten(),
            codigo: h.weather_code.get(i).copied().flatten().unwrap_or(-1),
        })
        .collect()
}

#[derive(Deserialize)]
struct RespGeo {
    results: Option<Vec<ResultadoGeo>>,
}

#[derive(Deserialize)]
struct ResultadoGeo {
    name: String,
    country: Option<String>,
    admin1: Option<String>,
    latitude: f64,
    longitude: f64,
}

/// Interpreta la respuesta JSON del geocodificador.
pub fn interpretar_ciudades(json: &str) -> Result<Vec<Ciudad>, String> {
    let r: RespGeo =
        serde_json::from_str(json).map_err(|_| "Respuesta de búsqueda no válida".to_string())?;
    Ok(r.results
        .unwrap_or_default()
        .into_iter()
        .map(|c| Ciudad {
            nombre: match c.admin1.as_deref() {
                Some(a) if !a.is_empty() && a != c.name => format!("{}, {}", c.name, a),
                _ => c.name,
            },
            pais: c.country.unwrap_or_default(),
            latitud: (c.latitude * 10_000.0).round() / 10_000.0,
            longitud: (c.longitude * 10_000.0).round() / 10_000.0,
        })
        .collect())
}

// --- Red ---

fn cliente() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("netrunner-panel")
        .build()
        .map_err(|_| "No se pudo iniciar la conexión".to_string())
}

fn obtener(url: &str, consulta: &[(&str, String)]) -> Result<String, String> {
    let resp = cliente()?.get(url).query(consulta).send().map_err(|e| {
        if e.is_timeout() {
            "Tiempo de espera agotado".to_string()
        } else {
            "Sin conexión con el servicio del tiempo".to_string()
        }
    })?;
    if !resp.status().is_success() {
        return Err(format!("El servicio del tiempo respondió HTTP {}", resp.status().as_u16()));
    }
    resp.text().map_err(|_| "No se pudo leer la respuesta".to_string())
}

fn descargar_prevision(c: &Ciudad) -> Result<(Actual, Vec<Dia>, Vec<Hora>), String> {
    let json = obtener(
        "https://api.open-meteo.com/v1/forecast",
        &[
            ("latitude", c.latitud.to_string()),
            ("longitude", c.longitud.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day,wind_direction_10m,uv_index".into(),
            ),
            (
                "daily",
                "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset,uv_index_max".into(),
            ),
            ("hourly", "temperature_2m,precipitation_probability,weather_code".into()),
            ("timezone", "auto".into()),
            ("forecast_days", "14".into()),
        ],
    )?;
    let (actual, dias) = interpretar_prevision(&json)?;
    Ok((actual, dias, interpretar_horas(&json)))
}

#[tauri::command]
pub async fn buscar_ciudad(texto: String) -> Result<Vec<Ciudad>, String> {
    let texto = texto.trim().to_string();
    if texto.chars().count() < 2 {
        return Ok(vec![]);
    }
    tauri::async_runtime::spawn_blocking(move || {
        let json = obtener(
            "https://geocoding-api.open-meteo.com/v1/search",
            &[
                ("name", texto),
                ("count", "5".into()),
                ("language", "es".into()),
                ("format", "json".into()),
            ],
        )?;
        interpretar_ciudades(&json)
    })
    .await
    .map_err(|_| "Error interno en la búsqueda".to_string())?
}

// --- Caché y estado ---

fn ruta_cache(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("meteo.json"))
}

fn cargar_cache(app: &AppHandle) -> Option<EstadoMeteo> {
    let s = std::fs::read_to_string(ruta_cache(app)?).ok()?;
    serde_json::from_str(&s).ok()
}

fn guardar_cache(app: &AppHandle, e: &EstadoMeteo) {
    if let Some(r) = ruta_cache(app) {
        if let Some(dir) = r.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let limpio = EstadoMeteo { cargando: false, error: None, ..e.clone() };
        if let Ok(json) = serde_json::to_string(&limpio) {
            let _ = std::fs::write(r, json);
        }
    }
}

fn emitir(app: &AppHandle) {
    let e = app.state::<Meteo>().0.lock().unwrap().clone();
    let _ = app.emit("meteo://estado", e);
}

fn ciudad_configurada(app: &AppHandle) -> Option<Ciudad> {
    app.state::<Estado>().config.lock().unwrap().ciudad.clone()
}

/// Refresca el tiempo en segundo plano. Si ya hay un refresco en curso, no hace nada.
pub fn actualizar(app: &AppHandle) {
    let ciudad = ciudad_configurada(app);
    {
        let m = app.state::<Meteo>();
        let mut e = m.0.lock().unwrap();
        if e.cargando {
            return;
        }
        if ciudad != e.ciudad {
            // Ciudad nueva: los datos anteriores ya no sirven
            *e = EstadoMeteo { ciudad: ciudad.clone(), ..Default::default() };
        }
        if ciudad.is_none() {
            drop(e);
            if let Some(r) = ruta_cache(app) {
                let _ = std::fs::remove_file(r);
            }
            emitir(app);
            return;
        }
        e.cargando = true;
    }
    emitir(app);

    let app = app.clone();
    thread::spawn(move || {
        let c = ciudad.unwrap();
        let resultado = descargar_prevision(&c);
        {
            let m = app.state::<Meteo>();
            let mut e = m.0.lock().unwrap();
            e.cargando = false;
            match resultado {
                Ok((actual, dias, horas)) => {
                    e.actual = Some(actual);
                    e.dias = dias;
                    e.por_horas = horas;
                    e.actualizado = Some(Utc::now());
                    e.error = None;
                    guardar_cache(&app, &e);
                }
                Err(msg) => e.error = Some(msg),
            }
        }
        emitir(&app);
        // La ciudad cambió mientras se descargaba
        if ciudad_configurada(&app).as_ref() != Some(&c) {
            actualizar(&app);
        }
    });
}

#[tauri::command]
pub fn estado_meteo(m: State<Meteo>) -> EstadoMeteo {
    m.0.lock().unwrap().clone()
}

pub fn iniciar(app: &AppHandle) {
    let ciudad = ciudad_configurada(app);
    let mut inicial = EstadoMeteo { ciudad: ciudad.clone(), ..Default::default() };
    if ciudad.is_some() {
        if let Some(c) = cargar_cache(app).filter(|c| c.ciudad == ciudad) {
            inicial = c;
        }
    }
    app.manage(Meteo(Mutex::new(inicial)));

    let h = app.clone();
    thread::spawn(move || loop {
        actualizar(&h);
        thread::sleep(INTERVALO);
    });
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn describe_codigos_wmo() {
        assert_eq!(describir_codigo(0, true), ("Despejado", "sol"));
        assert_eq!(describir_codigo(0, false), ("Despejado", "luna"));
        assert_eq!(describir_codigo(3, true).1, "nubes");
        assert_eq!(describir_codigo(45, true).1, "niebla");
        assert_eq!(describir_codigo(63, false), ("Lluvia moderada", "lluvia"));
        assert_eq!(describir_codigo(75, true).1, "nieve");
        assert_eq!(describir_codigo(95, true), ("Tormenta", "tormenta"));
        assert_eq!(describir_codigo(1234, true).0, "Desconocido");
    }

    const PREVISION: &str = r#"{
      "latitude": 40.4, "longitude": -3.7, "timezone": "Europe/Madrid",
      "current": {"time": "2026-11-16T12:00", "interval": 900, "temperature_2m": 11.4,
        "apparent_temperature": 9.8, "relative_humidity_2m": 72, "weather_code": 61,
        "wind_speed_10m": 14.2, "is_day": 1, "wind_direction_10m": 225, "uv_index": 3.5},
      "daily": {"time": ["2026-11-16","2026-11-17"], "weather_code": [61, 0],
        "temperature_2m_max": [13.1, 15.0], "temperature_2m_min": [6.2, null],
        "precipitation_probability_max": [80, 5],
        "sunrise": ["2026-11-16T07:40", "2026-11-17T07:41"], "sunset": ["2026-11-16T17:50", null],
        "uv_index_max": [2.1, 2.3]}
    }"#;

    #[test]
    fn lee_la_prevision() {
        let (a, dias) = interpretar_prevision(PREVISION).unwrap();
        assert_eq!(a.temperatura, 11.4);
        assert_eq!(a.humedad, 72.0);
        assert!(a.es_de_dia);
        assert_eq!((a.texto.as_str(), a.icono.as_str()), ("Lluvia ligera", "lluvia"));
        assert_eq!(dias.len(), 2);
        assert_eq!(dias[0].prob_lluvia, Some(80.0));
        assert_eq!(dias[1].minima, None);
        assert_eq!(dias[1].icono, "sol");
        assert_eq!(a.direccion_viento, Some(225.0));
        assert_eq!(a.uv, Some(3.5));
        assert_eq!(dias[0].salida_sol.as_deref(), Some("2026-11-16T07:40"));
        assert_eq!(dias[1].puesta_sol, None);
        assert_eq!(dias[1].uv_max, Some(2.3));
        assert!(interpretar_prevision("{}").is_err());
    }

    #[test]
    fn la_prevision_sin_campos_nuevos_sigue_valiendo() {
        let json = r#"{"current": {"temperature_2m": 1.0, "apparent_temperature": 0.0,
          "relative_humidity_2m": 50, "weather_code": 0, "wind_speed_10m": 2.0, "is_day": 0},
          "daily": {"time": ["2026-11-16"], "weather_code": [0], "temperature_2m_max": [3.0],
          "temperature_2m_min": [-1.0], "precipitation_probability_max": [0]}}"#;
        let (a, dias) = interpretar_prevision(json).unwrap();
        assert_eq!((a.direccion_viento, a.uv), (None, None));
        assert_eq!((dias[0].salida_sol.clone(), dias[0].puesta_sol.clone(), dias[0].uv_max), (None, None, None));
        // Un Dia guardado en la caché antigua también se lee
        let d: Dia = serde_json::from_str(r#"{"fecha":"2026-11-16","codigo":0,"maxima":1.0,"minima":0.0,"prob_lluvia":null,"texto":"x","icono":"sol"}"#).unwrap();
        assert!(d.salida_sol.is_none() && d.uv_max.is_none());
    }

    #[test]
    fn lee_las_proximas_48_horas() {
        // 96 horas desde el 16 a las 00:00; el actual es las 12:15
        let horas: Vec<String> = (0..96)
            .map(|i| format!("\"2026-11-{:02}T{:02}:00\"", 16 + i / 24, i % 24))
            .collect();
        let temps: Vec<String> = (0..96).map(|i| if i == 13 { "null".into() } else { format!("{}.5", i) }).collect();
        let json = format!(
            r#"{{"current": {{"time": "2026-11-16T12:15", "temperature_2m": 11.0,
              "apparent_temperature": 9.0, "relative_humidity_2m": 70, "weather_code": 3,
              "wind_speed_10m": 5.0, "is_day": 1}},
              "hourly": {{"time": [{}], "temperature_2m": [{}],
              "precipitation_probability": [{}], "weather_code": [{}]}}}}"#,
            horas.join(","),
            temps.join(","),
            vec!["40"; 96].join(","),
            vec!["61"; 96].join(","),
        );
        let h = interpretar_horas(&json);
        assert_eq!(h.len(), 48);
        assert_eq!(h[0].hora, "2026-11-16T12:00");
        assert_eq!(h[0].temperatura, Some(12.5));
        assert_eq!(h[1].temperatura, None);
        assert_eq!(h[0].prob_lluvia, Some(40.0));
        assert_eq!(h[0].codigo, 61);
        assert_eq!(h[23].hora, "2026-11-17T11:00");
        assert_eq!(h[47].hora, "2026-11-18T11:00");
        // Sin `hourly` (o con la caché antigua) no hay horas
        assert!(interpretar_horas(PREVISION).is_empty());
        // La caché antigua, sin `por_horas`, sigue leyéndose
        let viejo: EstadoMeteo = serde_json::from_str(r#"{"ciudad":null,"actual":null,"dias":[],"actualizado":null,"cargando":false,"error":null}"#).unwrap();
        assert!(viejo.por_horas.is_empty());
    }

    #[test]
    fn lee_la_busqueda_de_ciudades() {
        let json = r#"{"results":[
          {"id":1,"name":"Madrid","latitude":40.4165,"longitude":-3.70256,"country":"España","admin1":"Comunidad de Madrid"},
          {"id":2,"name":"Madrid","latitude":37.0,"longitude":-80.0,"country":"Estados Unidos"}],
          "generationtime_ms":0.5}"#;
        let c = interpretar_ciudades(json).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].nombre, "Madrid, Comunidad de Madrid");
        assert_eq!(c[0].pais, "España");
        assert_eq!(c[1].nombre, "Madrid");
        assert!(interpretar_ciudades(r#"{"generationtime_ms":0.1}"#).unwrap().is_empty());
    }
}
