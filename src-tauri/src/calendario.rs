//! Módulo CALENDARIO: agenda de solo lectura a partir de una URL ICS (Outlook).
//!
//! Descarga el ICS (reqwest bloqueante, timeout de 15 s), lo interpreta con `ical`,
//! expande recurrencias con `rrule` y devuelve los eventos de los próximos ~6 meses en
//! hora local. La URL es privada: jamás se escribe en logs ni en mensajes de error.
//! El último resultado bueno se guarda en `app_data_dir()/calendario.json`.

use std::collections::HashSet;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration as Espera, Instant};

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rrule::{RRule, Unvalidated};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Listener, Manager, State};

use crate::estado::Estado;

/// Unos 6 meses. Cada RRULE se expande solo dentro de la ventana (`after`/`before`) y con
/// un tope de `MAX_OCURRENCIAS`, así que el coste no depende de cuántos años dura la serie.
const DIAS_VENTANA: i64 = 183;
const MAX_DESCRIPCION: usize = 200;
const INTERVALO_SYNC: Espera = Espera::from_secs(600);
const EDAD_MAX_AL_MOSTRAR: Espera = Espera::from_secs(30);
const MAX_OCURRENCIAS: u16 = 1000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Evento {
    /// Estable por ocurrencia: `uid|inicio UTC` (en excepciones, el RECURRENCE-ID original).
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub uid: String,
    pub titulo: String,
    pub inicio: DateTime<FixedOffset>,
    /// Para eventos de día completo, el fin es exclusivo (00:00 del día siguiente).
    pub fin: DateTime<FixedOffset>,
    pub todo_el_dia: bool,
    pub ubicacion: Option<String>,
    #[serde(default)]
    pub categorias: Vec<String>,
    /// Primeras ~200 letras de DESCRIPTION, sin escapes.
    #[serde(default)]
    pub descripcion: Option<String>,
    /// Ocurrencia de una serie (RRULE) o excepción de una serie. No se puede borrar desde
    /// el panel: borrar una ocurrencia o toda la serie es una decisión que se toma en Outlook.
    #[serde(default)]
    pub recurrente: bool,
    /// Creado desde el panel y aún no publicado por el ICS: vive en la cola de `pendientes`.
    #[serde(default)]
    pub pendiente: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EstadoCalendario {
    /// Lo que se muestra: el ICS más lo que el usuario creó (y menos lo que borró) desde el
    /// panel y Outlook aún no ha procesado.
    pub eventos: Vec<Evento>,
    pub ultima_sync: Option<DateTime<Utc>>,
    pub sincronizando: bool,
    pub error: Option<String>,
    /// Creaciones y borrados que esperan a que Outlook esté abierto.
    #[serde(default)]
    pub pendientes: usize,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    eventos: Vec<Evento>,
    ultima_sync: DateTime<Utc>,
}

pub struct Calendario {
    estado: Mutex<EstadoCalendario>,
    ultimo_intento: Mutex<Option<Instant>>,
    /// Eventos tal como los dice el último ICS (sin la cola de pendientes). Orden de bloqueo:
    /// siempre `estado` antes que `crudos`.
    crudos: Mutex<Vec<Evento>>,
}

// ---------------------------------------------------------------------------
// Zonas horarias
// ---------------------------------------------------------------------------

/// Tabla de zonas de Windows (las que usa Outlook) a nombres IANA. No hay un crate
/// mantenido y pequeño que lo haga, así que se cubren las zonas más comunes de
/// Europa y América (y alguna de Asia/Oceanía).
const ZONAS_WINDOWS: &[(&str, &str)] = &[
    ("Romance Standard Time", "Europe/Paris"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Azores Standard Time", "Atlantic/Azores"),
    ("Cape Verde Standard Time", "Atlantic/Cape_Verde"),
    ("Eastern Standard Time", "America/New_York"),
    ("US Eastern Standard Time", "America/Indiana/Indianapolis"),
    ("Central Standard Time", "America/Chicago"),
    ("Central Standard Time (Mexico)", "America/Mexico_City"),
    ("Mountain Standard Time", "America/Denver"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Pacific Standard Time (Mexico)", "America/Tijuana"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("Canada Central Standard Time", "America/Regina"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("SA Western Standard Time", "America/La_Paz"),
    ("SA Eastern Standard Time", "America/Cayenne"),
    ("Venezuela Standard Time", "America/Caracas"),
    ("Pacific SA Standard Time", "America/Santiago"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("Argentina Standard Time", "America/Argentina/Buenos_Aires"),
    ("Montevideo Standard Time", "America/Montevideo"),
    ("Paraguay Standard Time", "America/Asuncion"),
    ("Central America Standard Time", "America/Guatemala"),
    ("Cuba Standard Time", "America/Havana"),
    ("UTC", "UTC"),
    ("Coordinated Universal Time", "UTC"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("Egypt Standard Time", "Africa/Cairo"),
    ("W. Central Africa Standard Time", "Africa/Lagos"),
    ("Morocco Standard Time", "Africa/Casablanca"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("Arab Standard Time", "Asia/Riyadh"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("India Standard Time", "Asia/Kolkata"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
];

/// Traduce un nombre de zona (IANA o de Windows) a `chrono_tz::Tz`.
pub fn zona_desde_nombre(nombre: &str) -> Option<Tz> {
    let n = nombre.trim().trim_matches('"');
    if let Ok(z) = n.parse::<Tz>() {
        return Some(z);
    }
    ZONAS_WINDOWS
        .iter()
        .find(|(w, _)| w.eq_ignore_ascii_case(n))
        .and_then(|(_, iana)| iana.parse::<Tz>().ok())
}

/// Zona local del equipo; si no se puede averiguar, UTC.
pub fn zona_local() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|n| n.parse::<Tz>().ok())
        .unwrap_or(Tz::UTC)
}

/// Resuelve una hora local ambigua o inexistente (cambios de hora) sin fallar.
fn resolver(zona: Tz, naive: NaiveDateTime) -> DateTime<Tz> {
    zona.from_local_datetime(&naive)
        .earliest()
        .or_else(|| zona.from_local_datetime(&(naive + Duration::hours(1))).earliest())
        .unwrap_or_else(|| Tz::UTC.from_utc_datetime(&naive).with_timezone(&zona))
}

fn medianoche(zona: Tz, dia: NaiveDate) -> DateTime<Tz> {
    resolver(zona, dia.and_hms_opt(0, 0, 0).unwrap())
}

// ---------------------------------------------------------------------------
// Lectura del ICS
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Fecha {
    Dia(NaiveDate),
    Instante(DateTime<Tz>),
}

#[derive(Default, Debug)]
struct Crudo {
    uid: String,
    titulo: String,
    ubicacion: Option<String>,
    categorias: Vec<String>,
    descripcion: Option<String>,
    cancelado: bool,
    inicio: Option<Fecha>,
    fin: Option<Fecha>,
    duracion: Option<Duration>,
    rrule: Option<String>,
    exdates: Vec<Fecha>,
    recurrence_id: Option<Fecha>,
}

fn parametro(p: &ical::property::Property, nombre: &str) -> Option<String> {
    p.params
        .as_ref()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(nombre))
        .and_then(|(_, v)| v.first())
        .map(|v| v.trim().trim_matches('"').to_string())
}

fn parsear_valor(valor: &str, tzid: Option<&str>, local: Tz) -> Option<Fecha> {
    let v = valor.trim();
    if v.len() == 8 && v.bytes().all(|b| b.is_ascii_digit()) {
        return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(Fecha::Dia);
    }
    let (sin_z, utc) = match v.strip_suffix(['Z', 'z']) {
        Some(s) => (s, true),
        None => (v, false),
    };
    let naive = NaiveDateTime::parse_from_str(sin_z, "%Y%m%dT%H%M%S").ok()?;
    let instante = if utc {
        Tz::UTC.from_utc_datetime(&naive)
    } else {
        // TZID desconocido o ausente (hora flotante): zona local
        let zona = tzid.and_then(zona_desde_nombre).unwrap_or(local);
        resolver(zona, naive)
    };
    Some(Fecha::Instante(instante))
}

fn parsear_fechas(p: &ical::property::Property, local: Tz) -> Vec<Fecha> {
    let tzid = parametro(p, "TZID");
    p.value
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|v| parsear_valor(v, tzid.as_deref(), local))
        .collect()
}

/// Duración ICS: `P1DT2H30M`, `PT45M`, `P1W`...
fn parsear_duracion(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (negativa, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let s = s.strip_prefix('P')?;
    let mut total = Duration::zero();
    let mut num = String::new();
    let mut en_hora = false;
    for c in s.chars() {
        match c {
            'T' => en_hora = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total = total
                    + match (c, en_hora) {
                        ('W', _) => Duration::weeks(n),
                        ('D', _) => Duration::days(n),
                        ('H', true) => Duration::hours(n),
                        ('M', true) => Duration::minutes(n),
                        ('S', true) => Duration::seconds(n),
                        _ => return None,
                    };
            }
        }
    }
    Some(if negativa { -total } else { total })
}

fn desescapar(s: &str) -> String {
    let mut fuera = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') | Some('N') => fuera.push('\n'),
                Some(o) => fuera.push(o),
                None => {}
            }
        } else {
            fuera.push(c);
        }
    }
    fuera.trim().to_string()
}

/// Separa un valor CATEGORIES por comas sin escapar; `\,` es una coma dentro del nombre.
fn separar_categorias(valor: &str) -> Vec<String> {
    let mut resultado = Vec::new();
    let mut actual = String::new();
    let mut it = valor.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                if let Some(o) = it.next() {
                    actual.push(o);
                }
            }
            ',' => resultado.push(std::mem::take(&mut actual)),
            _ => actual.push(c),
        }
    }
    resultado.push(actual);
    resultado
        .into_iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

