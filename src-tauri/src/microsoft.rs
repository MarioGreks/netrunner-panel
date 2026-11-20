//! Módulo MICROSOFT: conexión de varias cuentas con Microsoft identity platform
//! (OAuth 2.0 authorization code + PKCE, redirección loopback, cliente público sin secreto).
//!
//! - El `refresh_token` va en el Administrador de credenciales de Windows (crate `keyring`),
//!   troceado en varias entradas porque una credencial admite pocos bytes.
//! - El `access_token` y su caducidad viven solo en memoria.
//! - En `config.json` solo hay datos de la cuenta (id, correo, nombre, etiqueta, tenant).
//! - Jamás se escriben tokens, códigos de autorización ni el `code_verifier` en logs,
//!   errores o eventos.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{self, CuentaMs};
use crate::estado::Estado;

const SERVICIO: &str = "netrunner-panel";
const URL_AUTORIZAR: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
const URL_TOKEN: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const URL_GRAPH: &str = "https://graph.microsoft.com/v1.0";
const SCOPES_LOGIN: &str = "openid profile offline_access User.Read Mail.Read Calendars.ReadWrite";
/// Ámbito de Graph para los access tokens (sin los de identidad).
pub const SCOPES_GRAPH: &str = "User.Read Mail.Read Calendars.ReadWrite";
const TENANT_PERSONAL: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";
const ESPERA_LOGIN: Duration = Duration::from_secs(180);
const TIMEOUT_GRAPH: Duration = Duration::from_secs(20);
const MARGEN_CADUCIDAD: i64 = 300;
const TROZO: usize = 800;
const MAX_ETIQUETA: usize = 24;
const CARACTERES_PKCE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

// ---------------------------------------------------------------------------
// Errores
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoError {
    Bloqueada,
    Reconectar,
    Otro,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorMs {
    pub tipo: TipoError,
    pub mensaje: String,
}

impl ErrorMs {
    fn otro(m: impl Into<String>) -> Self {
        ErrorMs { tipo: TipoError::Otro, mensaje: m.into() }
    }
    fn tiempo_agotado() -> Self {
        Self::otro("No se completó el inicio de sesión a tiempo")
    }
}

impl std::fmt::Display for ErrorMs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.mensaje)
    }
}

/// Extrae el primer código `AADSTSnnnnn` de un texto.
fn extraer_aadsts(texto: &str) -> Option<String> {
    let i = texto.find("AADSTS")?;
    let digitos: String = texto[i + 6..].chars().take_while(|c| c.is_ascii_digit()).collect();
    if digitos.is_empty() {
        None
    } else {
        Some(format!("AADSTS{digitos}"))
    }
}

/// Descripción corta: primera línea, sin identificadores de traza, tope de 140 letras.
fn descripcion_corta(d: &str) -> String {
    let l = d.lines().next().unwrap_or("");
    let l = l.split("Trace ID").next().unwrap_or(l).trim();
    l.chars().take(140).collect()
}

/// Traduce un error de Microsoft (`error` + `error_description`) a un mensaje claro.
pub fn mapear_error(codigo: &str, descripcion: &str) -> ErrorMs {
    let cod = codigo.trim().to_ascii_lowercase();
    let aadsts = extraer_aadsts(descripcion).or_else(|| extraer_aadsts(codigo));
    let a = aadsts.as_deref().unwrap_or("");
    if matches!(a, "AADSTS65001" | "AADSTS90094")
        || cod == "consent_required"
        || cod == "admin_consent_required"
    {
        return ErrorMs {
            tipo: TipoError::Bloqueada,
            mensaje: "Tu organización exige que un administrador apruebe esta app".into(),
        };
    }
    if cod == "access_denied" {
        return ErrorMs::otro("Cancelaste el inicio de sesión");
    }
    if cod == "invalid_client" || a == "AADSTS700016" {
        return ErrorMs::otro("El ID de cliente no es válido o la app no permite este tipo de cuenta");
    }
    if matches!(cod.as_str(), "invalid_grant" | "interaction_required" | "login_required") {
        return ErrorMs {
            tipo: TipoError::Reconectar,
            mensaje: "Hay que volver a conectar la cuenta".into(),
        };
    }
    let corta = descripcion_corta(descripcion);
    let mensaje = match (codigo.is_empty(), corta.is_empty()) {
        (true, true) => "Error desconocido de Microsoft".to_string(),
        (false, true) => codigo.to_string(),
        (true, false) => corta,
        (false, false) => format!("{codigo}: {corta}"),
    };
    ErrorMs::otro(mensaje)
}

