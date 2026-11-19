//! Módulo CORREO y creación de eventos mediante la automatización COM del Outlook clásico
//! (`Outlook.Application`). No hay registro de apps ni permisos de la organización: se habla
//! con el Outlook que el usuario ya tiene abierto y configurado.
//!
//! Un único hilo STA (`outlook-sta`) posee todos los objetos COM; el resto del programa le manda
//! peticiones por un canal y espera la respuesta. Ningún objeto COM sale de ese hilo, y ninguno
//! se conserva entre peticiones: cada operación se engancha al Outlook abierto (`GetActiveObject`),
//! trabaja y lo suelta. Así no se mantiene vivo el proceso de Outlook cuando el usuario lo cierra
//! y no hay objetos caducados que reconectar.

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as Dur, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Listener, Manager};
use windows::core::{w, BSTR, GUID, HSTRING, Interface, PCWSTR};
use windows::Win32::Foundation::{VARIANT_BOOL, DISP_E_EXCEPTION};
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoInitializeEx, IDispatch, COINIT_APARTMENTTHREADED, DISPATCH_FLAGS, DISPATCH_METHOD,
    DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS, EXCEPINFO,
};
use windows::Win32::System::Ole::{GetActiveObject, DISPID_PROPERTYPUT};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::System::Variant::{
    VariantClear, VARIANT, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_BSTR, VT_DATE, VT_DISPATCH, VT_I2, VT_I4, VT_R4,
    VT_R8, VT_UI4,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY, SW_SHOWMINNOACTIVE};

use crate::calendario::Evento;
use crate::estado::Estado;
use crate::pendientes::{self, Operacion};
use crate::ventanas;

/// Cada cuánto se relee el correo.
const INTERVALO: Duration = Duration::from_secs(300);
/// Al mostrar el panel se relee si la última lectura es más antigua que esto.
const EDAD_MAX_AL_MOSTRAR: Duration = Duration::from_secs(60);
/// Tiempo máximo esperando a que arranque un Outlook lanzado por el panel.
const ESPERA_ARRANQUE: Duration = Duration::from_secs(60);
/// Tras un error se reintenta antes que en el intervalo normal.
const REINTENTO: Duration = Duration::from_secs(30);
const DIAS_VENTANA: i64 = 14;
const MAX_POR_CUENTA: usize = 200;
const LARGO_FRAGMENTO: usize = 160;
const LCID: u32 = 0x0400; // LOCALE_USER_DEFAULT
const OL_CLASE_CORREO: i32 = 43;
const OL_CARPETA_BANDEJA: i32 = 6;
const OL_CARPETA_CALENDARIO: i32 = 9;
const OL_ITEM_CITA: i32 = 1;
const OL_CLASE_CITA: i32 = 26;
const MSG_CERRADO: &str = "Outlook no está abierto";
/// Tope de citas que se recorren por calendario al buscar un evento a borrar.
const MAX_CITAS_BARRIDO: usize = 5000;

// ===========================================================================
// Lógica pura (sin COM): filtro, fragmento, fechas OLE, categorías y validación
// ===========================================================================

fn quitar_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        _ => c,
    }
}

/// Minúsculas y sin acentos, para comparar textos.
pub fn plegar(t: &str) -> String {
    t.chars().flat_map(|c| c.to_lowercase()).map(quitar_acento).collect()
}

/// ¿La dirección coincide con la entrada VIP? Una dirección exacta (`ana@example.com`) solo
/// coincide consigo misma; un dominio (`@example.com` o `example.com`) coincide con el
/// dominio y con sus subdominios (`alumnos.example.com`), pero no con `notexample.com`.
pub fn vip_coincide(direccion: &str, vip: &str) -> bool {
    let d = direccion.trim().to_lowercase();
    let v = vip.trim().to_lowercase();
    if v.is_empty() || d.is_empty() {
        return false;
    }
    if v.contains('@') && !v.starts_with('@') {
        return d == v;
    }
    let dominio = v.trim_start_matches('@');
    if dominio.is_empty() {
        return false;
    }
    match d.rsplit_once('@') {
        Some((_, host)) => host == dominio || host.ends_with(&format!(".{dominio}")),
        None => false,
    }
}

/// Por qué un correo es importante: `"VIP"` si el remitente coincide con algún VIP o, si no, la
/// primera palabra clave (tal como la escribió el usuario) que aparece en el asunto o en el
/// nombre del remitente. `None` si no es importante.
pub fn motivo_importante(
    direccion: &str,
    remitente: &str,
    asunto: &str,
    vip: &[String],
    palabras: &[String],
) -> Option<String> {
    if vip.iter().any(|v| vip_coincide(direccion, v)) {
        return Some("VIP".to_string());
    }
    let asunto = plegar(asunto);
    let remitente = plegar(remitente);
    palabras
        .iter()
        .find(|p| {
            let p = plegar(p.trim());
            !p.is_empty() && (asunto.contains(&p) || remitente.contains(&p))
        })
        .map(|p| p.trim().to_string())
}

/// Primeras `max` letras del cuerpo con todo el espacio en blanco (y los saltos de línea
/// repetidos) reducido a un solo espacio. Añade «…» si se recorta.
pub fn fragmento(cuerpo: &str, max: usize) -> String {
    // No se recorre un cuerpo enorme entero
    let limpio: String = cuerpo
        .chars()
        .take(max.saturating_mul(20).max(64))
        .filter(|c| !matches!(c, '\u{200b}'..='\u{200f}' | '\u{34f}' | '\u{feff}' | '\u{ad}') && (!c.is_control() || c.is_whitespace()))
        .collect();
    let junto = limpio.split_whitespace().collect::<Vec<_>>().join(" ");
    if junto.chars().count() <= max {
        junto
    } else {
        let corto: String = junto.chars().take(max).collect();
        format!("{}…", corto.trim_end())
    }
}

const BASE_OLE: (i32, u32, u32) = (1899, 12, 30);

fn base_ole() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(BASE_OLE.0, BASE_OLE.1, BASE_OLE.2).unwrap().and_hms_opt(0, 0, 0).unwrap()
}

/// Fecha OLE (días desde 1899-12-30, con la hora en la parte decimal) a fecha y hora locales
/// sin zona. `None` si el valor no es una fecha válida.
pub fn ole_a_naive(d: f64) -> Option<NaiveDateTime> {
    if !d.is_finite() || d.abs() > 3_000_000.0 {
        return None;
    }
    let dias = d.floor();
    let ms = ((d - dias) * 86_400_000.0).round() as i64;
    base_ole().checked_add_signed(Dur::days(dias as i64))?.checked_add_signed(Dur::milliseconds(ms))
}

/// Fecha y hora locales sin zona a fecha OLE.
pub fn naive_a_ole(n: NaiveDateTime) -> f64 {
    (n - base_ole()).num_milliseconds() as f64 / 86_400_000.0
}

/// Fecha OLE (hora local) a fecha con el desfase de la zona local.
pub fn ole_a_local(d: f64) -> Option<DateTime<FixedOffset>> {
    let n = ole_a_naive(d)?;
    Local.from_local_datetime(&n).earliest().map(|x| x.fixed_offset())
}

/// Categorías en el formato de Outlook: nombres separados por «, », sin repetidos (sin
/// distinguir mayúsculas) ni vacíos; las comas y puntos y coma dentro de un nombre se quitan.
pub fn formato_categorias(cats: &[String]) -> String {
    let mut vistas: Vec<String> = Vec::new();
    let mut salida: Vec<String> = Vec::new();
    for c in cats {
        let limpia = c.replace([',', ';'], " ").split_whitespace().collect::<Vec<_>>().join(" ");
        if limpia.is_empty() || vistas.contains(&limpia.to_lowercase()) {
            continue;
        }
        vistas.push(limpia.to_lowercase());
        salida.push(limpia);
    }
    salida.join(", ")
}

/// Datos del formulario «+ EVENTO». Fechas locales `AAAA-MM-DDTHH:MM` (sin zona).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NuevoEvento {
    pub titulo: String,
    pub inicio: String,
    pub fin: String,
    #[serde(default)]
    pub todo_el_dia: bool,
    #[serde(default)]
    pub ubicacion: Option<String>,
    #[serde(default)]
    pub categorias: Vec<String>,
    /// `DisplayName` de la cuenta cuyo calendario se usa; vacío = el principal.
    #[serde(default)]
    pub cuenta: Option<String>,
}

fn leer_fecha_hora(t: &str) -> Option<NaiveDateTime> {
    let t = t.trim();
    for f in ["%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(t, f) {
            return Some(n);
        }
    }
    NaiveDate::parse_from_str(t, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0))
}