fn crudo_desde(ev: &ical::parser::ical::component::IcalEvent, local: Tz) -> Crudo {
    let mut c = Crudo::default();
    for p in &ev.properties {
        let valor = p.value.as_deref().unwrap_or("");
        match p.name.to_ascii_uppercase().as_str() {
            "UID" => c.uid = valor.trim().to_string(),
            "SUMMARY" => c.titulo = desescapar(valor),
            "LOCATION" => {
                let u = desescapar(valor);
                if !u.is_empty() {
                    c.ubicacion = Some(u);
                }
            }
            "CATEGORIES" => {
                for cat in separar_categorias(valor) {
                    if !c.categorias.contains(&cat) {
                        c.categorias.push(cat);
                    }
                }
            }
            "DESCRIPTION" => {
                let d = desescapar(valor);
                if !d.is_empty() {
                    c.descripcion = Some(d.chars().take(MAX_DESCRIPCION).collect::<String>().trim_end().to_string());
                }
            }
            "STATUS" => c.cancelado = valor.trim().eq_ignore_ascii_case("CANCELLED"),
            "DTSTART" => c.inicio = parsear_fechas(p, local).into_iter().next(),
            "DTEND" => c.fin = parsear_fechas(p, local).into_iter().next(),
            "DURATION" => c.duracion = parsear_duracion(valor),
            "RRULE" => c.rrule = Some(valor.trim().to_string()),
            "EXDATE" => c.exdates.extend(parsear_fechas(p, local)),
            "RECURRENCE-ID" => c.recurrence_id = parsear_fechas(p, local).into_iter().next(),
            _ => {}
        }
    }
    if c.titulo.is_empty() {
        c.titulo = "(sin título)".to_string();
    }
    c
}