// ---------------------------------------------------------------------------
// PKCE y URL de autorización
// ---------------------------------------------------------------------------

/// Cadena aleatoria del conjunto permitido por RFC 7636 (rechazo para evitar sesgo).
fn aleatorio(n: usize) -> Result<String, ErrorMs> {
    let limite = (256 / CARACTERES_PKCE.len()) * CARACTERES_PKCE.len();
    let mut salida = String::with_capacity(n);
    let mut buf = [0u8; 64];
    while salida.len() < n {
        getrandom::fill(&mut buf).map_err(|_| ErrorMs::otro("No se pudo generar un valor aleatorio seguro"))?;
        for b in buf {
            if (b as usize) < limite && salida.len() < n {
                salida.push(CARACTERES_PKCE[b as usize % CARACTERES_PKCE.len()] as char);
            }
        }
    }
    Ok(salida)
}

pub fn generar_verifier() -> Result<String, ErrorMs> {
    aleatorio(64)
}

pub fn challenge_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn url_autorizacion(client_id: &str, puerto: u16, state: &str, challenge: &str) -> String {
    let redirect = format!("http://localhost:{puerto}");
    let mut url = url::Url::parse(URL_AUTORIZAR).expect("URL fija válida");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_mode", "query")
        .append_pair("scope", SCOPES_LOGIN)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "select_account");
    url.into()
}

// ---------------------------------------------------------------------------
// Redirección loopback
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Redireccion {
    Codigo(String),
    Error(ErrorMs),
    /// Petición sin `code` ni `error` (p. ej. el favicon): se ignora.
    Ignorar,
}

/// Interpreta la primera línea de la petición HTTP (`GET /?code=… HTTP/1.1`).
fn leer_redireccion(linea: &str, state: &str) -> Redireccion {
    let Some(destino) = linea.split_whitespace().nth(1) else {
        return Redireccion::Ignorar;
    };
    let Ok(url) = url::Url::parse(&format!("http://localhost{destino}")) else {
        return Redireccion::Ignorar;
    };
    let mut codigo = None;
    let mut error = None;
    let mut descripcion = String::new();
    let mut recibido = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => codigo = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            "error_description" => descripcion = v.into_owned(),
            "state" => recibido = Some(v.into_owned()),
            _ => {}
        }
    }
    if codigo.is_none() && error.is_none() {
        return Redireccion::Ignorar;
    }
    if recibido.as_deref() != Some(state) {
        return Redireccion::Error(ErrorMs::otro(
            "La respuesta no coincide con la solicitud (state incorrecto)",
        ));
    }
    if let Some(e) = error {
        return Redireccion::Error(mapear_error(&e, &descripcion));
    }
    match codigo {
        Some(c) if !c.is_empty() => Redireccion::Codigo(c),
        _ => Redireccion::Error(ErrorMs::otro("Microsoft no devolvió ningún código")),
    }
}

fn escapar_html(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn pagina_html(resultado: &Result<(), ErrorMs>) -> String {
    let (titulo, texto) = match resultado {
        Ok(()) => ("CUENTA CONECTADA", "Cuenta conectada. Ya puedes cerrar esta pestaña.".to_string()),
        Err(e) => ("NO SE PUDO CONECTAR", e.mensaje.clone()),
    };
    format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\"><title>Netrunner</title></head>\
         <body style=\"margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;\
         background:rgb(7,8,12);color:rgb(217,219,227);font-family:Consolas,monospace\">\
         <div style=\"text-align:center;padding:32px;border:1px solid rgb(45,226,230)\">\
         <h1 style=\"font-size:16px;letter-spacing:.2em;color:rgb(45,226,230)\">{}</h1>\
         <p style=\"font-size:14px\">{}</p></div></body></html>",
        titulo,
        escapar_html(&texto)
    )
}

fn responder(mut s: TcpStream, estado: &str, cuerpo: &str) {
    let r = format!(
        "HTTP/1.1 {estado}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{cuerpo}",
        cuerpo.len()
    );
    let _ = s.write_all(r.as_bytes());
    let _ = s.flush();
}