/// Valida el evento y devuelve su inicio y su fin. Con «todo el día» el inicio es la
/// medianoche del día de inicio y el fin, la medianoche siguiente al último día.
pub fn validar_evento(e: &NuevoEvento) -> Result<(NaiveDateTime, NaiveDateTime), String> {
    let titulo = e.titulo.trim();
    if titulo.is_empty() {
        return Err("El título no puede estar vacío".into());
    }
    if titulo.chars().count() > 255 {
        return Err("El título es demasiado largo".into());
    }
    let ini = leer_fecha_hora(&e.inicio).ok_or("La fecha de inicio no es válida")?;
    let fin = leer_fecha_hora(&e.fin).ok_or("La fecha de fin no es válida")?;
    if e.todo_el_dia {
        let dia_ini = ini.date();
        let ultimo = fin.date().max(dia_ini);
        let medianoche = |d: NaiveDate| d.and_hms_opt(0, 0, 0).unwrap();
        return Ok((medianoche(dia_ini), medianoche(ultimo + Dur::days(1))));
    }
    if fin <= ini {
        return Err("La hora de fin debe ser posterior a la de inicio".into());
    }
    Ok((ini, fin))
}

// ---------------------------------------------------------------------------
// Borrado de eventos
// ---------------------------------------------------------------------------
//
// La agenda del panel se lee de un ICS de solo lectura: no se puede borrar nada en él. El
// único calendario que el panel puede modificar es el del Outlook clásico abierto (COM),
// así que «borrar» es localizar el mismo evento en Outlook y borrarlo ahí (Outlook lo manda
// a «Elementos eliminados», donde se puede recuperar). Se rechaza todo lo que no sea una
// cita suelta e inequívoca: series (borrar una ocurrencia o toda la serie es decisión del
// usuario en Outlook), reuniones con invitados (borrarlas decide si se avisa a los demás)
// y coincidencias dudosas.

/// Datos de una cita de Outlook que bastan para reconocer un evento de la agenda.
#[derive(Debug, Clone)]
pub struct CitaLeida {
    /// `GlobalAppointmentID`; el ICS de Exchange suele usarlo como UID.
    pub global_id: String,
    pub titulo: String,
    pub inicio: NaiveDateTime,
    pub fin: NaiveDateTime,
    pub todo_el_dia: bool,
    pub recurrente: bool,
    /// `MeetingStatus` distinto de `olNonMeeting`: tiene (o tuvo) invitados.
    pub es_reunion: bool,
}

const MSG_SERIE: &str = "Es un evento recurrente: bórralo desde Outlook (allí eliges si es una ocurrencia o toda la serie)";
const MSG_NO_ENCONTRADO: &str =
    "No se encontró el evento en tu Outlook (puede ser de un calendario de solo lectura o estar ya borrado)";
const MSG_VARIOS: &str = "Hay varios eventos iguales en Outlook: bórralo desde Outlook para no borrar otro por error";
const MSG_REUNION: &str = "Es una reunión con invitados: bórrala desde Outlook para decidir si se avisa a los asistentes";

/// Comprobaciones previas al borrado que no necesitan Outlook.
pub fn validar_borrado(ev: &Evento) -> Result<(), String> {
    if ev.id.trim().is_empty() {
        return Err("El evento no tiene identificador".into());
    }
    if ev.recurrente {
        return Err(MSG_SERIE.into());
    }
    Ok(())
}

/// Margen de búsqueda alrededor del inicio del evento. Cubre cualquier diferencia de zona
/// horaria del filtro DASL; la coincidencia exacta se comprueba después, cita a cita.
pub fn ventana_borrado(ev: &Evento) -> (NaiveDateTime, NaiveDateTime) {
    let ini = ev.inicio.naive_local();
    (ini - Dur::days(1), ini + Dur::days(1))
}

/// Filtro DASL (independiente del idioma) de las citas que empiezan entre `desde` y `hasta`.
pub fn filtro_dasl_citas(desde: NaiveDateTime, hasta: NaiveDateTime) -> String {
    format!(
        "@SQL=\"urn:schemas:calendar:dtstart\" >= '{}' AND \"urn:schemas:calendar:dtstart\" <= '{}'",
        desde.format("%Y-%m-%d %H:%M"),
        hasta.format("%Y-%m-%d %H:%M")
    )
}

fn mismo_instante(a: NaiveDateTime, b: NaiveDateTime) -> bool {
    (a - b).num_seconds().abs() < 60
}

fn coincide_por_uid(ev: &Evento, c: &CitaLeida) -> bool {
    let uid = ev.uid.trim();
    !uid.is_empty() && uid.eq_ignore_ascii_case(c.global_id.trim())
}

fn coincide_por_datos(ev: &Evento, c: &CitaLeida) -> bool {
    c.titulo.trim().to_lowercase() == ev.titulo.trim().to_lowercase()
        && c.todo_el_dia == ev.todo_el_dia
        && mismo_instante(c.inicio, ev.inicio.naive_local())
        && mismo_instante(c.fin, ev.fin.naive_local())
}

/// Decide cuál de las citas de Outlook es el evento de la agenda. Se prefiere el UID; sin
/// coincidencia por UID, título + inicio + fin. Devuelve error (y no se borra nada) si no hay
/// ninguna, si hay varias, o si la elegida es una serie o una reunión con invitados.
pub fn elegir_cita(ev: &Evento, citas: &[CitaLeida]) -> Result<usize, String> {
    let por_uid: Vec<usize> = (0..citas.len()).filter(|&i| coincide_por_uid(ev, &citas[i])).collect();
    let candidatas = if por_uid.is_empty() {
        (0..citas.len()).filter(|&i| coincide_por_datos(ev, &citas[i])).collect()
    } else {
        por_uid
    };
    let i = match candidatas.as_slice() {
        [] => return Err(MSG_NO_ENCONTRADO.into()),
        [i] => *i,
        _ => return Err(MSG_VARIOS.into()),
    };
    if citas[i].recurrente {
        return Err(MSG_SERIE.into());
    }
    if citas[i].es_reunion {
        return Err(MSG_REUNION.into());
    }
    Ok(i)
}

/// Filtro DASL de Outlook, independiente del idioma, para los correos recibidos desde `desde`.
pub fn filtro_dasl(desde: NaiveDateTime) -> String {
    format!(
        "@SQL=\"urn:schemas:httpmail:datereceived\" >= '{}'",
        desde.format("%Y-%m-%d %H:%M")
    )
}

/// Correo leído de la bandeja de entrada (sin filtrar).
#[derive(Debug, Clone)]
pub struct Bruto {
    pub id: String,
    pub store_id: String,
    /// `Store.DisplayName`
    pub cuenta: String,
    pub remitente: String,
    pub direccion: String,
    pub asunto: String,
    pub recibido: DateTime<FixedOffset>,
    pub no_leido: bool,
    /// Solo se lee del cuerpo de los correos que pasan el filtro.
    pub fragmento: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CorreoSalida {
    pub id: String,
    pub store_id: String,
    /// Etiqueta de la cuenta (la elegida por el usuario o el `DisplayName`).
    pub cuenta: String,
    pub remitente: String,
    pub direccion: String,
    pub asunto: String,
    pub recibido: DateTime<FixedOffset>,
    pub no_leido: bool,
    pub fragmento: String,
    pub motivo: String,
}

/// Deja solo los correos importantes, de más reciente a más antiguo y mezclando las cuentas.
pub fn filtrar(
    brutos: &[Bruto],
    vip: &[String],
    palabras: &[String],
    etiquetas: &HashMap<String, String>,
) -> Vec<CorreoSalida> {
    let mut v: Vec<CorreoSalida> = brutos
        .iter()
        .filter_map(|b| {
            let motivo = motivo_importante(&b.direccion, &b.remitente, &b.asunto, vip, palabras)?;
            let etiqueta = etiquetas
                .get(&b.cuenta)
                .map(|e| e.trim())
                .filter(|e| !e.is_empty())
                .unwrap_or(&b.cuenta)
                .to_string();
            Some(CorreoSalida {
                id: b.id.clone(),
                store_id: b.store_id.clone(),
                cuenta: etiqueta,
                remitente: b.remitente.clone(),
                direccion: b.direccion.clone(),
                asunto: b.asunto.clone(),
                recibido: b.recibido,
                no_leido: b.no_leido,
                fragmento: b.fragmento.clone().unwrap_or_default(),
                motivo,
            })
        })
        .collect();
    v.sort_by(|a, b| b.recibido.cmp(&a.recibido));
    v
}

// ===========================================================================
// Llamadas por nombre con IDispatch
// ===========================================================================

/// Error de una llamada COM. Nunca se hace `panic`: todo fallo llega como `Err`.
#[derive(Debug, Clone)]
pub struct ErrCom {
    codigo: u32,
    texto: String,
}

impl ErrCom {
    fn nuevo(texto: &str) -> Self {
        ErrCom { codigo: 0, texto: texto.to_string() }
    }

    fn de(e: windows::core::Error) -> Self {
        ErrCom { codigo: e.code().0 as u32, texto: e.message().trim().to_string() }
    }

    /// Outlook se ha cerrado o ha dejado de responder (error RPC).
    fn desconexion(&self) -> bool {
        matches!(
            self.codigo,
            0x800706BA | 0x800706BE | 0x800706BF | 0x80010108 | 0x80010012 | 0x800706B5 | 0x80010007 | 0x8001010E
        )
    }

