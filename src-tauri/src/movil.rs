//! Conexión móvil: servidor HTTP mínimo en la red local, APAGADO por defecto, para que la app
//! móvil se empareje con un código y sincronice notas/tareas y lea la agenda.
//! Los datos viajan sin cifrar: pensado solo para redes de confianza. Nunca se escribe en
//! logs ni en errores el token, el código ni el contenido de notas o agenda.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::estado::Estado;
use crate::notas::Datos;

pub const PUERTO_DEFECTO: u16 = 47821;
const MAX_CUERPO: usize = 2 * 1024 * 1024;
const MAX_CABECERAS: usize = 16 * 1024;
const TIMEOUT_LECTURA: Duration = Duration::from_secs(5);
const VIDA_CODIGO: Duration = Duration::from_secs(300);
const MAX_FALLOS: u8 = 5;
const PAUSA_TOKEN: Duration = Duration::from_millis(300);
const ESPERA_ACCEPT: Duration = Duration::from_millis(100);

// ---------------------------------------------------------------------------
// Lógica pura: emparejamiento, parseo y enrutado
// ---------------------------------------------------------------------------

/// Comparación en tiempo constante (la longitud sí puede filtrarse).
fn igual_const(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, PartialEq, Eq)]
pub enum ErrorEmparejar {
    /// Código erróneo, caducado o inexistente (401).
    Codigo,
    /// Demasiados fallos con este código (429).
    Intentos,
    /// No se pudo guardar el token (500).
    Interno,
}

struct Codigo {
    valor: String,
    caduca: Instant,
    fallos: u8,
}

/// Código de emparejamiento vigente (como mucho uno).
#[derive(Default)]
pub struct Emparejamiento {
    actual: Option<Codigo>,
}