/// Lee la primera línea de la petición (tope de 8 KB y 5 s).
fn leer_primera_linea(s: &mut TcpStream) -> Option<String> {
    let _ = s.set_nonblocking(false);
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let mut datos = Vec::new();
    let mut buf = [0u8; 1024];
    while datos.len() < 8192 && !datos.windows(2).any(|w| w == b"\r\n") {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => datos.extend_from_slice(&buf[..n]),
        }
    }
    let texto = String::from_utf8_lossy(&datos).into_owned();
    texto.lines().next().map(|l| l.to_string())
}

/// Espera la redirección hasta el plazo. Devuelve el flujo abierto (para responder al final).
fn esperar_redireccion(
    listener: &TcpListener,
    state: &str,
) -> Result<(TcpStream, Result<String, ErrorMs>), ErrorMs> {
    listener.set_nonblocking(true).map_err(|_| ErrorMs::otro("No se pudo escuchar en el puerto local"))?;
    let limite = Instant::now() + ESPERA_LOGIN;
    loop {
        match listener.accept() {
            Ok((mut s, _)) => {
                let Some(linea) = leer_primera_linea(&mut s) else { continue };
                match leer_redireccion(&linea, state) {
                    Redireccion::Ignorar => responder(s, "404 Not Found", ""),
                    Redireccion::Codigo(c) => return Ok((s, Ok(c))),
                    Redireccion::Error(e) => return Ok((s, Err(e))),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= limite {
                    return Err(ErrorMs::tiempo_agotado());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return Err(ErrorMs::otro("Error al esperar la respuesta del navegador")),
        }
    }
}

// ---------------------------------------------------------------------------
// id_token (sin verificar la firma: solo para mostrar datos)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Claims {
    pub oid: Option<String>,
    pub tid: Option<String>,
    pub sub: Option<String>,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
    pub email: Option<String>,
}

pub fn decodificar_id_token(token: &str) -> Result<Claims, ErrorMs> {
    let malo = || ErrorMs::otro("Microsoft devolvió un id_token no válido");
    let payload = token.split('.').nth(1).ok_or_else(malo)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).map_err(|_| malo())?;
    serde_json::from_slice(&bytes).map_err(|_| malo())
}

pub fn etiqueta_defecto(tenant: &str) -> &'static str {
    if tenant.eq_ignore_ascii_case(TENANT_PERSONAL) {
        "PERSONAL"
    } else {
        "UAH"
    }
}

/// Construye la cuenta a partir de los claims (None si falta un identificador).
fn cuenta_desde_claims(c: &Claims) -> Option<CuentaMs> {
    let tenant = c.tid.clone().unwrap_or_default();
    let usuario = c.oid.clone().or_else(|| c.sub.clone())?;
    let id = if tenant.is_empty() { usuario } else { format!("{usuario}.{tenant}") };
    let correo = c.preferred_username.clone().or_else(|| c.email.clone()).unwrap_or_default();
    let nombre = c.name.clone().unwrap_or_else(|| correo.clone());
    Some(CuentaMs { id, correo, nombre, etiqueta: etiqueta_defecto(&tenant).to_string(), tenant })
}

// ---------------------------------------------------------------------------
// Caducidad y troceado de secretos (lógica pura)
// ---------------------------------------------------------------------------

pub fn necesita_refresco(caduca: i64, ahora: i64) -> bool {
    caduca - ahora < MARGEN_CADUCIDAD
}

/// Parte un secreto en trozos; el primero lleva el prefijo `N|` con el número de trozos.
fn partir_secreto(s: &str, tam: usize) -> Vec<String> {
    let letras: Vec<char> = s.chars().collect();
    let mut trozos: Vec<String> = letras.chunks(tam.max(1)).map(|c| c.iter().collect()).collect();
    if trozos.is_empty() {
        trozos.push(String::new());
    }
    let n = trozos.len();
    trozos[0] = format!("{n}|{}", trozos[0]);
    trozos
}