    /// Outlook está ocupado: conviene reintentar.
    fn ocupado(&self) -> bool {
        matches!(self.codigo, 0x80010001 | 0x8001010A)
    }
}

impl std::fmt::Display for ErrCom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.texto.is_empty(), self.codigo) {
            (true, 0) => write!(f, "error desconocido de Outlook"),
            (true, c) => write!(f, "error de Outlook (0x{c:08X})"),
            (false, _) => write!(f, "{}", self.texto),
        }
    }
}

type Res<T> = Result<T, ErrCom>;

/// Argumento de una llamada.
#[derive(Debug, Clone)]
enum Arg {
    Str(String),
    I32(i32),
    Bool(bool),
    /// Fecha OLE
    Fecha(f64),
}

/// Resultado de una llamada.
enum Valor {
    Vacio,
    Texto(String),
    Entero(i32),
    Bool(bool),
    Real(f64),
    Fecha(f64),
    Obj(Obj),
}

impl Valor {
    fn texto(&self) -> String {
        match self {
            Valor::Texto(t) => t.clone(),
            _ => String::new(),
        }
    }
    fn entero(&self) -> i32 {
        match self {
            Valor::Entero(n) => *n,
            Valor::Real(r) => *r as i32,
            Valor::Bool(b) => *b as i32,
            _ => 0,
        }
    }
    fn bool(&self) -> bool {
        match self {
            Valor::Bool(b) => *b,
            Valor::Entero(n) => *n != 0,
            _ => false,
        }
    }
    fn fecha(&self) -> Option<f64> {
        match self {
            Valor::Fecha(f) | Valor::Real(f) => Some(*f),
            _ => None,
        }
    }
    /// Objeto devuelto; `Nothing` (o cualquier otro tipo) es un error.
    fn obj(self) -> Res<Obj> {
        match self {
            Valor::Obj(o) => Ok(o),
            _ => Err(ErrCom::nuevo("Outlook no devolvió un objeto")),
        }
    }
    /// Objeto devuelto o `None` si es `Nothing`.
    fn obj_opt(self) -> Option<Obj> {
        match self {
            Valor::Obj(o) => Some(o),
            _ => None,
        }
    }
}

fn variante(a: &Arg) -> VARIANT {
    let mut v = VARIANT::default();
    // SAFETY: `v` está a cero (VT_EMPTY) y solo se escribe el tipo y el campo que le corresponde.
    unsafe {
        let d: &mut VARIANT_0_0 = &mut v.Anonymous.Anonymous;
        match a {
            Arg::Str(s) => {
                d.vt = VT_BSTR;
                d.Anonymous = VARIANT_0_0_0 { bstrVal: ManuallyDrop::new(BSTR::from(s.as_str())) };
            }
            Arg::I32(n) => {
                d.vt = VT_I4;
                d.Anonymous = VARIANT_0_0_0 { lVal: *n };
            }
            Arg::Bool(b) => {
                d.vt = VT_BOOL;
                d.Anonymous = VARIANT_0_0_0 { boolVal: VARIANT_BOOL(if *b { -1 } else { 0 }) };
            }
            Arg::Fecha(f) => {
                d.vt = VT_DATE;
                d.Anonymous = VARIANT_0_0_0 { date: *f };
            }
        }
    }
    v
}

/// Convierte el resultado de una llamada. No libera el VARIANT (lo hace quien llama).
fn valor_de(v: &VARIANT) -> Valor {
    // SAFETY: se lee solo el campo que indica `vt`.
    unsafe {
        let d: &VARIANT_0_0 = &v.Anonymous.Anonymous;
        let vt = d.vt;
        let u = &d.Anonymous;
        if vt == VT_BSTR {
            Valor::Texto(String::from_utf16_lossy(&(*u.bstrVal)))
        } else if vt == VT_I4 {
            Valor::Entero(u.lVal)
        } else if vt == VT_I2 {
            Valor::Entero(u.iVal as i32)
        } else if vt == VT_UI4 {
            Valor::Entero(u.ulVal as i32)
        } else if vt == VT_BOOL {
            Valor::Bool(u.boolVal.0 != 0)
        } else if vt == VT_R8 {
            Valor::Real(u.dblVal)
        } else if vt == VT_R4 {
            Valor::Real(u.fltVal as f64)
        } else if vt == VT_DATE {
            Valor::Fecha(u.date)
        } else if vt == VT_DISPATCH {
            match (*u.pdispVal).clone() {
                Some(d) => Valor::Obj(Obj(d)),
                None => Valor::Vacio,
            }
        } else {
            Valor::Vacio
        }
    }
}

/// Objeto COM de Outlook al que se llama por nombre. Solo vive en el hilo STA.
#[derive(Clone)]
struct Obj(IDispatch);

impl Obj {
    fn id_de(&self, nombre: &str) -> Res<i32> {
        let h = HSTRING::from(nombre);
        let nombres = [PCWSTR(h.as_ptr())];
        let mut id = 0i32;
        // SAFETY: `nombres` e `id` viven durante la llamada.
        unsafe { self.0.GetIDsOfNames(&GUID::zeroed(), nombres.as_ptr(), 1, LCID, &mut id) }.map_err(ErrCom::de)?;
        Ok(id)
    }