impl Emparejamiento {
    /// Genera un código de 6 cifras válido 5 minutos; sustituye al anterior.
    pub fn nuevo(&mut self, ahora: Instant) -> String {
        let b = *uuid::Uuid::new_v4().as_bytes();
        let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) % 1_000_000;
        let valor = format!("{n:06}");
        self.actual = Some(Codigo { valor: valor.clone(), caduca: ahora + VIDA_CODIGO, fallos: 0 });
        valor
    }

    /// Prueba un código. Si coincide se consume; tras 5 fallos queda anulado.
    pub fn probar(&mut self, intento: &str, ahora: Instant) -> Result<(), ErrorEmparejar> {
        let Some(c) = self.actual.as_mut() else { return Err(ErrorEmparejar::Codigo) };
        if ahora >= c.caduca {
            self.actual = None;
            return Err(ErrorEmparejar::Codigo);
        }
        if c.fallos >= MAX_FALLOS {
            return Err(ErrorEmparejar::Intentos);
        }
        if igual_const(&c.valor, intento) {
            self.actual = None;
            return Ok(());
        }
        c.fallos += 1;
        Err(ErrorEmparejar::Codigo)
    }

    /// Código usable y segundos que le quedan (None si no hay, caducó o está anulado).
    pub fn vigente(&self, ahora: Instant) -> Option<(String, u64)> {
        let c = self.actual.as_ref()?;
        (ahora < c.caduca && c.fallos < MAX_FALLOS).then(|| (c.valor.clone(), (c.caduca - ahora).as_secs()))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Cabecera {
    pub metodo: String,
    pub ruta: String,
    pub token: Option<String>,
    pub longitud: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Peticion {
    pub metodo: String,
    pub ruta: String,
    pub token: Option<String>,
    pub cuerpo: Vec<u8>,
}

/// Posición donde empieza el `\r\n\r\n` que cierra las cabeceras.
fn fin_cabeceras(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Interpreta línea de petición y cabeceras (sin el `\r\n\r\n` final). Error = código HTTP.
pub fn parsear_cabecera(texto: &str) -> Result<Cabecera, u16> {
    let mut lineas = texto.split("\r\n");
    let mut partes = lineas.next().unwrap_or("").split_whitespace();
    let (Some(metodo), Some(destino), Some(version), None) = (partes.next(), partes.next(), partes.next(), partes.next())
    else {
        return Err(400);
    };
    if !version.starts_with("HTTP/") {
        return Err(400);
    }
    let ruta = destino.split('?').next().unwrap_or("").to_string();
    let (mut token, mut longitud) = (None, 0usize);
    for l in lineas {
        let Some((nombre, valor)) = l.split_once(':') else { continue };
        let valor = valor.trim();
        match nombre.trim().to_ascii_lowercase().as_str() {
            "content-length" => longitud = valor.parse().map_err(|_| 400u16)?,
            "authorization" => {
                if valor.len() > 7 && valor[..7].eq_ignore_ascii_case("bearer ") {
                    token = Some(valor[7..].trim().to_string());
                }
            }
            _ => {}
        }
    }
    if longitud > MAX_CUERPO {
        return Err(413);
    }
    Ok(Cabecera { metodo: metodo.to_string(), ruta, token, longitud })
}

pub struct Respuesta {
    pub codigo: u16,
    pub cuerpo: String,
}

fn error(codigo: u16, clave: &str) -> Respuesta {
    Respuesta { codigo, cuerpo: json!({ "error": clave }).to_string() }
}

fn ok(valor: Value) -> Respuesta {
    Respuesta { codigo: 200, cuerpo: valor.to_string() }
}

/// Lo que el enrutado necesita del panel (en las pruebas, un doble).
pub trait Servicio {
    fn token_valido(&self, candidato: &str) -> bool;
    fn emparejar(&self, codigo: &str) -> Result<String, ErrorEmparejar>;
    fn sincronizar(&self, datos: Datos) -> Result<Datos, String>;
    fn agenda(&self) -> Value;
}

pub fn enrutar(p: &Peticion, s: &dyn Servicio) -> Respuesta {
    if p.metodo == "POST" && p.ruta == "/emparejar" {
        let codigo = serde_json::from_slice::<Value>(&p.cuerpo)
            .ok()
            .and_then(|v| v.get("codigo")?.as_str().map(str::to_string));
        let Some(codigo) = codigo else { return error(400, "datos") };
        return match s.emparejar(&codigo) {
            Ok(token) => ok(json!({ "token": token })),
            Err(ErrorEmparejar::Codigo) => error(401, "codigo"),
            Err(ErrorEmparejar::Intentos) => error(429, "intentos"),
            Err(ErrorEmparejar::Interno) => error(500, "interno"),
        };
    }
    if !p.token.as_deref().is_some_and(|t| s.token_valido(t)) {
        return error(401, "token");
    }
    match (p.metodo.as_str(), p.ruta.as_str()) {
        ("GET", "/estado") => ok(json!({ "app": "netrunner-panel", "version": env!("CARGO_PKG_VERSION") })),
        ("GET", "/agenda") => ok(s.agenda()),
        ("POST", "/notas/sincronizar") => match serde_json::from_slice::<Datos>(&p.cuerpo) {
            Err(_) => error(400, "datos"),
            Ok(datos) => match s.sincronizar(datos) {
                Ok(d) => ok(serde_json::to_value(d).unwrap_or(Value::Null)),
                Err(_) => error(500, "interno"),
            },
        },
        _ => error(404, "ruta"),
    }
}

// ---------------------------------------------------------------------------
// E/S: lectura de la petición y escritura de la respuesta
// ---------------------------------------------------------------------------

/// Lee un trozo respetando el plazo total (evita conexiones que gotean bytes).
fn leer(s: &mut TcpStream, buf: &mut [u8], limite: Instant) -> Result<usize, u16> {
    let resto = limite.saturating_duration_since(Instant::now());
    if resto.is_zero() {
        return Err(408);
    }
    let _ = s.set_read_timeout(Some(resto));
    match s.read(buf) {
        Ok(0) => Err(400),
        Ok(n) => Ok(n),
        Err(_) => Err(408),
    }
}

fn leer_peticion(s: &mut TcpStream) -> Result<Peticion, u16> {
    let limite = Instant::now() + TIMEOUT_LECTURA;
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];
    let fin = loop {
        if let Some(i) = fin_cabeceras(&buf) {
            break i;
        }
        if buf.len() > MAX_CABECERAS {
            return Err(431);
        }
        let n = leer(s, &mut tmp, limite)?;
        buf.extend_from_slice(&tmp[..n]);
    };
    if fin > MAX_CABECERAS {
        return Err(431);
    }
    let cab = parsear_cabecera(&String::from_utf8_lossy(&buf[..fin]))?;
    let mut cuerpo = buf.split_off(fin + 4);
    while cuerpo.len() < cab.longitud {
        let n = leer(s, &mut tmp, limite)?;
        cuerpo.extend_from_slice(&tmp[..n]);
    }
    cuerpo.truncate(cab.longitud);
    Ok(Peticion { metodo: cab.metodo, ruta: cab.ruta, token: cab.token, cuerpo })
}

fn texto_estado(codigo: u16) -> &'static str {
    match codigo {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    }
}

fn escribir(s: &mut TcpStream, r: &Respuesta) {
    let cab = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        r.codigo,
        texto_estado(r.codigo),
        r.cuerpo.len()
    );
    let _ = s.write_all(cab.as_bytes());
    let _ = s.write_all(r.cuerpo.as_bytes());
    let _ = s.flush();
}