fn cabecera_secreto(cab: &str) -> Option<(usize, &str)> {
    let (n, resto) = cab.split_once('|')?;
    let n: usize = n.parse().ok()?;
    if (1..=64).contains(&n) {
        Some((n, resto))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Almacén de credenciales de Windows
// ---------------------------------------------------------------------------

fn entrada(id: &str, i: usize) -> Result<keyring::Entry, String> {
    let usuario = if i == 0 { id.to_string() } else { format!("{id}#{i}") };
    keyring::Entry::new(SERVICIO, &usuario).map_err(|_| "No se pudo acceder al Administrador de credenciales".to_string())
}

fn guardar_refresco(id: &str, token: &str) -> Result<(), String> {
    let trozos = partir_secreto(token, TROZO);
    for (i, t) in trozos.iter().enumerate() {
        entrada(id, i)?
            .set_password(t)
            .map_err(|_| "No se pudo guardar la credencial de forma segura".to_string())?;
    }
    // Trozos sobrantes de un token anterior más largo
    for i in trozos.len()..trozos.len() + 8 {
        if let Ok(e) = entrada(id, i) {
            let _ = e.delete_credential();
        }
    }
    Ok(())
}

fn leer_refresco(id: &str) -> Option<String> {
    let cab = entrada(id, 0).ok()?.get_password().ok()?;
    let (n, primero) = cabecera_secreto(&cab)?;
    let mut total = primero.to_string();
    for i in 1..n {
        total.push_str(&entrada(id, i).ok()?.get_password().ok()?);
    }
    if total.is_empty() {
        None
    } else {
        Some(total)
    }
}

fn borrar_refresco(id: &str) {
    let n = entrada(id, 0)
        .ok()
        .and_then(|e| e.get_password().ok())
        .and_then(|c| cabecera_secreto(&c).map(|(n, _)| n))
        .unwrap_or(1);
    for i in 0..n.max(1) + 8 {
        if let Ok(e) = entrada(id, i) {
            let _ = e.delete_credential();
        }
    }
}

// ---------------------------------------------------------------------------
// Estado en memoria y eventos
// ---------------------------------------------------------------------------

struct Acceso {
    token: String,
    caduca: i64,
}

#[derive(Default)]
pub struct Microsoft {
    accesos: Mutex<HashMap<String, Acceso>>,
    estados: Mutex<HashMap<String, (&'static str, Option<String>)>>,
    refresco: Mutex<()>,
    conectando: AtomicBool,
}

#[derive(Serialize, Clone, Debug)]
pub struct CuentaMsVista {
    pub id: String,
    pub correo: String,
    pub nombre: String,
    pub etiqueta: String,
    /// "conectada" | "reconectar" | "bloqueada" | "error"
    pub estado: String,
    pub detalle: Option<String>,
}

pub fn iniciar(app: &AppHandle) {
    app.manage(Microsoft::default());
}

fn ahora_s() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn nombre_estado(t: TipoError) -> &'static str {
    match t {
        TipoError::Bloqueada => "bloqueada",
        TipoError::Reconectar => "reconectar",
        TipoError::Otro => "error",
    }
}

fn instantanea(app: &AppHandle) -> Vec<CuentaMsVista> {
    let cuentas = app.state::<Estado>().config.lock().unwrap().cuentas_ms.clone();
    let ms = app.state::<Microsoft>();
    let estados = ms.estados.lock().unwrap();
    cuentas
        .into_iter()
        .map(|c| {
            let (estado, detalle) = estados.get(&c.id).cloned().unwrap_or(("conectada", None));
            CuentaMsVista {
                id: c.id,
                correo: c.correo,
                nombre: c.nombre,
                etiqueta: c.etiqueta,
                estado: estado.to_string(),
                detalle,
            }
        })
        .collect()
}

fn emitir(app: &AppHandle) {
    let _ = app.emit("cuentas_ms://estado", instantanea(app));
}

fn fijar_estado(app: &AppHandle, id: &str, estado: &'static str, detalle: Option<String>) {
    app.state::<Microsoft>().estados.lock().unwrap().insert(id.to_string(), (estado, detalle));
    emitir(app);
}

fn marcar_error(app: &AppHandle, id: &str, e: &ErrorMs) {
    fijar_estado(app, id, nombre_estado(e.tipo), Some(e.mensaje.clone()));
}

/// Modifica las cuentas de la configuración y la guarda.
fn modificar_cuentas(app: &AppHandle, f: impl FnOnce(&mut Vec<CuentaMs>)) -> Result<(), ErrorMs> {
    let estado = app.state::<Estado>();
    let mut c = estado.config.lock().unwrap();
    f(&mut c.cuentas_ms);
    config::guardar(app, &c).map_err(|_| ErrorMs::otro("No se pudo guardar la configuración"))
}

fn client_id(app: &AppHandle) -> Result<String, ErrorMs> {
    app.state::<Estado>()
        .config
        .lock()
        .unwrap()
        .ms_client_id
        .clone()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| ErrorMs::otro("Falta el ID de cliente (Entra) en AJUSTES"))
}

// ---------------------------------------------------------------------------
// Red
// ---------------------------------------------------------------------------

fn cliente(timeout: Duration) -> Result<reqwest::blocking::Client, ErrorMs> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .user_agent("netrunner-panel")
        .build()
        .map_err(|_| ErrorMs::otro("No se pudo iniciar la conexión"))
}