/// Instante (en segundos UTC) que identifica una fecha, para casar excepciones.
fn clave(f: Fecha, local: Tz) -> i64 {
    match f {
        Fecha::Dia(d) => medianoche(local, d).timestamp(),
        Fecha::Instante(i) => i.timestamp(),
    }
}

/// UNTIL en fecha sola o sin `Z` se pasa a UTC para que `rrule` lo acepte.
fn normalizar_rrule(r: &str) -> String {
    r.split(';')
        .filter(|p| !p.trim().is_empty())
        .map(|p| match p.trim().strip_prefix("UNTIL=").or_else(|| p.trim().strip_prefix("until=")) {
            Some(v) if v.len() == 8 => format!("UNTIL={v}T235959Z"),
            Some(v) if !v.ends_with(['Z', 'z']) => format!("UNTIL={v}Z"),
            _ => p.trim().to_string(),
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// (inicio, fin, todo el día, instante UTC en segundos que identifica la ocurrencia).
type Ocurrencia = (DateTime<Tz>, DateTime<Tz>, bool, i64);

/// Ocurrencias de un evento crudo que tocan la ventana.
fn ocurrencias(
    c: &Crudo,
    ini_v: DateTime<Tz>,
    fin_v: DateTime<Tz>,
    local: Tz,
    excepciones: &HashSet<(String, i64)>,
) -> Vec<Ocurrencia> {
    let Some(inicio) = c.inicio else { return vec![] };
    let todo_dia = matches!(inicio, Fecha::Dia(_));

    // Inicio base y duración
    let (inicio0, dias, dur) = match inicio {
        Fecha::Dia(d) => {
            let n = match c.fin {
                Some(Fecha::Dia(f)) => (f - d).num_days().max(1),
                _ => 1,
            };
            (medianoche(local, d), n, Duration::days(n))
        }
        Fecha::Instante(i) => {
            let dur = match c.fin {
                Some(Fecha::Instante(f)) => (f - i).max(Duration::zero()),
                _ => c.duracion.unwrap_or_else(Duration::zero),
            };
            (i, 0, dur)
        }
    };

    // Una excepción conserva el id de la ocurrencia que sustituye (su RECURRENCE-ID)
    let id_excepcion = c.recurrence_id.map(|r| clave(r, local));
    let construir = |s: DateTime<Tz>| -> Ocurrencia {
        let (ini, fin, dia) = if todo_dia {
            let d = s.with_timezone(&local).date_naive();
            (medianoche(local, d), medianoche(local, d + Duration::days(dias)), true)
        } else {
            (s, s + dur, false)
        };
        (ini, fin, dia, id_excepcion.unwrap_or_else(|| ini.timestamp()))
    };
    let toca = |(s, f, _, _): &Ocurrencia| {
        *s < fin_v && (*f > ini_v || *s >= ini_v)
    };

    let a_rrule = |dt: DateTime<Tz>| dt.with_timezone(&rrule::Tz::from(dt.timezone()));

    // Sin recurrencia (o excepción con RECURRENCE-ID): una sola ocurrencia
    let Some(regla) = c.rrule.as_ref().filter(|_| c.recurrence_id.is_none()) else {
        let o = construir(inicio0);
        return if toca(&o) { vec![o] } else { vec![] };
    };

    let Some(mut conjunto) = normalizar_rrule(regla)
        .parse::<RRule<Unvalidated>>()
        .ok()
        .and_then(|r| r.build(a_rrule(inicio0)).ok())
    else {
        // Regla ilegible: se muestra solo la ocurrencia original
        let o = construir(inicio0);
        return if toca(&o) { vec![o] } else { vec![] };
    };
    for ex in &c.exdates {
        let dt = match *ex {
            Fecha::Dia(d) => medianoche(local, d),
            Fecha::Instante(i) => i,
        };
        conjunto = conjunto.exdate(a_rrule(dt));
    }
    let resultado = conjunto
        .after(a_rrule(ini_v - dur - Duration::hours(1)))
        .before(a_rrule(fin_v))
        .all(MAX_OCURRENCIAS);

    resultado
        .dates
        .into_iter()
        .filter(|d| !excepciones.contains(&(c.uid.clone(), d.timestamp())))
        .map(|d| construir(d.with_timezone(&inicio0.timezone())))
        .filter(toca)
        .collect()
}

/// Id estable de una ocurrencia: `uid|AAAAMMDDTHHMMSSZ` (inicio original en UTC).
fn id_ocurrencia(uid: &str, instante: i64) -> String {
    let t = Utc.timestamp_opt(instante, 0).single().unwrap_or_default();
    format!("{uid}|{}", t.format("%Y%m%dT%H%M%SZ"))
}

/// Interpreta un ICS y devuelve los eventos de `[hoy 00:00, hoy + 183 días)` en la zona
/// `local`, ordenados. Función pura: no toca red ni disco.
pub fn interpretar(ics: &str, ahora: DateTime<Utc>, local: Tz) -> Result<Vec<Evento>, String> {
    let hoy = ahora.with_timezone(&local).date_naive();
    let ini_v = medianoche(local, hoy);
    let fin_v = medianoche(local, hoy + Duration::days(DIAS_VENTANA));

    let mut crudos: Vec<Crudo> = Vec::new();
    let mut calendarios = 0;
    for cal in ical::IcalParser::new(BufReader::new(ics.as_bytes())) {
        let cal = cal.map_err(|_| "El archivo no es un calendario ICS válido".to_string())?;
        calendarios += 1;
        crudos.extend(cal.events.iter().map(|e| crudo_desde(e, local)));
    }
    if calendarios == 0 {
        return Err("El archivo no es un calendario ICS válido".to_string());
    }

    // Excepciones: sustituyen a la ocurrencia original (incluidas las canceladas)
    let excepciones: HashSet<(String, i64)> = crudos
        .iter()
        .filter_map(|c| c.recurrence_id.map(|r| (c.uid.clone(), clave(r, local))))
        .collect();

    let mut eventos: Vec<Evento> = Vec::new();
    for c in crudos.iter().filter(|c| !c.cancelado) {
        for (s, f, dia, instante) in ocurrencias(c, ini_v, fin_v, local, &excepciones) {
            eventos.push(Evento {
                id: id_ocurrencia(&c.uid, instante),
                uid: c.uid.clone(),
                titulo: c.titulo.clone(),
                inicio: s.with_timezone(&local).fixed_offset(),
                fin: f.with_timezone(&local).fixed_offset(),
                todo_el_dia: dia,
                ubicacion: c.ubicacion.clone(),
                categorias: c.categorias.clone(),
                descripcion: c.descripcion.clone(),
                recurrente: c.rrule.is_some() || c.recurrence_id.is_some(),
                pendiente: false,
            });
        }
    }
    // Los de día completo primero dentro de su día
    eventos.sort_by(|a, b| {
        a.inicio
            .cmp(&b.inicio)
            .then(b.todo_el_dia.cmp(&a.todo_el_dia))
            .then(a.titulo.cmp(&b.titulo))
    });
    Ok(eventos)
}

// ---------------------------------------------------------------------------
// Red, caché y sincronización
// ---------------------------------------------------------------------------

/// `webcal://` es `https://`.
fn normalizar_url(url: &str) -> String {
    let u = url.trim();
    let minus = u.to_ascii_lowercase();
    if minus.starts_with("webcal://") {
        format!("https://{}", &u[9..])
    } else if minus.starts_with("webcals://") {
        format!("https://{}", &u[10..])
    } else {
        u.to_string()
    }
}

/// Descarga el ICS. Los errores son textos fijos: nunca incluyen la URL.
fn descargar(url: &str) -> Result<String, String> {
    let cliente = reqwest::blocking::Client::builder()
        .timeout(Espera::from_secs(15))
        .user_agent("netrunner-panel")
        .build()
        .map_err(|_| "No se pudo iniciar la conexión".to_string())?;
    let resp = cliente.get(normalizar_url(url)).send().map_err(|e| {
        if e.is_builder() {
            "La URL del calendario no es válida".to_string()
        } else if e.is_timeout() {
            "Tiempo de espera agotado".to_string()
        } else if e.is_connect() {
            "Sin conexión con el servidor".to_string()
        } else {
            "No se pudo descargar el calendario".to_string()
        }
    })?;
    if !resp.status().is_success() {
        return Err(format!("El servidor respondió HTTP {}", resp.status().as_u16()));
    }
    let texto = resp
        .text()
        .map_err(|_| "No se pudo leer la respuesta".to_string())?;
    if !texto.contains("BEGIN:VCALENDAR") {
        return Err("La URL no devuelve un calendario ICS".to_string());
    }
    Ok(texto)
}

fn ruta_cache(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("calendario.json"))
}

fn cargar_cache(app: &AppHandle) -> Option<Cache> {
    let s = std::fs::read_to_string(ruta_cache(app)?).ok()?;
    serde_json::from_str(&s).ok()
}

fn guardar_cache(app: &AppHandle, cache: &Cache) {
    if let Some(r) = ruta_cache(app) {
        if let Some(dir) = r.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string(cache) {
            let _ = std::fs::write(r, json);
        }
    }
}

fn borrar_cache(app: &AppHandle) {
    if let Some(r) = ruta_cache(app) {
        let _ = std::fs::remove_file(r);
    }
}

fn emitir(app: &AppHandle) {
    let est = app.state::<Calendario>().estado.lock().unwrap().clone();
    let _ = app.emit("calendario://estado", est);
}

fn url_configurada(app: &AppHandle) -> Option<String> {
    app.state::<Estado>()
        .config
        .lock()
        .unwrap()
        .ics_url
        .clone()
        .filter(|u| !u.trim().is_empty())
}

/// Lanza una sincronización en segundo plano. Si ya hay una en curso, no hace nada.
pub fn sincronizar(app: &AppHandle) {
    let cal = app.state::<Calendario>();
    *cal.ultimo_intento.lock().unwrap() = Some(Instant::now());
    let url = url_configurada(app);
    {
        let mut est = cal.estado.lock().unwrap();
        if est.sincronizando {
            return;
        }
        if url.is_none() {
            *est = EstadoCalendario::default();
            cal.crudos.lock().unwrap().clear();
            drop(est);
            borrar_cache(app);
            emitir(app);
            return;
        }
        est.sincronizando = true;
    }
    emitir(app);

    let app = app.clone();
    thread::spawn(move || {
        let url = url.unwrap();
        let resultado = descargar(&url)
            .and_then(|t| interpretar(&t, Utc::now(), zona_local()));
        let mut crudos_nuevos: Option<Vec<Evento>> = None;
        {
            let cal = app.state::<Calendario>();
            let mut est = cal.estado.lock().unwrap();
            est.sincronizando = false;
            match resultado {
                Ok(eventos) => {
                    let ahora = Utc::now();
                    est.ultima_sync = Some(ahora);
                    est.error = None;
                    guardar_cache(&app, &Cache { eventos: eventos.clone(), ultima_sync: ahora });
                    *cal.crudos.lock().unwrap() = eventos.clone();
                    crudos_nuevos = Some(eventos);
                }
                // Se conservan los eventos de la caché
                Err(e) => est.error = Some(e),
            }
        }
        // Las creaciones y borrados que el ICS ya refleja salen de la cola; después se
        // recompone la agenda (ICS + cola) y se emite
        if let Some(crudos) = &crudos_nuevos {
            crate::pendientes::limpiar_con_ics(&app, crudos);
        }
        refrescar_vista(&app);
        // Las cuentas atrás se ponen al día con los eventos recién leídos
        if crudos_nuevos.is_some() {
            let visibles = app.state::<Calendario>().estado.lock().unwrap().eventos.clone();
            crate::cuentas::reconciliar(&app, &visibles);
        }
        // La URL cambió mientras se sincronizaba: se repite con la nueva
        if url_configurada(&app).as_deref() != Some(url.as_str()) {
            sincronizar(&app);
        }
    });
}

/// Recompone lo que se muestra (último ICS + cola de pendientes) y lo emite. Se llama tras cada
/// sincronización y cada vez que cambia la cola.
pub fn refrescar_vista(app: &AppHandle) {
    let Some(cal) = app.try_state::<Calendario>() else { return };
    let cola = crate::pendientes::instantanea(app);
    {
        let mut est = cal.estado.lock().unwrap();
        let crudos = cal.crudos.lock().unwrap();
        est.eventos = crate::pendientes::superponer(&crudos, &cola);
        est.pendientes = crate::pendientes::sin_enviar(&cola);
    }
    emitir(app);
}

#[tauri::command]
pub fn estado_calendario(cal: State<Calendario>) -> EstadoCalendario {
    cal.estado.lock().unwrap().clone()
}

#[tauri::command]
pub fn sincronizar_calendario(app: AppHandle) {
    sincronizar(&app);
}

pub fn iniciar(app: &AppHandle) {
    let mut inicial = EstadoCalendario::default();
    let mut crudos: Vec<Evento> = Vec::new();
    if url_configurada(app).is_some() {
        if let Some(c) = cargar_cache(app) {
            crudos = c.eventos;
            inicial.ultima_sync = Some(c.ultima_sync);
        }
    }
    let cola = crate::pendientes::instantanea(app);
    inicial.eventos = crate::pendientes::superponer(&crudos, &cola);
    inicial.pendientes = crate::pendientes::sin_enviar(&cola);
    app.manage(Calendario {
        estado: Mutex::new(inicial),
        ultimo_intento: Mutex::new(None),
        crudos: Mutex::new(crudos),
    });

    // Al mostrar el panel, sincroniza si la última tiene más de 30 s
    let h = app.clone();
    app.listen("panel://mostrar", move |_| {
        let vieja = h
            .state::<Calendario>()
            .ultimo_intento
            .lock()
            .unwrap()
            .map_or(true, |t| t.elapsed() > EDAD_MAX_AL_MOSTRAR);
        if vieja && url_configurada(&h).is_some() {
            sincronizar(&h);
        }
    });

    // Sincronización periódica (la primera, nada más arrancar)
    let h = app.clone();
    thread::spawn(move || loop {
        if url_configurada(&h).is_some() {
            sincronizar(&h);
        }
        thread::sleep(INTERVALO_SYNC);
    });
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const MADRID: Tz = Tz::Europe__Madrid;

    fn ahora() -> DateTime<Utc> {
        // Lunes 16 nov 2026, 10:00 UTC (11:00 en Madrid)
        Utc.with_ymd_and_hms(2026, 11, 16, 10, 0, 0).unwrap()
    }

    fn ics(cuerpo: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Prueba//ES\r\n{cuerpo}END:VCALENDAR\r\n")
    }

    fn hora(e: &Evento) -> String {
        e.inicio.format("%Y-%m-%d %H:%M").to_string()
    }

    /// Los tests de recurrencias miran solo hasta el 30 de nov (la ventana antigua de 14 días).
    fn hasta_30_nov(ev: Vec<Evento>) -> Vec<Evento> {
        ev.into_iter().filter(|e| hora(e).as_str() < "2026-12-01").collect()
    }

    #[test]
    fn evento_simple() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:a1\r\nSUMMARY:Reunión\\, equipo\r\nLOCATION:Sala 3\r\nDTSTART:20261117T080000Z\r\nDTEND:20261117T090000Z\r\nEND:VEVENT\r\n",
        );
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].titulo, "Reunión, equipo");
        assert_eq!(ev[0].ubicacion.as_deref(), Some("Sala 3"));
        assert_eq!(hora(&ev[0]), "2026-11-17 09:00"); // UTC+1 en invierno
        assert!(!ev[0].todo_el_dia);
        assert_eq!((ev[0].fin - ev[0].inicio).num_minutes(), 60);
    }

    #[test]
    fn dia_completo() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:d1\r\nSUMMARY:Festivo\r\nDTSTART;VALUE=DATE:20261118\r\nDTEND;VALUE=DATE:20261119\r\nEND:VEVENT\r\n",
        );
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(ev[0].todo_el_dia);
        assert_eq!(hora(&ev[0]), "2026-11-18 00:00");
        assert_eq!(hora(&Evento { inicio: ev[0].fin, ..ev[0].clone() }), "2026-11-19 00:00");
    }

    #[test]
    fn semanal_con_rrule_y_exdate() {
        // Martes a las 09:00 de Madrid; el del 24 de noviembre se excluye
        let s = ics(
            "BEGIN:VEVENT\r\nUID:w1\r\nSUMMARY:Semanal\r\nDTSTART;TZID=Europe/Madrid:20261103T090000\r\nDTEND;TZID=Europe/Madrid:20261103T093000\r\nRRULE:FREQ=WEEKLY;BYDAY=TU\r\nEXDATE;TZID=Europe/Madrid:20261124T090000\r\nEND:VEVENT\r\n",
        );
        let ev = hasta_30_nov(interpretar(&s, ahora(), MADRID).unwrap());
        let horas: Vec<String> = ev.iter().map(hora).collect();
        assert_eq!(horas, vec!["2026-11-17 09:00"]);
    }

    #[test]
    fn semanal_sin_exdate_da_dos() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:w2\r\nSUMMARY:Semanal\r\nDTSTART;TZID=Europe/Madrid:20261103T090000\r\nDTEND;TZID=Europe/Madrid:20261103T093000\r\nRRULE:FREQ=WEEKLY;BYDAY=TU\r\nEND:VEVENT\r\n",
        );
        let ev = hasta_30_nov(interpretar(&s, ahora(), MADRID).unwrap());
        let horas: Vec<String> = ev.iter().map(hora).collect();
        assert_eq!(horas, vec!["2026-11-17 09:00", "2026-11-24 09:00"]);
    }

    #[test]
    fn excepcion_con_recurrence_id() {
        // El del día 24 se mueve al miércoles 25 a las 15:00
        let s = ics(
            "BEGIN:VEVENT\r\nUID:x1\r\nSUMMARY:Serie\r\nDTSTART;TZID=Europe/Madrid:20261103T090000\r\nDTEND;TZID=Europe/Madrid:20261103T100000\r\nRRULE:FREQ=WEEKLY;BYDAY=TU\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:x1\r\nSUMMARY:Serie (movida)\r\nRECURRENCE-ID;TZID=Europe/Madrid:20261124T090000\r\nDTSTART;TZID=Europe/Madrid:20261125T150000\r\nDTEND;TZID=Europe/Madrid:20261125T160000\r\nEND:VEVENT\r\n",
        );
        let ev = hasta_30_nov(interpretar(&s, ahora(), MADRID).unwrap());
        let v: Vec<(String, String)> = ev.iter().map(|e| (hora(e), e.titulo.clone())).collect();
        assert_eq!(
            v,
            vec![
                ("2026-11-17 09:00".to_string(), "Serie".to_string()),
                ("2026-11-25 15:00".to_string(), "Serie (movida)".to_string()),
            ]
        );
    }

    #[test]
    fn zona_de_windows_romance() {
        assert_eq!(zona_desde_nombre("Romance Standard Time"), Some(Tz::Europe__Paris));
        assert_eq!(zona_desde_nombre("Zona Inventada"), None);
        let s = ics(
            "BEGIN:VEVENT\r\nUID:t1\r\nSUMMARY:Windows\r\nDTSTART;TZID=Romance Standard Time:20261117T100000\r\nDTEND;TZID=Romance Standard Time:20261117T110000\r\nEND:VEVENT\r\n",
        );
        // Con zona local Londres, 10:00 de Romance (UTC+1) son las 09:00 locales
        let ev = interpretar(&s, ahora(), Tz::Europe__London).unwrap();
        assert_eq!(hora(&ev[0]), "2026-11-17 09:00");
    }

    #[test]
    fn zona_desconocida_usa_la_local() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:t2\r\nSUMMARY:Rara\r\nDTSTART;TZID=Zona Inventada:20261117T100000\r\nEND:VEVENT\r\n",
        );
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert_eq!(hora(&ev[0]), "2026-11-17 10:00");
    }

    #[test]
    fn evento_cancelado() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:c1\r\nSUMMARY:Cancelado\r\nSTATUS:CANCELLED\r\nDTSTART:20261117T080000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:c2\r\nSUMMARY:Vivo\r\nDTSTART:20261117T090000Z\r\nEND:VEVENT\r\n",
        );
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].titulo, "Vivo");
    }

    #[test]
    fn fuera_de_la_ventana_y_webcal() {
        let s = ics("BEGIN:VEVENT\r\nUID:f1\r\nSUMMARY:Lejos\r\nDTSTART:20270601T080000Z\r\nEND:VEVENT\r\n");
        assert!(interpretar(&s, ahora(), MADRID).unwrap().is_empty());
        assert_eq!(normalizar_url("webcal://ejemplo.invalid/a.ics"), "https://ejemplo.invalid/a.ics");
        assert!(interpretar("no es un ics", ahora(), MADRID).is_err());
    }

    #[test]
    fn categorias_simples_y_multiples() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:k1\r\nSUMMARY:Clase\r\nCATEGORIES:Clase\r\nDTSTART:20261117T080000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:k2\r\nSUMMARY:Entrega\r\nCATEGORIES:TFM,Entrega\r\nCATEGORIES:Urgente\r\nDTSTART:20261117T090000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:k3\r\nSUMMARY:Escapada\r\nCATEGORIES:Casa\\, jardin,Otra\r\nDTSTART:20261117T100000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:k4\r\nSUMMARY:Sin\r\nDTSTART:20261117T110000Z\r\nEND:VEVENT\r\n",
        );
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert_eq!(ev[0].categorias, vec!["Clase"]);
        assert_eq!(ev[1].categorias, vec!["TFM", "Entrega", "Urgente"]);
        assert_eq!(ev[2].categorias, vec!["Casa, jardin", "Otra"]);
        assert!(ev[3].categorias.is_empty());
    }

    #[test]
    fn descripcion_sin_escapes_y_recortada() {
        let larga = "a".repeat(300);
        let s = ics(&format!(
            "BEGIN:VEVENT\r\nUID:d9\r\nSUMMARY:Con texto\r\nDESCRIPTION:Linea uno\\nlinea\\, dos\r\nDTSTART:20261117T080000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:d10\r\nSUMMARY:Larga\r\nDESCRIPTION:{larga}\r\nDTSTART:20261117T090000Z\r\nEND:VEVENT\r\n"
        ));
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert_eq!(ev[0].descripcion.as_deref(), Some("Linea uno\nlinea, dos"));
        assert_eq!(ev[1].descripcion.as_ref().unwrap().chars().count(), 200);
    }

    #[test]
    fn id_estable_de_ocurrencia_recurrente() {
        let serie = "BEGIN:VEVENT\r\nUID:r1\r\nSUMMARY:Semanal\r\nDTSTART;TZID=Europe/Madrid:20261103T090000\r\nDTEND;TZID=Europe/Madrid:20261103T093000\r\nRRULE:FREQ=WEEKLY;BYDAY=TU\r\n";
        let a = interpretar(&ics(&format!("{serie}END:VEVENT\r\n")), ahora(), MADRID).unwrap();
        // Otro «hoy» y otro título: la misma ocurrencia conserva el id
        let serie2 = serie.replace("Semanal", "Renombrada");
        let b = interpretar(
            &ics(&format!("{serie2}END:VEVENT\r\n")),
            ahora() + Duration::days(3),
            MADRID,
        )
        .unwrap();
        assert_eq!(a[0].id, "r1|20261117T080000Z");
        assert_eq!(a[1].id, "r1|20261124T080000Z");
        assert_eq!(a[1].uid, "r1");
        let b24 = b.iter().find(|e| e.id == a[1].id).expect("misma ocurrencia");
        assert_eq!(b24.titulo, "Renombrada");
        let ids: HashSet<&String> = a.iter().map(|e| &e.id).collect();
        assert_eq!(ids.len(), a.len());
    }

    #[test]
    fn id_de_excepcion_movida_es_el_original() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:x1\r\nSUMMARY:Serie\r\nDTSTART;TZID=Europe/Madrid:20261103T090000\r\nDTEND;TZID=Europe/Madrid:20261103T100000\r\nRRULE:FREQ=WEEKLY;BYDAY=TU\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:x1\r\nSUMMARY:Serie (movida)\r\nRECURRENCE-ID;TZID=Europe/Madrid:20261124T090000\r\nDTSTART;TZID=Europe/Madrid:20261125T150000\r\nDTEND;TZID=Europe/Madrid:20261125T160000\r\nEND:VEVENT\r\n",
        );
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        let movida = ev.iter().find(|e| e.titulo == "Serie (movida)").unwrap();
        assert_eq!(hora(movida), "2026-11-25 15:00");
        // El id sale del RECURRENCE-ID (24 nov 09:00 Madrid = 08:00 UTC), no del nuevo inicio
        assert_eq!(movida.id, "x1|20261124T080000Z");
    }

    #[test]
    fn ventana_de_seis_meses() {
        // A 150 días (15 abr 2027) aparece; a 200 días (4 jun 2027), no
        let s = ics(
            "BEGIN:VEVENT\r\nUID:l1\r\nSUMMARY:Cerca\r\nDTSTART:20270415T080000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:l2\r\nSUMMARY:Lejos\r\nDTSTART:20270604T080000Z\r\nEND:VEVENT\r\n",
        );
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].titulo, "Cerca");
    }

    #[test]
    fn serie_diaria_larga_es_rapida_y_acotada() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:dd\r\nSUMMARY:Diario\r\nDTSTART;TZID=Europe/Madrid:20200106T090000\r\nDTEND;TZID=Europe/Madrid:20200106T093000\r\nRRULE:FREQ=DAILY\r\nEND:VEVENT\r\n",
        );
        let t = Instant::now();
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert_eq!(ev.len(), 183); // un evento por día de la ventana, ni uno más
        assert!(t.elapsed() < Espera::from_secs(5));
    }

    #[test]
    fn recurrente_solo_en_series_y_excepciones() {
        let s = ics(
            "BEGIN:VEVENT\r\nUID:u1\r\nSUMMARY:Suelto\r\nDTSTART:20261117T080000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:u2\r\nSUMMARY:Serie\r\nDTSTART:20261117T090000Z\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:u2\r\nSUMMARY:Serie movida\r\nRECURRENCE-ID:20261124T090000Z\r\nDTSTART:20261125T100000Z\r\nEND:VEVENT\r\n",
        );
        let ev = interpretar(&s, ahora(), MADRID).unwrap();
        assert!(!ev.iter().find(|e| e.titulo == "Suelto").unwrap().recurrente);
        assert!(ev.iter().filter(|e| e.titulo == "Serie").all(|e| e.recurrente));
        assert!(ev.iter().find(|e| e.titulo == "Serie movida").unwrap().recurrente);
        // Una caché guardada antes de existir el campo se lee como no recurrente
        let json = serde_json::to_string(&ev[0]).unwrap().replace(",\"recurrente\":false", "").replace(",\"recurrente\":true", "");
        assert!(!serde_json::from_str::<Evento>(&json).unwrap().recurrente);
    }

    #[test]
    fn los_eventos_del_ics_nunca_son_pendientes() {
        let s = ics("BEGIN:VEVENT\r\nUID:p1\r\nSUMMARY:Normal\r\nDTSTART:20261117T080000Z\r\nEND:VEVENT\r\n");
        assert!(interpretar(&s, ahora(), MADRID).unwrap().iter().all(|e| !e.pendiente));
    }

    #[test]
    fn duraciones() {
        assert_eq!(parsear_duracion("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parsear_duracion("P1DT2H"), Some(Duration::hours(26)));
    }
}