fn atender(mut s: TcpStream, servicio: &dyn Servicio) {
    let _ = s.set_nonblocking(false);
    let _ = s.set_write_timeout(Some(TIMEOUT_LECTURA));
    let r = match leer_peticion(&mut s) {
        Ok(p) => enrutar(&p, servicio),
        Err(c) => error(c, match c {
            408 => "tiempo",
            413 => "tamano",
            431 => "cabeceras",
            _ => "peticion",
        }),
    };
    if r.codigo == 401 && r.cuerpo.contains("\"token\"") {
        thread::sleep(PAUSA_TOKEN); // frena la fuerza bruta
    }
    escribir(&mut s, &r);
}

// ---------------------------------------------------------------------------
// Servicio real, ciclo de vida y comandos
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Movil {
    emparejamiento: Mutex<Emparejamiento>,
    /// Bandera de parada del servidor en marcha.
    servidor: Mutex<Option<Arc<AtomicBool>>>,
    error: Mutex<Option<String>>,
}

struct Real<'a>(&'a AppHandle);

impl Servicio for Real<'_> {
    fn token_valido(&self, candidato: &str) -> bool {
        let estado = self.0.state::<Estado>();
        let c = estado.config.lock().unwrap();
        c.movil_token.as_deref().is_some_and(|t| igual_const(t, candidato))
    }

    fn emparejar(&self, codigo: &str) -> Result<String, ErrorEmparejar> {
        self.0.state::<Movil>().emparejamiento.lock().unwrap().probar(codigo, Instant::now())?;
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let estado = self.0.state::<Estado>();
        let mut c = estado.config.lock().unwrap();
        let anterior = c.movil_token.replace(token.clone());
        if crate::config::guardar(self.0, &c).is_err() {
            c.movil_token = anterior;
            return Err(ErrorEmparejar::Interno);
        }
        Ok(token)
    }

    fn sincronizar(&self, datos: Datos) -> Result<Datos, String> {
        crate::notas::fusionar_desde_movil(self.0, datos)
    }

    fn agenda(&self) -> Value {
        let cal = crate::calendario::estado_calendario(self.0.state());
        json!({
            "eventos": cal.eventos,
            "cuentas": crate::cuentas::estado_cuentas(self.0.clone()),
            "ultima_sync": cal.ultima_sync,
        })
    }
}

fn servir(listener: TcpListener, parar: Arc<AtomicBool>, app: AppHandle) {
    let _ = listener.set_nonblocking(true);
    while !parar.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((s, _)) => {
                // Un fallo de una conexión (incluso un pánico) no debe tumbar el servidor
                if catch_unwind(AssertUnwindSafe(|| atender(s, &Real(&app)))).is_err() {
                    eprintln!("movil: error interno al atender una conexión");
                }
            }
            Err(_) => thread::sleep(ESPERA_ACCEPT),
        }
    }
}