fn error_red(e: &reqwest::Error) -> ErrorMs {
    if e.is_timeout() {
        ErrorMs::otro("Tiempo de espera agotado")
    } else {
        ErrorMs::otro("Sin conexión con Microsoft")
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RespuestaToken {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
}

fn pedir_token(params: &[(&str, &str)]) -> Result<RespuestaToken, ErrorMs> {
    let resp = cliente(Duration::from_secs(20))?.post(URL_TOKEN).form(params).send().map_err(|e| error_red(&e))?;
    let estado = resp.status();
    let r: RespuestaToken = resp
        .json()
        .map_err(|_| ErrorMs::otro(format!("Respuesta inesperada de Microsoft (HTTP {})", estado.as_u16())))?;
    if let Some(e) = &r.error {
        return Err(mapear_error(e, r.error_description.as_deref().unwrap_or("")));
    }
    if !estado.is_success() || r.access_token.is_none() {
        return Err(ErrorMs::otro(format!("Respuesta inesperada de Microsoft (HTTP {})", estado.as_u16())));
    }
    Ok(r)
}

fn guardar_acceso(app: &AppHandle, id: &str, scopes: &str, r: &RespuestaToken) {
    if let Some(t) = &r.access_token {
        let caduca = ahora_s() + r.expires_in.unwrap_or(3600);
        app.state::<Microsoft>()
            .accesos
            .lock()
            .unwrap()
            .insert(format!("{id}|{scopes}"), Acceso { token: t.clone(), caduca });
    }
}

/// Devuelve un access token válido para la cuenta; refresca si faltan menos de 5 min.
pub fn token(app: &AppHandle, cuenta_id: &str, scopes: &str) -> Result<String, ErrorMs> {
    let ms = app.state::<Microsoft>();
    let clave = format!("{cuenta_id}|{scopes}");
    let en_cache = |ms: &Microsoft| {
        ms.accesos
            .lock()
            .unwrap()
            .get(&clave)
            .filter(|a| !necesita_refresco(a.caduca, ahora_s()))
            .map(|a| a.token.clone())
    };
    if let Some(t) = en_cache(&ms) {
        return Ok(t);
    }
    // Un solo refresco a la vez: el refresh_token puede rotar
    let _g = ms.refresco.lock().unwrap();
    if let Some(t) = en_cache(&ms) {
        return Ok(t);
    }
    let resultado = refrescar(app, cuenta_id, scopes);
    match &resultado {
        Ok(_) => {
            let mut est = ms.estados.lock().unwrap();
            let cambia = est.get(cuenta_id).map(|e| e.0 != "conectada").unwrap_or(false);
            if cambia {
                est.remove(cuenta_id);
                drop(est);
                emitir(app);
            }
        }
        Err(e) => marcar_error(app, cuenta_id, e),
    }
    resultado
}

fn refrescar(app: &AppHandle, cuenta_id: &str, scopes: &str) -> Result<String, ErrorMs> {
    let cid = client_id(app)?;
    let refresco = leer_refresco(cuenta_id).ok_or(ErrorMs {
        tipo: TipoError::Reconectar,
        mensaje: "Hay que volver a conectar la cuenta".into(),
    })?;
    let r = pedir_token(&[
        ("client_id", &cid),
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresco),
        ("scope", scopes),
    ])?;
    if let Some(nuevo) = &r.refresh_token {
        if nuevo != &refresco {
            guardar_refresco(cuenta_id, nuevo).map_err(ErrorMs::otro)?;
        }
    }
    guardar_acceso(app, cuenta_id, scopes, &r);
    Ok(r.access_token.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

fn graph(app: &AppHandle, cuenta_id: &str, post: bool, ruta: &str, cuerpo: Option<&Value>) -> Result<Value, ErrorMs> {
    let url = format!("{URL_GRAPH}/{}", ruta.trim_start_matches('/'));
    let http = cliente(TIMEOUT_GRAPH)?;
    for intento in 0..2 {
        let t = token(app, cuenta_id, SCOPES_GRAPH)?;
        let mut p = if post { http.post(&url) } else { http.get(&url) }.bearer_auth(t);
        if let Some(c) = cuerpo {
            p = p.json(c);
        }
        let resp = p.send().map_err(|e| error_red(&e))?;
        let estado = resp.status();
        if estado.as_u16() == 401 && intento == 0 {
            // Token rechazado: se olvida y se refresca una vez
            app.state::<Microsoft>().accesos.lock().unwrap().remove(&format!("{cuenta_id}|{SCOPES_GRAPH}"));
            continue;
        }
        let texto = resp.text().unwrap_or_default();
        if estado.is_success() {
            return Ok(serde_json::from_str(&texto).unwrap_or(Value::Null));
        }
        let codigo = serde_json::from_str::<Value>(&texto)
            .ok()
            .and_then(|v| v["error"]["code"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        let tipo = if estado.as_u16() == 401 { TipoError::Reconectar } else { TipoError::Otro };
        return Err(ErrorMs {
            tipo,
            mensaje: if codigo.is_empty() {
                format!("Microsoft Graph respondió HTTP {}", estado.as_u16())
            } else {
                format!("Microsoft Graph respondió HTTP {} ({codigo})", estado.as_u16())
            },
        });
    }
    Err(ErrorMs::otro("Microsoft Graph rechazó el token"))
}

pub fn graph_get(app: &AppHandle, cuenta_id: &str, ruta: &str) -> Result<Value, ErrorMs> {
    graph(app, cuenta_id, false, ruta, None)
}

// ---------------------------------------------------------------------------
// Inicio de sesión
// ---------------------------------------------------------------------------

fn abrir_navegador(url: &str) -> Result<(), ErrorMs> {
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    if crate::accesos::shell_execute(&windows::core::HSTRING::from(url), SW_SHOWNORMAL) {
        Ok(())
    } else {
        Err(ErrorMs::otro("No se pudo abrir el navegador"))
    }
}

struct GuardaConexion<'a>(&'a AtomicBool);
impl Drop for GuardaConexion<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn conectar(app: &AppHandle) -> Result<(), ErrorMs> {
    let cid = client_id(app)?;
    let ms = app.state::<Microsoft>();
    if ms.conectando.swap(true, Ordering::SeqCst) {
        return Err(ErrorMs::otro("Ya hay un inicio de sesión en curso"));
    }
    let _guarda = GuardaConexion(&ms.conectando);

    let verifier = generar_verifier()?;
    let state = aleatorio(32)?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| ErrorMs::otro("No se pudo abrir un puerto local"))?;
    let puerto = listener.local_addr().map_err(|_| ErrorMs::otro("No se pudo abrir un puerto local"))?.port();
    let url = url_autorizacion(&cid, puerto, &state, &challenge_s256(&verifier));
    abrir_navegador(&url)?;

    let (flujo, codigo) = esperar_redireccion(&listener, &state)?;
    let resultado = (|| -> Result<(), ErrorMs> {
        let codigo = codigo?;
        let redirect = format!("http://localhost:{puerto}");
        let r = pedir_token(&[
            ("client_id", &cid),
            ("grant_type", "authorization_code"),
            ("code", &codigo),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
            ("scope", SCOPES_LOGIN),
        ])?;
        let claims = decodificar_id_token(r.id_token.as_deref().unwrap_or(""))?;
        let cuenta = cuenta_desde_claims(&claims).ok_or_else(|| ErrorMs::otro("El id_token no identifica la cuenta"))?;
        let refresco = r.refresh_token.clone().ok_or_else(|| ErrorMs::otro("Microsoft no devolvió credencial de refresco"))?;
        guardar_refresco(&cuenta.id, &refresco).map_err(ErrorMs::otro)?;
        guardar_acceso(app, &cuenta.id, SCOPES_LOGIN, &r);
        let id = cuenta.id.clone();
        modificar_cuentas(app, |lista| {
            if let Some(e) = lista.iter_mut().find(|c| c.id == cuenta.id) {
                e.correo = cuenta.correo.clone();
                e.nombre = cuenta.nombre.clone();
                e.tenant = cuenta.tenant.clone();
                if e.etiqueta.trim().is_empty() {
                    e.etiqueta = cuenta.etiqueta.clone();
                }
            } else {
                lista.push(cuenta.clone());
            }
        })?;
        app.state::<Microsoft>().estados.lock().unwrap().remove(&id);
        emitir(app);
        Ok(())
    })();
    responder(flujo, "200 OK", &pagina_html(&resultado));
    resultado
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn estado_cuentas_ms(app: AppHandle) -> Vec<CuentaMsVista> {
    instantanea(&app)
}

#[tauri::command]
pub async fn conectar_cuenta_ms(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || conectar(&app))
        .await
        .map_err(|_| "Error interno al conectar".to_string())?
        .map_err(|e| e.mensaje)
}

#[tauri::command]
pub fn desconectar_cuenta_ms(app: AppHandle, id: String) -> Result<(), String> {
    borrar_refresco(&id);
    let ms = app.state::<Microsoft>();
    ms.accesos.lock().unwrap().retain(|k, _| !k.starts_with(&format!("{id}|")));
    ms.estados.lock().unwrap().remove(&id);
    modificar_cuentas(&app, |l| l.retain(|c| c.id != id)).map_err(|e| e.mensaje)?;
    emitir(&app);
    Ok(())
}

#[tauri::command]
pub fn renombrar_cuenta_ms(app: AppHandle, id: String, etiqueta: String) -> Result<(), String> {
    let limpia: String = etiqueta.trim().chars().take(MAX_ETIQUETA).collect();
    modificar_cuentas(&app, |l| {
        if let Some(c) = l.iter_mut().find(|c| c.id == id) {
            c.etiqueta = if limpia.is_empty() { etiqueta_defecto(&c.tenant).to_string() } else { limpia.clone() };
        }
    })
    .map_err(|e| e.mensaje)?;
    emitir(&app);
    Ok(())
}

#[tauri::command]
pub async fn probar_cuenta_ms(app: AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || match graph_get(&app, &id, "/me") {
        Ok(v) => {
            let ms = app.state::<Microsoft>();
            let cambia = ms.estados.lock().unwrap().remove(&id).is_some();
            if cambia {
                emitir(&app);
            }
            let quien = v["userPrincipalName"].as_str().or_else(|| v["mail"].as_str()).unwrap_or("");
            Ok(format!("OK · {quien}").trim_end_matches([' ', '·']).to_string())
        }
        Err(e) => {
            marcar_error(&app, &id, &e);
            Err(e.mensaje)
        }
    })
    .await
    .map_err(|_| "Error interno al probar".to_string())?
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn verifier_longitud_y_caracteres() {
        let v = generar_verifier().unwrap();
        assert_eq!(v.len(), 64);
        assert!(v.bytes().all(|b| CARACTERES_PKCE.contains(&b)));
        assert_ne!(v, generar_verifier().unwrap());
    }

    #[test]
    fn challenge_vector_rfc7636() {
        assert_eq!(
            challenge_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn url_de_autorizacion() {
        let u = url_autorizacion("00000000-0000-0000-0000-000000000000", 54321, "est ado", "ch-all_enge");
        assert!(u.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"));
        let url = url::Url::parse(&u).unwrap();
        let p: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(p["client_id"], "00000000-0000-0000-0000-000000000000");
        assert_eq!(p["response_type"], "code");
        assert_eq!(p["redirect_uri"], "http://localhost:54321");
        assert_eq!(p["response_mode"], "query");
        assert_eq!(p["scope"], SCOPES_LOGIN);
        assert_eq!(p["state"], "est ado");
        assert_eq!(p["code_challenge_method"], "S256");
        assert_eq!(p["prompt"], "select_account");
        // Codificado: la redirección y los espacios no van en crudo
        assert!(u.contains("redirect_uri=http%3A%2F%2Flocalhost%3A54321"));
        assert!(!u.contains(' '));
    }

    #[test]
    fn redireccion_con_codigo() {
        let r = leer_redireccion("GET /?code=abc%20def&state=xyz&session_state=1 HTTP/1.1", "xyz");
        assert_eq!(r, Redireccion::Codigo("abc def".into()));
    }

    #[test]
    fn redireccion_con_error_y_state_incorrecto() {
        match leer_redireccion("GET /?error=access_denied&error_description=x&state=xyz HTTP/1.1", "xyz") {
            Redireccion::Error(e) => assert_eq!(e.mensaje, "Cancelaste el inicio de sesión"),
            o => panic!("{o:?}"),
        }
        match leer_redireccion("GET /?code=abc&state=otro HTTP/1.1", "xyz") {
            Redireccion::Error(e) => assert!(e.mensaje.contains("state")),
            o => panic!("{o:?}"),
        }
        match leer_redireccion("GET /?code=abc HTTP/1.1", "xyz") {
            Redireccion::Error(_) => {}
            o => panic!("{o:?}"),
        }
        assert_eq!(leer_redireccion("GET /favicon.ico HTTP/1.1", "xyz"), Redireccion::Ignorar);
    }

    #[test]
    fn mapeo_de_errores() {
        for (c, d) in [
            ("invalid_grant", "AADSTS65001: The user has not consented"),
            ("access_denied", "AADSTS90094: admin consent"),
            ("consent_required", ""),
            ("admin_consent_required", ""),
        ] {
            let e = mapear_error(c, d);
            assert_eq!(e.tipo, TipoError::Bloqueada, "{c}");
            assert!(e.mensaje.contains("administrador"));
        }
        assert_eq!(mapear_error("access_denied", "").mensaje, "Cancelaste el inicio de sesión");
        assert!(mapear_error("invalid_client", "").mensaje.starts_with("El ID de cliente no es válido"));
        assert!(mapear_error("unauthorized_client", "AADSTS700016: no encontrada").mensaje.starts_with("El ID de cliente"));
        assert_eq!(mapear_error("invalid_grant", "AADSTS70000: caducó").tipo, TipoError::Reconectar);
        let e = mapear_error("server_error", "AADSTS51000: algo\r\nTrace ID: 123\r\nCorrelation ID: 456");
        assert_eq!(e.tipo, TipoError::Otro);
        assert_eq!(e.mensaje, "server_error: AADSTS51000: algo");
        assert!(!e.mensaje.contains("Trace"));
        assert_eq!(ErrorMs::tiempo_agotado().mensaje, "No se completó el inicio de sesión a tiempo");
    }

    #[test]
    fn id_token_de_ejemplo() {
        let payload = r#"{"oid":"11111111-1111-1111-1111-111111111111","tid":"22222222-2222-2222-2222-222222222222","name":"Ada Prueba","preferred_username":"ada@example.com"}"#;
        let tok = format!("{}.{}.firma", URL_SAFE_NO_PAD.encode("{\"alg\":\"none\"}"), URL_SAFE_NO_PAD.encode(payload));
        let c = decodificar_id_token(&tok).unwrap();
        assert_eq!(c.name.as_deref(), Some("Ada Prueba"));
        let cuenta = cuenta_desde_claims(&c).unwrap();
        assert_eq!(cuenta.id, "11111111-1111-1111-1111-111111111111.22222222-2222-2222-2222-222222222222");
        assert_eq!(cuenta.correo, "ada@example.com");
        assert_eq!(cuenta.etiqueta, "UAH");
        assert!(decodificar_id_token("basura").is_err());
        assert_eq!(etiqueta_defecto(TENANT_PERSONAL), "PERSONAL");
    }

    #[test]
    fn caducidad_del_token() {
        assert!(!necesita_refresco(1000 + 301, 1000));
        assert!(necesita_refresco(1000 + 299, 1000));
        assert!(necesita_refresco(900, 1000));
    }

    #[test]
    fn troceado_de_secretos() {
        let s: String = "a".repeat(2000);
        let t = partir_secreto(&s, TROZO);
        assert_eq!(t.len(), 3);
        let (n, primero) = cabecera_secreto(&t[0]).unwrap();
        assert_eq!(n, 3);
        let unido = format!("{primero}{}{}", t[1], t[2]);
        assert_eq!(unido, s);
        assert!(cabecera_secreto("basura").is_none());
    }
}