    fn invocar(&self, nombre: &str, flags: DISPATCH_FLAGS, args: &[Arg]) -> Res<Valor> {
        let id = self.id_de(nombre)?;
        let mut intento = 0;
        loop {
            // Los argumentos van en orden inverso
            let mut vars: Vec<VARIANT> = args.iter().rev().map(variante).collect();
            let mut id_put = DISPID_PROPERTYPUT;
            let mut parametros = DISPPARAMS {
                rgvarg: if vars.is_empty() { std::ptr::null_mut() } else { vars.as_mut_ptr() },
                rgdispidNamedArgs: std::ptr::null_mut(),
                cArgs: vars.len() as u32,
                cNamedArgs: 0,
            };
            let es_put = flags == DISPATCH_PROPERTYPUT;
            if es_put {
                parametros.rgdispidNamedArgs = &mut id_put;
                parametros.cNamedArgs = 1;
            }
            let mut resultado = VARIANT::default();
            let mut excepcion = EXCEPINFO::default();
            // SAFETY: todos los punteros apuntan a datos locales que viven durante la llamada.
            let r = unsafe {
                self.0.Invoke(
                    id,
                    &GUID::zeroed(),
                    LCID,
                    flags,
                    &parametros,
                    if es_put { None } else { Some(&mut resultado) },
                    Some(&mut excepcion),
                    None,
                )
            };
            // SAFETY: cada VARIANT lo creamos nosotros (BSTR y objetos con su propia referencia).
            for v in vars.iter_mut() {
                unsafe {
                    let _ = VariantClear(v);
                }
            }
            let descripcion = {
                let d = String::from_utf16_lossy(&(*excepcion.bstrDescription));
                // SAFETY: son BSTR de la excepción; se liberan una sola vez.
                unsafe {
                    ManuallyDrop::drop(&mut excepcion.bstrSource);
                    ManuallyDrop::drop(&mut excepcion.bstrDescription);
                    ManuallyDrop::drop(&mut excepcion.bstrHelpFile);
                }
                d
            };
            match r {
                Ok(()) => {
                    let valor = valor_de(&resultado);
                    // SAFETY: el resultado lo rellenó Invoke.
                    unsafe {
                        let _ = VariantClear(&mut resultado);
                    }
                    return Ok(valor);
                }
                Err(e) => {
                    let mut err = ErrCom::de(e);
                    if err.codigo == DISP_E_EXCEPTION.0 as u32 && !descripcion.trim().is_empty() {
                        err.texto = descripcion.trim().to_string();
                    }
                    intento += 1;
                    if err.ocupado() && intento < 5 {
                        thread::sleep(Duration::from_millis(300));
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    /// Lee una propiedad.
    fn get(&self, nombre: &str) -> Res<Valor> {
        self.invocar(nombre, DISPATCH_PROPERTYGET, &[])
    }

    /// Llama a un método o a una propiedad con parámetros (`Item(1)`, `GetDefaultFolder(6)`…).
    fn call(&self, nombre: &str, args: &[Arg]) -> Res<Valor> {
        self.invocar(nombre, DISPATCH_FLAGS(DISPATCH_METHOD.0 | DISPATCH_PROPERTYGET.0), args)
    }

    /// Asigna una propiedad.
    fn put(&self, nombre: &str, valor: Arg) -> Res<()> {
        self.invocar(nombre, DISPATCH_PROPERTYPUT, &[valor]).map(|_| ())
    }
}

// ===========================================================================
// Conexión con Outlook
// ===========================================================================

/// Se engancha al Outlook clásico que ya está abierto. `Ok(None)` si no hay ninguno (nunca
/// lo arranca: `CoCreateInstance` lo abriría a la vista).
fn conectar() -> Res<Option<Obj>> {
    // SAFETY: llamadas COM normales desde un hilo STA inicializado.
    unsafe {
        let clsid = CLSIDFromProgID(w!("Outlook.Application"))
            .map_err(|_| ErrCom::nuevo("El Outlook clásico no está instalado"))?;
        let mut u: Option<windows::core::IUnknown> = None;
        if GetActiveObject(&clsid, None, &mut u).is_err() {
            return Ok(None);
        }
        match u {
            Some(u) => Ok(Some(Obj(u.cast::<IDispatch>().map_err(ErrCom::de)?))),
            None => Ok(None),
        }
    }
}

fn ruta_outlook() -> Option<PathBuf> {
    let mut buf = [0u16; 520];
    let mut tam = (buf.len() * 2) as u32;
    // SAFETY: `buf` y `tam` viven durante la llamada.
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\OUTLOOK.EXE"),
            PCWSTR::null(),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut tam),
        )
    };
    if r.0 == 0 {
        let n = ((tam as usize) / 2).min(buf.len());
        let texto = String::from_utf16_lossy(&buf[..n]);
        let ruta = PathBuf::from(texto.trim_matches(|c: char| c == '\0' || c == '"' || c.is_whitespace()));
        if ruta.is_file() {
            return Some(ruta);
        }
    }
    [
        r"C:\Program Files\Microsoft Office\root\Office16\OUTLOOK.EXE",
        r"C:\Program Files (x86)\Microsoft Office\root\Office16\OUTLOOK.EXE",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// Lanza `OUTLOOK.EXE` minimizado y sin darle el foco.
fn lanzar_outlook() -> Result<(), String> {
    let ruta = ruta_outlook().ok_or("No se encontró el Outlook clásico")?;
    // SAFETY: ShellExecuteW con cadenas que viven durante la llamada.
    let h = unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(ruta.as_os_str()), PCWSTR::null(), PCWSTR::null(), SW_SHOWMINNOACTIVE)
    };
    if h.0 as isize > 32 {
        Ok(())
    } else {
        Err("No se pudo abrir el Outlook clásico".into())
    }
}

// ===========================================================================
// Estado compartido y hilo STA
// ===========================================================================

enum Peticion {
    /// Releer el correo.
    Sincronizar,
    /// El usuario activó «abrir Outlook al arrancar»: se permite un nuevo lanzamiento automático.
    ActivarAuto,
    LanzarOutlook(Sender<Result<(), String>>),
    AbrirCorreo { entry_id: String, store_id: String, resp: Sender<Result<(), String>> },
    Categorias(Sender<Result<Vec<String>, String>>),
    /// Llevar a Outlook las creaciones y borrados apuntados en la cola de `pendientes`.
    VaciarCola,
}

struct Datos {
    /// "conectado" | "cerrado" | "conectando" | "error"
    estado: &'static str,
    detalle: String,
    brutos: Vec<Bruto>,
    cuentas: Vec<String>,
    ultima_sync: Option<DateTime<Utc>>,
    ultima_lectura: Option<Instant>,
}

pub struct Outlook {
    tx: Mutex<Sender<Peticion>>,
    datos: Mutex<Datos>,
    sync_pendiente: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EstadoCorreo {
    pub estado: String,
    pub detalle: String,
    pub correos: Vec<CorreoSalida>,
    pub ultima_sync: Option<DateTime<Utc>>,
    /// `DisplayName` de las cuentas detectadas (para ponerles etiqueta en AJUSTES).
    pub cuentas: Vec<String>,
}

fn instantanea(app: &AppHandle) -> EstadoCorreo {
    let estado_cfg = app.state::<Estado>();
    let (vip, palabras, etiquetas) = {
        let c = estado_cfg.config.lock().unwrap();
        (c.correo_vip.clone(), c.correo_palabras.clone(), c.etiquetas_cuentas_correo.clone())
    };
    let salida = app.state::<Outlook>();
    let d = salida.datos.lock().unwrap();
    EstadoCorreo {
        estado: d.estado.to_string(),
        detalle: d.detalle.clone(),
        correos: filtrar(&d.brutos, &vip, &palabras, &etiquetas),
        ultima_sync: d.ultima_sync,
        cuentas: d.cuentas.clone(),
    }
}

fn emitir(app: &AppHandle) {
    let _ = app.emit("correo://estado", instantanea(app));
}

fn enviar(app: &AppHandle, p: Peticion) -> Result<(), String> {
    app.state::<Outlook>()
        .tx
        .lock()
        .unwrap()
        .send(p)
        .map_err(|_| "El módulo de Outlook no está activo".to_string())
}

/// Pide una lectura del correo (si ya hay una pendiente, no hace nada).
fn pedir_sync(app: &AppHandle) {
    if !app.state::<Outlook>().sync_pendiente.swap(true, Ordering::SeqCst) {
        let _ = enviar(app, Peticion::Sincronizar);
    }
}

/// Manda una petición al hilo y espera su respuesta (llamar desde un hilo que pueda bloquear).
fn pedir<T>(
    app: &AppHandle,
    crear: impl FnOnce(Sender<Result<T, String>>) -> Peticion,
    espera: Duration,
) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    enviar(app, crear(tx))?;
    rx.recv_timeout(espera).map_err(|_| "Outlook no respondió a tiempo".to_string())?
}

/// Cambio de VIP, palabras o etiquetas: se refiltra con lo que ya se leyó, sin tocar Outlook.
/// Si algún correo que ahora pasa el filtro no tiene fragmento, se lee en segundo plano.
pub fn refiltrar(app: &AppHandle) {
    emitir(app);
    let estado_cfg = app.state::<Estado>();
    let (vip, palabras) = {
        let c = estado_cfg.config.lock().unwrap();
        (c.correo_vip.clone(), c.correo_palabras.clone())
    };
    let salida = app.state::<Outlook>();
    let faltan = salida.datos.lock().unwrap().brutos.iter().any(|b| {
        b.fragmento.is_none() && motivo_importante(&b.direccion, &b.remitente, &b.asunto, &vip, &palabras).is_some()
    });
    if faltan {
        pedir_sync(app);
    }
}

/// El usuario activó «abrir Outlook al arrancar».
pub fn activar_auto(app: &AppHandle) {
    let _ = enviar(app, Peticion::ActivarAuto);
}

enum Intento {
    Bien,
    Cerrado,
    Fallo(ErrCom),
}

struct Motor {
    app: AppHandle,
    /// Direcciones SMTP ya resueltas de remitentes de Exchange (dirección X.500 -> SMTP).
    smtp: HashMap<String, String>,
    /// El lanzamiento automático se hace una sola vez por arranque (o al activar la opción).
    auto_intentado: bool,
    conectando_hasta: Option<Instant>,
    proxima: Instant,
}

impl Motor {
    fn config(&self) -> crate::config::Config {
        self.app.state::<Estado>().config.lock().unwrap().clone()
    }

    fn fijar(&self, estado: &'static str, detalle: &str, borrar_correo: bool) {
        {
            let o = self.app.state::<Outlook>();
            let mut d = o.datos.lock().unwrap();
            d.estado = estado;
            d.detalle = detalle.to_string();
            if borrar_correo {
                d.brutos.clear();
            }
        }
        emitir(&self.app);
    }

    fn intentar(&mut self) -> Intento {
        match conectar() {
            Ok(Some(app)) => match self.leer(&app) {
                Ok(()) => Intento::Bien,
                Err(e) => Intento::Fallo(e),
            },
            Ok(None) => Intento::Cerrado,
            Err(e) => Intento::Fallo(e),
        }
    }

    /// Un ciclo de conexión y lectura; decide el estado y cuándo se vuelve a intentar.
    fn ciclo(&mut self) {
        self.app.state::<Outlook>().sync_pendiente.store(false, Ordering::SeqCst);
        self.proxima = Instant::now() + INTERVALO;
        let esperando = self.conectando_hasta.map(|t| Instant::now() < t).unwrap_or(false);
        match self.intentar() {
            Intento::Bien => {
                self.conectando_hasta = None;
                // Outlook está abierto: se llevan a él las acciones que esperaban
                self.vaciar_cola();
            }
            Intento::Cerrado => {
                if esperando {
                    return; // sigue arrancando
                }
                let auto = self.config().outlook_auto;
                if auto && !self.auto_intentado {
                    self.auto_intentado = true;
                    self.arrancar();
                } else {
                    self.conectando_hasta = None;
                    self.fijar("cerrado", "", true);
                    // Con acciones esperando, se vigila más a menudo si Outlook se abre
                    if pendientes::hay_por_enviar(&self.app) {
                        self.proxima = Instant::now() + REINTENTO;
                    }
                }
            }
            Intento::Fallo(e) => {
                if esperando {
                    return; // Outlook aún está cargando el perfil
                }
                self.conectando_hasta = None;
                if e.desconexion() {
                    self.fijar("cerrado", "", true);
                } else {
                    self.fijar("error", &e.to_string(), false);
                }
                self.proxima = Instant::now() + REINTENTO;
            }
        }
        // Se acabó la espera sin conseguir conectar
        if self.conectando_hasta.map(|t| Instant::now() >= t).unwrap_or(false) {
            self.conectando_hasta = None;
            self.fijar("error", "Outlook no respondió tras abrirlo", false);
            self.proxima = Instant::now() + REINTENTO;
        }
    }

    /// Lanza Outlook y queda a la espera de poder conectar (hasta 60 s).
    fn arrancar(&mut self) {
        match lanzar_outlook() {
            Ok(()) => {
                self.conectando_hasta = Some(Instant::now() + ESPERA_ARRANQUE);
                self.fijar("conectando", "", true);
            }
            Err(e) => {
                self.conectando_hasta = None;
                self.fijar("error", &e, false);
            }
        }
    }

    /// Lee la bandeja de entrada de cada cuenta y guarda el resultado.
    fn leer(&mut self, app: &Obj) -> Res<()> {
        let cfg = self.config();
        let sesion = app.get("Session")?.obj()?;
        let almacenes = sesion.get("Stores")?.obj()?;
        let total = almacenes.get("Count")?.entero();
        let desde = Local::now().naive_local() - Dur::days(DIAS_VENTANA);
        let previos: HashMap<String, Option<String>> = {
            let o = self.app.state::<Outlook>();
            let d = o.datos.lock().unwrap();
            d.brutos.iter().map(|b| (b.id.clone(), b.fragmento.clone())).collect()
        };
        let mut brutos: Vec<Bruto> = Vec::new();
        let mut cuentas: Vec<String> = Vec::new();
        for i in 1..=total {
            let Ok(almacen) = almacenes.call("Item", &[Arg::I32(i)]).and_then(|v| v.obj()) else { continue };
            // PST sin bandeja, carpetas públicas…: se ignoran
            let Ok(bandeja) = almacen.call("GetDefaultFolder", &[Arg::I32(OL_CARPETA_BANDEJA)]).and_then(|v| v.obj())
            else {
                continue;
            };
            let nombre = almacen.get("DisplayName").map(|v| v.texto()).unwrap_or_default();
            let store_id = almacen.get("StoreID").map(|v| v.texto()).unwrap_or_default();
            match self.leer_bandeja(&bandeja, &nombre, &store_id, desde, &cfg, &previos) {
                Ok(mut v) => {
                    cuentas.push(nombre);
                    brutos.append(&mut v);
                }
                Err(e) if e.desconexion() => return Err(e),
                Err(_) => cuentas.push(nombre),
            }
        }
        {
            let o = self.app.state::<Outlook>();
            let mut d = o.datos.lock().unwrap();
            d.estado = "conectado";
            d.detalle = String::new();
            d.brutos = brutos;
            d.cuentas = cuentas;
            d.ultima_sync = Some(Utc::now());
            d.ultima_lectura = Some(Instant::now());
        }
        emitir(&self.app);
        Ok(())
    }

    fn leer_bandeja(
        &mut self,
        bandeja: &Obj,
        cuenta: &str,
        store_id: &str,
        desde: NaiveDateTime,
        cfg: &crate::config::Config,
        previos: &HashMap<String, Option<String>>,
    ) -> Res<Vec<Bruto>> {
        let items = bandeja.get("Items")?.obj()?;
        // El filtro DASL no depende del idioma; si no devuelve nada o falla, se recorre todo
        // ordenado y se corta por fecha igualmente.
        let lista = match items
            .call("Restrict", &[Arg::Str(filtro_dasl(desde))])
            .and_then(|v| v.obj())
        {
            Ok(r) if r.get("Count").map(|c| c.entero()).unwrap_or(0) > 0 => r,
            _ => items,
        };
        let ordenado = lista.call("Sort", &[Arg::Str("[ReceivedTime]".into()), Arg::Bool(true)]).is_ok();
        let mut salida: Vec<Bruto> = Vec::new();
        let mut actual = lista.call("GetFirst", &[])?.obj_opt();
        while let Some(it) = actual {
            if salida.len() >= MAX_POR_CUENTA {
                break;
            }
            actual = lista.call("GetNext", &[]).ok().and_then(|v| v.obj_opt());
            let Ok(clase) = it.get("Class") else { continue };
            if clase.entero() != OL_CLASE_CORREO {
                continue;
            }
            let Some(recibido_ole) = it.get("ReceivedTime").ok().and_then(|v| v.fecha()) else { continue };
            let Some(naive) = ole_a_naive(recibido_ole) else { continue };
            if naive < desde {
                if ordenado {
                    break;
                }
                continue;
            }
            let Some(recibido) = ole_a_local(recibido_ole) else { continue };
            let id = it.get("EntryID").map(|v| v.texto()).unwrap_or_default();
            if id.is_empty() {
                continue;
            }
            let asunto = it.get("Subject").map(|v| v.texto()).unwrap_or_default();
            let remitente = it.get("SenderName").map(|v| v.texto()).unwrap_or_default();
            let direccion = self.direccion_remitente(&it);
            let no_leido = it.get("UnRead").map(|v| v.bool()).unwrap_or(false);
            let mut frag = previos.get(&id).cloned().flatten();
            if frag.is_none()
                && motivo_importante(&direccion, &remitente, &asunto, &cfg.correo_vip, &cfg.correo_palabras).is_some()
            {
                frag = it.get("Body").ok().map(|v| fragmento(&v.texto(), LARGO_FRAGMENTO));
            }
            salida.push(Bruto {
                id,
                store_id: store_id.to_string(),
                cuenta: cuenta.to_string(),
                remitente,
                direccion,
                asunto,
                recibido,
                no_leido,
                fragmento: frag,
            });
        }
        Ok(salida)
    }

    /// Dirección SMTP del remitente; en Exchange hay que resolver la dirección interna.
    fn direccion_remitente(&mut self, it: &Obj) -> String {
        let bruta = it.get("SenderEmailAddress").map(|v| v.texto()).unwrap_or_default();
        let tipo = it.get("SenderEmailType").map(|v| v.texto()).unwrap_or_default();
        if !tipo.eq_ignore_ascii_case("EX") {
            return bruta;
        }
        if let Some(s) = self.smtp.get(&bruta) {
            return s.clone();
        }
        let smtp = (|| -> Res<String> {
            let remitente = it.get("Sender")?.obj()?;
            let usuario = remitente.call("GetExchangeUser", &[])?.obj()?;
            Ok(usuario.get("PrimarySmtpAddress")?.texto())
        })()
        .ok()
        .filter(|s| s.contains('@'))
        .unwrap_or_else(|| bruta.clone());
        self.smtp.insert(bruta, smtp.clone());
        smtp
    }

    fn manejar(&mut self, p: Peticion) {
        match p {
            Peticion::Sincronizar => self.ciclo(),
            Peticion::ActivarAuto => {
                self.auto_intentado = false;
                self.ciclo();
            }
            Peticion::LanzarOutlook(resp) => {
                self.auto_intentado = true;
                let r = match conectar() {
                    Ok(Some(_)) => {
                        self.ciclo();
                        Ok(())
                    }
                    Ok(None) => {
                        self.arrancar();
                        self.proxima = Instant::now() + INTERVALO;
                        if self.conectando_hasta.is_some() {
                            Ok(())
                        } else {
                            Err("No se pudo abrir el Outlook clásico".to_string())
                        }
                    }
                    Err(e) => Err(e.to_string()),
                };
                let _ = resp.send(r);
            }
            Peticion::AbrirCorreo { entry_id, store_id, resp } => {
                let _ = resp.send(self.abrir_correo(&entry_id, &store_id));
            }
            Peticion::Categorias(resp) => {
                let _ = resp.send(self.categorias());
            }
            Peticion::VaciarCola => {
                // Con Outlook cerrado no se hace nada: la cola espera (y se reintenta cada 30 s)
                if matches!(conectar(), Ok(Some(_))) {
                    self.vaciar_cola();
                }
            }
        }
    }

    /// Lleva a Outlook, en orden, lo que el usuario creó o borró desde el panel mientras
    /// Outlook estaba cerrado (o hace un instante). Los fallos pasajeros se reintentan; los
    /// definitivos (serie, reunión, evento no encontrado…) se descartan y se avisa.
    fn vaciar_cola(&mut self) {
        let mut enviado = false;
        for op in pendientes::por_enviar(&self.app) {
            let clave = op.clave();
            let r = match &op {
                Operacion::Crear { evento, .. } => match validar_evento(evento) {
                    Err(m) => Err(ErrOp::Definitivo(m)),
                    Ok(_) => self.crear_evento(evento).map_err(ErrOp::Transitorio),
                },
                Operacion::Borrar { evento, .. } => self.borrar_evento(evento),
            };
            match r {
                Ok(()) => {
                    pendientes::operacion_enviada(&self.app, &clave);
                    enviado = true;
                }
                Err(ErrOp::Definitivo(m)) => pendientes::descartar(&self.app, &op, &m),
                Err(ErrOp::Transitorio(m)) => {
                    if m == MSG_CERRADO {
                        break; // Outlook se cerró: se sigue otro día, sin gastar intentos
                    }
                    pendientes::fallo_transitorio(&self.app, &clave, &m);
                }
            }
        }
        if enviado {
            // Outlook tarda en subir los cambios: se vuelve a leer el ICS a los 90 s y a los 4 min
            for segundos in [90u64, 240] {
                let h = self.app.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs(segundos));
                    crate::calendario::sincronizar(&h);
                });
            }
        }
    }

    fn con_outlook(&self) -> Result<Obj, String> {
        match conectar() {
            Ok(Some(o)) => Ok(o),
            Ok(None) => Err(MSG_CERRADO.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn abrir_correo(&mut self, entry_id: &str, store_id: &str) -> Result<(), String> {
        let app = self.con_outlook()?;
        let sesion = app.get("Session").and_then(|v| v.obj()).map_err(|e| e.to_string())?;
        let item = sesion
            .call("GetItemFromID", &[Arg::Str(entry_id.to_string()), Arg::Str(store_id.to_string())])
            .and_then(|v| v.obj())
            .map_err(|_| "No se encontró el correo (¿lo moviste o lo borraste?)".to_string())?;
        // El panel tiene el foco: se permite que Outlook lo tome para mostrar el correo delante
        // SAFETY: llamada simple de Win32.
        unsafe {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
        }
        item.call("Display", &[]).map_err(|e| format!("No se pudo abrir el correo: {e}"))?;
        if let Ok(inspector) = item.call("GetInspector", &[]).and_then(|v| v.obj()) {
            let _ = inspector.call("Activate", &[]);
        }
        Ok(())
    }

    fn categorias(&mut self) -> Result<Vec<String>, String> {
        let app = self.con_outlook()?;
        let r = (|| -> Res<Vec<String>> {
            let sesion = app.get("Session")?.obj()?;
            let cats = sesion.get("Categories")?.obj()?;
            let n = cats.get("Count")?.entero();
            let mut v = Vec::new();
            for i in 1..=n {
                if let Ok(c) = cats.call("Item", &[Arg::I32(i)]).and_then(|c| c.obj()) {
                    let nombre = c.get("Name").map(|n| n.texto()).unwrap_or_default();
                    if !nombre.trim().is_empty() {
                        v.push(nombre);
                    }
                }
            }
            Ok(v)
        })();
        r.map_err(|e| format!("No se pudieron leer las categorías: {e}"))
    }

    fn crear_evento(&mut self, ev: &NuevoEvento) -> Result<(), String> {
        let (ini, fin) = validar_evento(ev)?;
        let app = self.con_outlook()?;
        let r = (|| -> Res<()> {
            let sesion = app.get("Session")?.obj()?;
            let carpeta = match ev.cuenta.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                Some(nombre) => {
                    let almacenes = sesion.get("Stores")?.obj()?;
                    let n = almacenes.get("Count")?.entero();
                    let mut hallada = None;
                    for i in 1..=n {
                        let Ok(a) = almacenes.call("Item", &[Arg::I32(i)]).and_then(|v| v.obj()) else { continue };
                        if a.get("DisplayName").map(|d| d.texto()).unwrap_or_default() == nombre {
                            hallada = Some(a.call("GetDefaultFolder", &[Arg::I32(OL_CARPETA_CALENDARIO)])?.obj()?);
                            break;
                        }
                    }
                    hallada.ok_or_else(|| ErrCom::nuevo("No se encontró esa cuenta en Outlook"))?
                }
                None => sesion.call("GetDefaultFolder", &[Arg::I32(OL_CARPETA_CALENDARIO)])?.obj()?,
            };
            let items = carpeta.get("Items")?.obj()?;
            let cita = items.call("Add", &[Arg::I32(OL_ITEM_CITA)])?.obj()?;
            cita.put("Subject", Arg::Str(ev.titulo.trim().to_string()))?;
            if ev.todo_el_dia {
                cita.put("AllDayEvent", Arg::Bool(true))?;
            }
            cita.put("Start", Arg::Fecha(naive_a_ole(ini)))?;
            cita.put("End", Arg::Fecha(naive_a_ole(fin)))?;
            if let Some(u) = ev.ubicacion.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
                cita.put("Location", Arg::Str(u.to_string()))?;
            }
            let cats = formato_categorias(&ev.categorias);
            if !cats.is_empty() {
                cita.put("Categories", Arg::Str(cats))?;
            }
            cita.call("Save", &[])?;
            Ok(())
        })();
        r.map_err(|e| {
            if e.desconexion() {
                "Outlook se cerró mientras se creaba el evento".to_string()
            } else {
                format!("No se pudo crear el evento: {e}")
            }
        })
    }
}