fn parar(app: &AppHandle) {
    let m = app.state::<Movil>();
    let bandera = m.servidor.lock().unwrap().take();
    if let Some(b) = bandera {
        b.store(true, Ordering::SeqCst);
    }
}

/// (Re)arranca el servidor con el puerto de la configuración. Si no puede abrirlo, lo anota
/// en `error` (lo muestra AJUSTES).
fn arrancar(app: &AppHandle) {
    parar(app);
    let m = app.state::<Movil>();
    let puerto = match app.state::<Estado>().config.lock().unwrap().movil_puerto {
        0 => PUERTO_DEFECTO,
        p => p,
    };
    // Si se acaba de parar, el hilo anterior tarda unos 100 ms en soltar el puerto
    let mut res = TcpListener::bind(("0.0.0.0", puerto));
    for _ in 0..10 {
        if !matches!(&res, Err(e) if e.kind() == std::io::ErrorKind::AddrInUse) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
        res = TcpListener::bind(("0.0.0.0", puerto));
    }
    match res {
        Ok(listener) => {
            *m.error.lock().unwrap() = None;
            let bandera = Arc::new(AtomicBool::new(false));
            *m.servidor.lock().unwrap() = Some(bandera.clone());
            let h = app.clone();
            thread::spawn(move || servir(listener, bandera, h));
        }
        Err(e) => {
            *m.error.lock().unwrap() = Some(if e.kind() == std::io::ErrorKind::AddrInUse {
                format!("El puerto {puerto} está ocupado por otro programa")
            } else {
                format!("No se pudo abrir el puerto {puerto}")
            });
        }
    }
}

pub fn iniciar(app: &AppHandle) {
    app.manage(Movil::default());
    if app.state::<Estado>().config.lock().unwrap().movil_activo {
        let h = app.clone();
        thread::spawn(move || arrancar(&h));
    }
}

/// IPv4 local principal: un UDP «conectado» a una IP pública (no envía nada) revela la interfaz de salida.
fn ip_local() -> Option<String> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then(|| ip.to_string())
}

#[derive(Serialize)]
pub struct EstadoMovil {
    pub activo: bool,
    pub puerto: u16,
    pub ip: Option<String>,
    pub emparejado: bool,
    pub codigo: Option<String>,
    pub caduca_en_s: Option<u64>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn estado_movil(app: AppHandle) -> EstadoMovil {
    let (activo, puerto, emparejado) = {
        let estado = app.state::<Estado>();
        let c = estado.config.lock().unwrap();
        (c.movil_activo, c.movil_puerto, c.movil_token.is_some())
    };
    let m = app.state::<Movil>();
    let vigente = if activo { m.emparejamiento.lock().unwrap().vigente(Instant::now()) } else { None };
    let error = m.error.lock().unwrap().clone();
    EstadoMovil {
        activo,
        puerto,
        ip: ip_local(),
        emparejado,
        caduca_en_s: vigente.as_ref().map(|v| v.1),
        codigo: vigente.map(|v| v.0),
        error,
    }
}

#[tauri::command]
pub fn activar_movil(app: AppHandle, activo: bool) -> Result<(), String> {
    {
        let estado = app.state::<Estado>();
        let mut c = estado.config.lock().unwrap();
        c.movil_activo = activo;
        crate::config::guardar(&app, &c)?;
    }
    if activo {
        arrancar(&app);
    } else {
        parar(&app);
        let m = app.state::<Movil>();
        *m.emparejamiento.lock().unwrap() = Emparejamiento::default();
        *m.error.lock().unwrap() = None;
    }
    Ok(())
}

#[tauri::command]
pub fn nuevo_codigo_movil(app: AppHandle) -> String {
    app.state::<Movil>().emparejamiento.lock().unwrap().nuevo(Instant::now())
}

#[tauri::command]
pub fn desvincular_movil(app: AppHandle) -> Result<(), String> {
    let estado = app.state::<Estado>();
    let mut c = estado.config.lock().unwrap();
    let anterior = c.movil_token.take();
    crate::config::guardar(&app, &c).map_err(|e| {
        c.movil_token = anterior;
        e
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    struct Doble;
    impl Servicio for Doble {
        fn token_valido(&self, t: &str) -> bool {
            t == "tok"
        }
        fn emparejar(&self, codigo: &str) -> Result<String, ErrorEmparejar> {
            match codigo {
                "111111" => Ok("nuevo".into()),
                "999999" => Err(ErrorEmparejar::Intentos),
                _ => Err(ErrorEmparejar::Codigo),
            }
        }
        fn sincronizar(&self, datos: Datos) -> Result<Datos, String> {
            Ok(datos)
        }
        fn agenda(&self) -> Value {
            json!({ "eventos": [], "cuentas": [], "ultima_sync": null })
        }
    }

    fn pet(metodo: &str, ruta: &str, token: Option<&str>, cuerpo: &str) -> Peticion {
        Peticion { metodo: metodo.into(), ruta: ruta.into(), token: token.map(Into::into), cuerpo: cuerpo.as_bytes().to_vec() }
    }

    #[test]
    fn peticion_bien_formada() {
        let c = parsear_cabecera("POST /notas/sincronizar?x=1 HTTP/1.1\r\nHost: a\r\nContent-Length: 12\r\nAuthorization: Bearer abc").unwrap();
        assert_eq!(c, Cabecera { metodo: "POST".into(), ruta: "/notas/sincronizar".into(), token: Some("abc".into()), longitud: 12 });
    }

    #[test]
    fn cabeceras_en_minusculas() {
        let c = parsear_cabecera("GET /estado HTTP/1.1\r\nauthorization: bearer xyz\r\ncontent-length: 0").unwrap();
        assert_eq!(c.token.as_deref(), Some("xyz"));
        assert_eq!(c.longitud, 0);
    }

    #[test]
    fn cabecera_mal_formada() {
        assert_eq!(parsear_cabecera("GET /estado"), Err(400));
        assert_eq!(parsear_cabecera("GET / HTTP/1.1\r\nContent-Length: x"), Err(400));
        assert!(parsear_cabecera("GET / HTTP/1.1\r\nAuthorization: Basic abc").unwrap().token.is_none());
    }

    #[test]
    fn cuerpo_demasiado_grande_es_413() {
        let t = format!("POST /notas/sincronizar HTTP/1.1\r\nContent-Length: {}", MAX_CUERPO + 1);
        assert_eq!(parsear_cabecera(&t), Err(413));
        let t = format!("POST /notas/sincronizar HTTP/1.1\r\nContent-Length: {}", MAX_CUERPO);
        assert!(parsear_cabecera(&t).is_ok());
    }

    #[test]
    fn sin_token_o_con_token_malo_es_401() {
        for t in [None, Some("malo")] {
            let r = enrutar(&pet("GET", "/estado", t, ""), &Doble);
            assert_eq!((r.codigo, r.cuerpo.as_str()), (401, r#"{"error":"token"}"#));
        }
        assert_eq!(enrutar(&pet("GET", "/nada", None, ""), &Doble).codigo, 401);
    }

    #[test]
    fn token_bueno_pasa() {
        let r = enrutar(&pet("GET", "/estado", Some("tok"), ""), &Doble);
        assert_eq!(r.codigo, 200);
        let v: Value = serde_json::from_str(&r.cuerpo).unwrap();
        assert_eq!(v["app"], "netrunner-panel");
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(enrutar(&pet("GET", "/agenda", Some("tok"), ""), &Doble).codigo, 200);
        let r = enrutar(&pet("GET", "/nada", Some("tok"), ""), &Doble);
        assert_eq!((r.codigo, r.cuerpo.as_str()), (404, r#"{"error":"ruta"}"#));
        assert_eq!(enrutar(&pet("POST", "/estado", Some("tok"), ""), &Doble).codigo, 404);
    }

    #[test]
    fn emparejar_sin_token() {
        let r = enrutar(&pet("POST", "/emparejar", None, r#"{"codigo":"111111"}"#), &Doble);
        assert_eq!((r.codigo, r.cuerpo.as_str()), (200, r#"{"token":"nuevo"}"#));
        assert_eq!(enrutar(&pet("POST", "/emparejar", None, r#"{"codigo":"000000"}"#), &Doble).codigo, 401);
        assert_eq!(enrutar(&pet("POST", "/emparejar", None, r#"{"codigo":"999999"}"#), &Doble).codigo, 429);
        assert_eq!(enrutar(&pet("POST", "/emparejar", None, "no json"), &Doble).codigo, 400);
    }

    #[test]
    fn sincronizar_datos_validos_e_invalidos() {
        let cuerpo = r#"{"tareas":[{"id":"a","texto":"x","modificada":5,"borrada":true}],"notas":[]}"#;
        let r = enrutar(&pet("POST", "/notas/sincronizar", Some("tok"), cuerpo), &Doble);
        assert_eq!(r.codigo, 200);
        let d: Datos = serde_json::from_str(&r.cuerpo).unwrap();
        assert!(d.tareas[0].borrada);
        for malo in ["no es json", "[1,2]", r#"{"tareas":"x"}"#, ""] {
            let r = enrutar(&pet("POST", "/notas/sincronizar", Some("tok"), malo), &Doble);
            assert_eq!((r.codigo, r.cuerpo.as_str()), (400, r#"{"error":"datos"}"#));
        }
    }

    #[test]
    fn codigo_correcto_se_consume() {
        let (mut e, t0) = (Emparejamiento::default(), Instant::now());
        let c = e.nuevo(t0);
        assert_eq!(c.len(), 6);
        assert!(c.bytes().all(|b| b.is_ascii_digit()));
        assert_eq!(e.probar(&c, t0), Ok(()));
        assert_eq!(e.probar(&c, t0), Err(ErrorEmparejar::Codigo));
    }

    #[test]
    fn codigo_erroneo_o_caducado() {
        let (mut e, t0) = (Emparejamiento::default(), Instant::now());
        assert_eq!(e.probar("123456", t0), Err(ErrorEmparejar::Codigo));
        let c = e.nuevo(t0);
        let malo = if c == "000000" { "000001" } else { "000000" };
        assert_eq!(e.probar(malo, t0), Err(ErrorEmparejar::Codigo));
        assert!(e.vigente(t0).is_some());
        let tarde = t0 + VIDA_CODIGO;
        assert!(e.vigente(tarde).is_none());
        assert_eq!(e.probar(&c, tarde), Err(ErrorEmparejar::Codigo));
    }

    #[test]
    fn cinco_fallos_anulan_el_codigo() {
        let (mut e, t0) = (Emparejamiento::default(), Instant::now());
        let c = e.nuevo(t0);
        let malo = if c == "000000" { "000001" } else { "000000" };
        for _ in 0..MAX_FALLOS {
            assert_eq!(e.probar(malo, t0), Err(ErrorEmparejar::Codigo));
        }
        assert_eq!(e.probar(&c, t0), Err(ErrorEmparejar::Intentos));
        assert!(e.vigente(t0).is_none());
        // un código nuevo vuelve a funcionar
        let n = e.nuevo(t0);
        assert_eq!(e.probar(&n, t0), Ok(()));
    }

    #[test]
    fn comparacion_constante() {
        assert!(igual_const("abc", "abc"));
        assert!(!igual_const("abc", "abd"));
        assert!(!igual_const("abc", "abcd"));
    }

    #[test]
    fn lectura_por_tcp_con_cuerpo_en_varios_trozos() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let cli = thread::spawn(move || {
            let mut c = TcpStream::connect(addr).unwrap();
            c.write_all(b"POST /emparejar HTTP/1.1\r\ncontent-length: 19\r\n\r\n{\"codigo\"").unwrap();
            c.flush().unwrap();
            thread::sleep(Duration::from_millis(50));
            c.write_all(b":\"111111\"}").unwrap();
            let mut resp = String::new();
            c.read_to_string(&mut resp).unwrap();
            resp
        });
        let (s, _) = l.accept().unwrap();
        atender(s, &Doble);
        let resp = cli.join().unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 OK"), "{resp}");
        assert!(resp.contains("Connection: close") && resp.contains("Cache-Control: no-store"));
        assert!(resp.ends_with(r#"{"token":"nuevo"}"#));
    }
}