/// Citas de todos los calendarios principales de Outlook que empiezan cerca del evento.
fn buscar_citas(app: &Obj, ev: &Evento) -> Res<Vec<(Obj, CitaLeida)>> {
    let (desde, hasta) = ventana_borrado(ev);
    let sesion = app.get("Session")?.obj()?;
    let almacenes = sesion.get("Stores")?.obj()?;
    let total = almacenes.get("Count")?.entero();
    let mut salida: Vec<(Obj, CitaLeida)> = Vec::new();
    for i in 1..=total {
        let Ok(almacen) = almacenes.call("Item", &[Arg::I32(i)]).and_then(|v| v.obj()) else { continue };
        let Ok(carpeta) = almacen
            .call("GetDefaultFolder", &[Arg::I32(OL_CARPETA_CALENDARIO)])
            .and_then(|v| v.obj())
        else {
            continue;
        };
        let Ok(items) = carpeta.get("Items").and_then(|v| v.obj()) else { continue };
        // Si el filtro falla o no devuelve nada se recorre todo (con tope); cada cita se
        // vuelve a comprobar por fecha, así que el filtro solo sirve para ir más rápido.
        let lista = match items
            .call("Restrict", &[Arg::Str(filtro_dasl_citas(desde, hasta))])
            .and_then(|v| v.obj())
        {
            Ok(r) if r.get("Count").map(|c| c.entero()).unwrap_or(0) > 0 => r,
            Err(e) if e.desconexion() => return Err(e),
            _ => items,
        };
        let mut actual = lista.call("GetFirst", &[]).ok().and_then(|v| v.obj_opt());
        let mut vistas = 0;
        while let Some(it) = actual {
            if vistas >= MAX_CITAS_BARRIDO {
                break;
            }
            vistas += 1;
            actual = lista.call("GetNext", &[]).ok().and_then(|v| v.obj_opt());
            if it.get("Class").map(|c| c.entero()).unwrap_or(0) != OL_CLASE_CITA {
                continue;
            }
            let Some(inicio) = it.get("Start").ok().and_then(|v| v.fecha()).and_then(ole_a_naive) else { continue };
            if inicio < desde || inicio > hasta {
                continue;
            }
            let Some(fin) = it.get("End").ok().and_then(|v| v.fecha()).and_then(ole_a_naive) else { continue };
            let cita = CitaLeida {
                global_id: it.get("GlobalAppointmentID").map(|v| v.texto()).unwrap_or_default(),
                titulo: it.get("Subject").map(|v| v.texto()).unwrap_or_default(),
                inicio,
                fin,
                todo_el_dia: it.get("AllDayEvent").map(|v| v.bool()).unwrap_or(false),
                // Si no se puede saber, se trata como serie / reunión: no se borra
                recurrente: it.get("IsRecurring").map(|v| v.bool()).unwrap_or(true),
                es_reunion: it.get("MeetingStatus").map(|v| v.entero() != 0).unwrap_or(true),
            };
            salida.push((it, cita));
        }
    }
    Ok(salida)
}

impl Motor {
    /// Borra la cita de Outlook que corresponde al evento (va a «Elementos eliminados»).
    fn borrar_evento(&mut self, ev: &Evento) -> Result<(), ErrOp> {
        validar_borrado(ev).map_err(ErrOp::Definitivo)?;
        let app = self.con_outlook().map_err(ErrOp::Transitorio)?;
        let texto_error = |e: ErrCom| {
            ErrOp::Transitorio(if e.desconexion() {
                "Outlook se cerró mientras se borraba el evento".to_string()
            } else {
                format!("No se pudo borrar el evento: {e}")
            })
        };
        let halladas = buscar_citas(&app, ev).map_err(texto_error)?;
        let citas: Vec<CitaLeida> = halladas.iter().map(|(_, c)| c.clone()).collect();
        let i = elegir_cita(ev, &citas).map_err(ErrOp::Definitivo)?;
        halladas[i].0.call("Delete", &[]).map(|_| ()).map_err(texto_error)
    }
}

/// Resultado fallido de una operación de la cola.
enum ErrOp {
    /// No se va a poder nunca (serie, reunión, evento no encontrado…): se descarta y se avisa.
    Definitivo(String),
    /// Puede arreglarse solo (Outlook cerrado u ocupado): se reintenta.
    Transitorio(String),
}

fn bucle(app: AppHandle, rx: mpsc::Receiver<Peticion>) {
    // SAFETY: este hilo es el STA que posee todos los objetos COM de Outlook.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let mut m = Motor { app, smtp: HashMap::new(), auto_intentado: false, conectando_hasta: None, proxima: Instant::now() };
    m.ciclo();
    loop {
        let espera = if m.conectando_hasta.is_some() {
            Duration::from_secs(2)
        } else {
            m.proxima.saturating_duration_since(Instant::now()).max(Duration::from_millis(500))
        };
        match rx.recv_timeout(espera) {
            Ok(p) => m.manejar(p),
            Err(RecvTimeoutError::Timeout) => m.ciclo(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

// ===========================================================================
// Comandos
// ===========================================================================

#[tauri::command]
pub fn estado_correo(app: AppHandle) -> EstadoCorreo {
    instantanea(&app)
}

#[tauri::command]
pub fn sincronizar_correo(app: AppHandle) {
    pedir_sync(&app);
}

async fn en_hilo<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Abre el correo en Outlook y esconde el panel.
#[tauri::command]
pub async fn abrir_correo(app: AppHandle, entry_id: String, store_id: String) -> Result<(), String> {
    let h = app.clone();
    en_hilo(move || {
        pedir(&h, |resp| Peticion::AbrirCorreo { entry_id, store_id, resp }, Duration::from_secs(30))
    })
    .await?;
    ventanas::pedir_ocultar(&app);
    Ok(())
}

/// Lanza el Outlook clásico minimizado (una vez, sin esperar a que arranque).
#[tauri::command]
pub async fn abrir_outlook(app: AppHandle) -> Result<(), String> {
    en_hilo(move || pedir(&app, Peticion::LanzarOutlook, Duration::from_secs(30))).await
}

#[tauri::command]
pub async fn categorias_outlook(app: AppHandle) -> Result<Vec<String>, String> {
    en_hilo(move || pedir(&app, Peticion::Categorias, Duration::from_secs(30))).await
}

/// Pide al hilo de Outlook que vacíe la cola de pendientes (si Outlook está abierto).
fn despertar_cola(app: &AppHandle) {
    let _ = enviar(app, Peticion::VaciarCola);
}

/// Crea un evento: aparece en la agenda al instante (marcado «pendiente») y se crea en
/// Outlook ahora si está abierto, o en cuanto se abra.
#[tauri::command]
pub async fn crear_evento(app: AppHandle, evento: NuevoEvento) -> Result<(), String> {
    pendientes::anadir_creacion(&app, evento)?;
    despertar_cola(&app);
    Ok(())
}

/// Borra un evento: desaparece de la agenda al instante y se borra de Outlook ahora si está
/// abierto, o en cuanto se abra (el ICS no se puede modificar). Rechaza ya mismo las series;
/// las reuniones con invitados y los eventos que no se encuentran en Outlook se avisan al
/// intentar el borrado (`pendientes://fallo`) y vuelven a la agenda.
#[tauri::command]
pub async fn borrar_evento(app: AppHandle, evento: Evento) -> Result<(), String> {
    if pendientes::anadir_borrado(&app, &evento)? != pendientes::AccionBorrado::CreacionCancelada {
        despertar_cola(&app);
    }
    Ok(())
}

pub fn iniciar(app: &AppHandle) {
    let (tx, rx) = mpsc::channel();
    app.manage(Outlook {
        tx: Mutex::new(tx),
        datos: Mutex::new(Datos {
            estado: "conectando",
            detalle: String::new(),
            brutos: Vec::new(),
            cuentas: Vec::new(),
            ultima_sync: None,
            ultima_lectura: None,
        }),
        sync_pendiente: AtomicBool::new(false),
    });
    let h = app.clone();
    let _ = thread::Builder::new().name("outlook-sta".into()).spawn(move || bucle(h, rx));

    // Al mostrar el panel se relee si la última lectura tiene más de 60 s
    let h = app.clone();
    app.listen("panel://mostrar", move |_| {
        let vieja = h
            .state::<Outlook>()
            .datos
            .lock()
            .unwrap()
            .ultima_lectura
            .map_or(true, |t| t.elapsed() > EDAD_MAX_AL_MOSTRAR);
        if vieja {
            pedir_sync(&h);
        }
    });
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn v(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn vip_por_direccion_exacta() {
        assert!(vip_coincide("ana@example.com", "ana@example.com"));
        assert!(vip_coincide("Ana@Example.COM", " ana@example.com "));
        assert!(!vip_coincide("otra@example.com", "ana@example.com"));
        assert!(!vip_coincide("ana@example.com.evil.org", "ana@example.com"));
    }

    #[test]
    fn vip_por_dominio_con_y_sin_arroba_sin_distinguir_mayusculas() {
        for entrada in ["@example.com", "example.com", "@EXAMPLE.com", "Example.COM"] {
            assert!(vip_coincide("profe@example.com", entrada), "{entrada}");
            assert!(vip_coincide("PROFE@Example.Com", entrada), "{entrada}");
            assert!(!vip_coincide("profe@example.org", entrada), "{entrada}");
        }
        // Subdominios sí; dominios que solo terminan igual, no
        assert!(vip_coincide("a@alumnos.example.com", "example.com"));
        assert!(!vip_coincide("a@notexample.com", "example.com"));
        assert!(!vip_coincide("sin-arroba", "example.com"));
        assert!(!vip_coincide("a@example.com", ""));
        assert!(!vip_coincide("a@example.com", "@"));
    }

    #[test]
    fn palabras_sin_distinguir_acentos_ni_mayusculas() {
        let pal = v(&["Examen", "convocatoria"]);
        assert_eq!(motivo_importante("x@example.com", "X", "EXÁMEN final", &[], &pal), Some("Examen".into()));
        assert_eq!(motivo_importante("x@example.com", "X", "Convocatória abierta", &[], &pal), Some("convocatoria".into()));
        assert_eq!(motivo_importante("x@example.com", "Oficina de Exámenes", "Hola", &[], &pal), Some("Examen".into()));
        assert_eq!(motivo_importante("x@example.com", "X", "Hola", &[], &pal), None);
        // Una palabra vacía no coincide con todo
        assert_eq!(motivo_importante("x@example.com", "X", "Hola", &[], &v(&["  "])), None);
        assert_eq!(plegar("ÁÉÍÓÚ Ñandú"), "aeiou nandu");
    }

    #[test]
    fn motivo_vip_tiene_prioridad() {
        let motivo = motivo_importante("jefe@example.com", "Jefe", "Examen", &v(&["@example.com"]), &v(&["examen"]));
        assert_eq!(motivo, Some("VIP".into()));
        let motivo = motivo_importante("otro@example.org", "Otro", "Examen", &v(&["@example.com"]), &v(&["examen"]));
        assert_eq!(motivo, Some("examen".into()));
    }

    #[test]
    fn fragmento_recortado_y_sin_saltos_repetidos() {
        assert_eq!(fragmento("Hola\r\n\r\n\r\n  mundo \t\n  otra   vez", 160), "Hola mundo otra vez");
        assert_eq!(fragmento("   ", 160), "");
        let largo = "palabra ".repeat(100);
        let f = fragmento(&largo, 20);
        assert!(f.ends_with('…'));
        assert!(f.chars().count() <= 21);
        assert!(!f.contains("  "));
        // Exactamente el máximo: sin puntos suspensivos
        assert_eq!(fragmento("abcde", 5), "abcde");
        assert_eq!(fragmento("abcdef", 5), "abcde…");
        // Caracteres invisibles de los boletines
        assert_eq!(fragmento("a\u{200b}\u{34f}b \u{feff}c", 160), "ab c");
    }

    #[test]
    fn fechas_ole_a_local_y_al_reves() {
        let n = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap().and_hms_opt(12, 30, 0).unwrap();
        let ole = naive_a_ole(n);
        assert_eq!(ole_a_naive(ole), Some(n));
        // Valores conocidos: 1900-01-01 = 2.0; 2000-01-01 = 36526.0
        assert_eq!(naive_a_ole(NaiveDate::from_ymd_opt(1900, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()), 2.0);
        assert_eq!(ole_a_naive(36526.5), Some(NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(12, 0, 0).unwrap()));
        assert_eq!(ole_a_naive(f64::NAN), None);
        assert_eq!(ole_a_naive(f64::INFINITY), None);
        // La conversión a fecha local conserva la hora de reloj
        let local = ole_a_local(ole).unwrap();
        assert_eq!(local.naive_local(), n);
    }

    #[test]
    fn categorias_separadas_por_coma() {
        assert_eq!(formato_categorias(&v(&["TFM", "Clase"])), "TFM, Clase");
        assert_eq!(formato_categorias(&v(&[" TFM ", "tfm", "", "A, B", "C;D"])), "TFM, A B, C D");
        assert_eq!(formato_categorias(&[]), "");
    }

    fn evento(titulo: &str, ini: &str, fin: &str, todo: bool) -> NuevoEvento {
        NuevoEvento {
            titulo: titulo.into(),
            inicio: ini.into(),
            fin: fin.into(),
            todo_el_dia: todo,
            ubicacion: None,
            categorias: vec![],
            cuenta: None,
        }
    }

    #[test]
    fn validacion_de_fin_posterior_a_inicio() {
        let ok = validar_evento(&evento("Tutoría", "2026-11-18T10:00", "2026-11-18T11:00", false)).unwrap();
        assert_eq!(ok.0.to_string(), "2026-11-18 10:00:00");
        assert_eq!(ok.1.to_string(), "2026-11-18 11:00:00");
        assert!(validar_evento(&evento("X", "2026-11-18T10:00", "2026-11-18T10:00", false)).is_err());
        assert!(validar_evento(&evento("X", "2026-11-18T10:00", "2026-11-18T09:00", false)).is_err());
        assert!(validar_evento(&evento("  ", "2026-11-18T10:00", "2026-11-18T11:00", false)).is_err());
        assert!(validar_evento(&evento("X", "basura", "2026-11-18T11:00", false)).is_err());
    }

    #[test]
    fn evento_de_todo_el_dia_acaba_en_la_medianoche_siguiente() {
        let (i, f) = validar_evento(&evento("Festivo", "2026-11-18T10:00", "2026-11-18T10:00", true)).unwrap();
        assert_eq!(i.to_string(), "2026-11-18 00:00:00");
        assert_eq!(f.to_string(), "2026-11-19 00:00:00");
        let (_, f) = validar_evento(&evento("Viaje", "2026-11-18", "2026-11-20", true)).unwrap();
        assert_eq!(f.to_string(), "2026-11-21 00:00:00");
    }

    fn naive(d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn ev_agenda(uid: &str, titulo: &str, ini: NaiveDateTime, fin: NaiveDateTime) -> Evento {
        let desfase = chrono::FixedOffset::east_opt(3600).unwrap();
        Evento {
            id: format!("{uid}|x"),
            uid: uid.into(),
            titulo: titulo.into(),
            inicio: desfase.from_local_datetime(&ini).unwrap(),
            fin: desfase.from_local_datetime(&fin).unwrap(),
            todo_el_dia: false,
            ubicacion: None,
            categorias: vec![],
            descripcion: None,
            recurrente: false,
            pendiente: false,
        }
    }

    fn cita(gid: &str, titulo: &str, ini: NaiveDateTime, fin: NaiveDateTime) -> CitaLeida {
        CitaLeida {
            global_id: gid.into(),
            titulo: titulo.into(),
            inicio: ini,
            fin,
            todo_el_dia: false,
            recurrente: false,
            es_reunion: false,
        }
    }

    #[test]
    fn borrado_rechaza_series_y_eventos_sin_id() {
        let mut e = ev_agenda("U1", "Clase", naive(18, 10, 0), naive(18, 11, 0));
        assert!(validar_borrado(&e).is_ok());
        e.recurrente = true;
        assert!(validar_borrado(&e).unwrap_err().contains("recurrente"));
        e.recurrente = false;
        e.id = "  ".into();
        assert!(validar_borrado(&e).is_err());
    }

    #[test]
    fn borrado_localiza_por_uid_sin_distinguir_mayusculas() {
        let e = ev_agenda("0400ABCD", "Tutoría", naive(18, 10, 0), naive(18, 11, 0));
        let citas = vec![
            cita("OTRA", "Tutoría", naive(18, 10, 0), naive(18, 11, 0)),
            cita("0400abcd", "Titulo cambiado", naive(18, 12, 0), naive(18, 13, 0)),
        ];
        // El UID manda aunque el título y la hora ya no coincidan
        assert_eq!(elegir_cita(&e, &citas), Ok(1));
    }

    #[test]
    fn borrado_sin_uid_usa_titulo_e_inicio_y_fin() {
        let e = ev_agenda("sin-coincidencia", "Tutoría", naive(18, 10, 0), naive(18, 11, 0));
        let citas = vec![
            cita("A", "Tutoría", naive(18, 10, 0), naive(18, 12, 0)), // otro fin
            cita("B", "Tutoria", naive(18, 10, 0), naive(18, 11, 0)), // otro título (sin tilde)
            cita("C", "  tutoría ", naive(18, 10, 0), naive(18, 11, 0)),
        ];
        assert_eq!(elegir_cita(&e, &citas), Ok(2));
        // Una diferencia de segundos (redondeo de las fechas OLE) no impide reconocerlo
        let mut c = cita("D", "Tutoría", naive(18, 10, 0), naive(18, 11, 0));
        c.inicio += Dur::seconds(20);
        assert_eq!(elegir_cita(&e, &[c]), Ok(0));
    }

    #[test]
    fn borrado_no_encontrado_o_ambiguo_no_borra_nada() {
        let e = ev_agenda("U", "Tutoría", naive(18, 10, 0), naive(18, 11, 0));
        assert!(elegir_cita(&e, &[]).unwrap_err().contains("No se encontró"));
        assert!(elegir_cita(&e, &[cita("Z", "Otra", naive(18, 10, 0), naive(18, 11, 0))]).is_err());
        let dos = vec![
            cita("X", "Tutoría", naive(18, 10, 0), naive(18, 11, 0)),
            cita("Y", "Tutoría", naive(18, 10, 0), naive(18, 11, 0)),
        ];
        assert!(elegir_cita(&e, &dos).unwrap_err().contains("varios"));
        // Dos citas con el mismo UID tampoco se resuelven a la suerte
        let mismo_uid = vec![
            cita("U", "Tutoría", naive(18, 10, 0), naive(18, 11, 0)),
            cita("u", "Tutoría", naive(18, 10, 0), naive(18, 11, 0)),
        ];
        assert!(elegir_cita(&e, &mismo_uid).is_err());
    }

    #[test]
    fn borrado_rechaza_series_y_reuniones_aunque_coincidan() {
        let e = ev_agenda("U", "Tutoría", naive(18, 10, 0), naive(18, 11, 0));
        let mut serie = cita("U", "Tutoría", naive(18, 10, 0), naive(18, 11, 0));
        serie.recurrente = true;
        assert!(elegir_cita(&e, &[serie]).unwrap_err().contains("recurrente"));
        let mut reunion = cita("U", "Tutoría", naive(18, 10, 0), naive(18, 11, 0));
        reunion.es_reunion = true;
        assert!(elegir_cita(&e, &[reunion]).unwrap_err().contains("invitados"));
    }

    #[test]
    fn borrado_distingue_dia_completo() {
        let mut e = ev_agenda("U", "Festivo", naive(18, 0, 0), naive(19, 0, 0));
        e.todo_el_dia = true;
        let mut c = cita("Q", "Festivo", naive(18, 0, 0), naive(19, 0, 0));
        assert!(elegir_cita(&e, &[c.clone()]).is_err()); // la cita no es de día completo
        c.todo_el_dia = true;
        assert_eq!(elegir_cita(&e, &[c]), Ok(0));
    }

    #[test]
    fn borrado_busca_en_una_ventana_de_un_dia_y_filtro_dasl() {
        let e = ev_agenda("U", "X", naive(18, 10, 0), naive(18, 11, 0));
        let (d, h) = ventana_borrado(&e);
        assert_eq!(d, naive(17, 10, 0));
        assert_eq!(h, naive(19, 10, 0));
        assert_eq!(
            filtro_dasl_citas(d, h),
            "@SQL=\"urn:schemas:calendar:dtstart\" >= '2026-11-17 10:00' AND \"urn:schemas:calendar:dtstart\" <= '2026-11-19 10:00'"
        );
    }

    #[test]
    fn filtro_dasl_independiente_del_idioma() {
        let n = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(9, 5, 0).unwrap();
        assert_eq!(
            filtro_dasl(n),
            "@SQL=\"urn:schemas:httpmail:datereceived\" >= '2026-11-03 09:05'"
        );
    }

    fn bruto(id: &str, cuenta: &str, dir: &str, asunto: &str, hora: u32) -> Bruto {
        Bruto {
            id: id.into(),
            store_id: "S".into(),
            cuenta: cuenta.into(),
            remitente: "Remitente".into(),
            direccion: dir.into(),
            asunto: asunto.into(),
            recibido: Utc.with_ymd_and_hms(2026, 11, 16, hora, 0, 0).unwrap().fixed_offset(),
            no_leido: true,
            fragmento: None,
        }
    }

    #[test]
    fn filtrar_mezcla_cuentas_ordena_y_etiqueta() {
        let brutos = vec![
            bruto("1", "Cuenta A", "a@example.com", "Hola", 8),
            bruto("2", "Cuenta B", "b@example.org", "Examen mañana", 10),
            bruto("3", "Cuenta A", "c@example.net", "Spam", 12),
            bruto("4", "Cuenta A", "d@example.com", "Otra cosa", 11),
        ];
        let mut etiquetas = HashMap::new();
        etiquetas.insert("Cuenta A".to_string(), "UNI".to_string());
        let r = filtrar(&brutos, &v(&["example.com"]), &v(&["examen"]), &etiquetas);
        let ids: Vec<&str> = r.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["4", "2", "1"]);
        assert_eq!(r[0].cuenta, "UNI");
        assert_eq!(r[1].cuenta, "Cuenta B");
        assert_eq!(r[0].motivo, "VIP");
        assert_eq!(r[1].motivo, "examen");
        assert!(filtrar(&brutos, &[], &[], &etiquetas).is_empty());
    }
}
