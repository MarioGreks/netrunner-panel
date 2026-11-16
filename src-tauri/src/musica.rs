//! Módulo MÚSICA: controles multimedia de Windows (SMTC).
//!
//! Un hilo consulta la sesión actual cada segundo, pero solo mientras el panel está
//! visible. La carátula se lee únicamente cuando cambia la pista. Cualquier error de
//! WinRT se degrada a «nada sonando» (`None`) o a un error breve.

use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Sesion,
    GlobalSystemMediaTransportControlsSessionManager as Gestor,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as EstadoRep,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use crate::estado::Estado;

const TICKS_POR_S: i64 = 10_000_000;
/// Diferencia entre la época de Windows (1601) y la de Unix, en ticks de 100 ns.
const TICKS_EPOCA_UNIX: i64 = 116_444_736_000_000_000;
const MAX_CARATULA: u64 = 4 * 1024 * 1024;
/// Desvío tolerado entre la posición esperada y la real antes de volver a emitir.
const TOLERANCIA_POSICION: f64 = 2.5;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct EstadoMusica {
    pub titulo: String,
    pub artista: String,
    pub album: String,
    /// Nombre legible de la app de origen («SPOTIFY», «NAVEGADOR»…).
    pub origen: String,
    pub reproduciendo: bool,
    /// Segundos
    pub posicion: f64,
    pub duracion: f64,
    pub puede_anterior: bool,
    pub puede_siguiente: bool,
    /// `data:image/...;base64,...` o `None` si la pista no tiene carátula.
    pub caratula: Option<String>,
}

// ---------------------------------------------------------------------------
// Lógica pura
// ---------------------------------------------------------------------------

/// Traduce el `SourceAppUserModelId` a una etiqueta corta y legible.
pub fn nombre_origen(id: &str) -> String {
    let bajo = id.to_lowercase();
    if bajo.contains("spotify") {
        return "SPOTIFY".into();
    }
    const NAVEGADORES: [&str; 7] = ["chrome", "msedge", "firefox", "opera", "brave", "vivaldi", "iexplore"];
    if NAVEGADORES.iter().any(|n| bajo.contains(n)) {
        return "NAVEGADOR".into();
    }
    // Recorta: sin ruta, sin .exe y, en ids de apps de la tienda («Editor.App_hash!App»),
    // solo el primer componente del nombre del paquete.
    let mut t = id.trim();
    if let Some(i) = t.rfind(['\\', '/']) {
        t = &t[i + 1..];
    }
    if let Some(i) = t.find('!') {
        t = &t[..i];
    }
    if let Some(i) = t.find('_') {
        t = &t[..i];
    }
    let t = if t.to_lowercase().ends_with(".exe") { &t[..t.len() - 4] } else { t };
    // «Microsoft.ZuneMusic» -> «Microsoft»: se prefiere el último componente si hay puntos
    let t = t.rsplit('.').find(|s| !s.is_empty()).unwrap_or(t);
    if t.is_empty() {
        return "DESCONOCIDO".into();
    }
    t.chars().take(14).collect::<String>().to_uppercase()
}

/// Tipo MIME de una imagen según sus primeros bytes.
pub fn mime_imagen(b: &[u8]) -> &'static str {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if b.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if b.starts_with(b"BM") {
        "image/bmp"
    } else if b.starts_with(b"GIF8") {
        "image/gif"
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// ¿Hay que emitir? Todo salvo la posición debe ser igual y la posición debe coincidir
/// (con tolerancia) con la que la interfaz ya habrá interpolado.
fn cambio_relevante(anterior: &Option<(EstadoMusica, Instant)>, nuevo: &Option<EstadoMusica>, ahora: Instant) -> bool {
    match (anterior, nuevo) {
        (None, None) => false,
        (Some((a, t)), Some(n)) => {
            let mut a2 = a.clone();
            a2.posicion = n.posicion;
            if a2 != *n {
                return true;
            }
            let esperada = if a.reproduciendo {
                let avance = a.posicion + ahora.duration_since(*t).as_secs_f64();
                if a.duracion > 0.0 { avance.min(a.duracion) } else { avance }
            } else {
                a.posicion
            };
            (esperada - n.posicion).abs() > TOLERANCIA_POSICION
        }
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// Lectura de WinRT
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Interno {
    gestor: Option<Gestor>,
    /// Última emisión y cuándo se hizo.
    ultimo: Option<(EstadoMusica, Instant)>,
    /// Clave de la pista cuya carátula está en caché.
    clave_caratula: Option<String>,
    caratula: Option<String>,
}

// El gestor de SMTC es un objeto agile de WinRT: se puede usar desde cualquier hilo.
unsafe impl Send for Interno {}

pub struct Musica {
    interno: Mutex<Interno>,
}

fn leer_caratula(r: IRandomAccessStreamReference) -> Option<String> {
    let flujo = r.OpenReadAsync().ok()?.join().ok()?;
    let tam = flujo.Size().ok()?;
    if tam == 0 || tam > MAX_CARATULA {
        return None;
    }
    let lector = DataReader::CreateDataReader(&flujo).ok()?;
    lector.LoadAsync(tam as u32).ok()?.join().ok()?;
    let mut buf = vec![0u8; tam as usize];
    lector.ReadBytes(&mut buf).ok()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    Some(format!("data:{};base64,{}", mime_imagen(&buf), b64))
}

fn sesion_actual(g: &Gestor) -> Option<Sesion> {
    g.GetCurrentSession().ok()
}

fn ahora_ticks_windows() -> i64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_nanos() / 100) as i64 + TICKS_EPOCA_UNIX
}

/// Lee la sesión actual. `None` si no hay nada sonando o algo falla.
fn leer(i: &mut Interno) -> Option<EstadoMusica> {
    let sesion = sesion_actual(i.gestor.as_ref()?)?;
    let props = sesion.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let titulo = props.Title().ok()?.to_string();
    let artista = props.Artist().map(|s| s.to_string()).unwrap_or_default();
    let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
    if titulo.trim().is_empty() && artista.trim().is_empty() {
        return None;
    }
    let origen = nombre_origen(&sesion.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default());

    let info = sesion.GetPlaybackInfo().ok()?;
    let reproduciendo = info.PlaybackStatus().ok()? == EstadoRep::Playing;
    let (mut puede_anterior, mut puede_siguiente) = (false, false);
    if let Ok(c) = info.Controls() {
        puede_anterior = c.IsPreviousEnabled().unwrap_or(false);
        puede_siguiente = c.IsNextEnabled().unwrap_or(false);
    }

    let (mut posicion, mut duracion) = (0.0, 0.0);
    if let Ok(t) = sesion.GetTimelineProperties() {
        let ini = t.StartTime().map(|d| d.Duration).unwrap_or(0);
        let fin = t.EndTime().map(|d| d.Duration).unwrap_or(0);
        let pos = t.Position().map(|d| d.Duration).unwrap_or(0);
        let mut pos_ticks = (pos - ini).max(0);
        // La posición es una instantánea: si está sonando, se avanza desde su marca
        if reproduciendo {
            if let Ok(u) = t.LastUpdatedTime() {
                let transcurrido = (ahora_ticks_windows() - u.UniversalTime).clamp(0, 6 * 3600 * TICKS_POR_S);
                pos_ticks += transcurrido;
            }
        }
        duracion = ((fin - ini).max(0)) as f64 / TICKS_POR_S as f64;
        posicion = pos_ticks as f64 / TICKS_POR_S as f64;
        if duracion > 0.0 {
            posicion = posicion.min(duracion);
        }
    }

    // Carátula: solo cuando cambia la pista
    let clave = format!("{origen}\u{1}{titulo}\u{1}{artista}\u{1}{album}");
    if i.clave_caratula.as_deref() != Some(clave.as_str()) {
        i.caratula = props.Thumbnail().ok().and_then(leer_caratula);
        i.clave_caratula = Some(clave);
    }

    Some(EstadoMusica {
        titulo,
        artista,
        album,
        origen,
        reproduciendo,
        posicion,
        duracion,
        puede_anterior,
        puede_siguiente,
        caratula: i.caratula.clone(),
    })
}

fn asegurar_gestor(i: &mut Interno) {
    if i.gestor.is_none() {
        i.gestor = Gestor::RequestAsync().ok().and_then(|o| o.join().ok());
    }
}

/// Lee, y emite si algo cambió (o siempre si `forzar`).
fn refrescar(app: &AppHandle, forzar: bool) {
    let m = app.state::<Musica>();
    let ahora = Instant::now();
    let (emitir, nuevo) = {
        let Ok(mut i) = m.interno.lock() else { return };
        asegurar_gestor(&mut i);
        let nuevo = leer(&mut i);
        let cambia = forzar || cambio_relevante(&i.ultimo, &nuevo, ahora);
        if cambia {
            i.ultimo = nuevo.clone().map(|n| (n, ahora));
        }
        (cambia, nuevo)
    };
    if emitir {
        let _ = app.emit("musica://estado", nuevo);
    }
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn estado_musica(app: AppHandle) -> Option<EstadoMusica> {
    // Lo último emitido: no bloquea el hilo principal con llamadas WinRT
    let m = app.state::<Musica>();
    let i = m.interno.lock().ok()?;
    i.ultimo.as_ref().map(|(e, _)| e.clone())
}

fn ordenar(app: AppHandle, accion: fn(&Sesion) -> windows::core::Result<bool>) {
    thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let sesion = {
            let m = app.state::<Musica>();
            let Ok(mut i) = m.interno.lock() else { return };
            asegurar_gestor(&mut i);
            i.gestor.as_ref().and_then(sesion_actual)
        };
        if let Some(s) = sesion {
            let _ = accion(&s);
        }
        // Los reproductores tardan un instante en reflejar el cambio
        thread::sleep(Duration::from_millis(150));
        refrescar(&app, true);
    });
}

#[tauri::command]
pub fn musica_alternar(app: AppHandle) {
    ordenar(app, |s| s.TryTogglePlayPauseAsync()?.join());
}

#[tauri::command]
pub fn musica_siguiente(app: AppHandle) {
    ordenar(app, |s| s.TrySkipNextAsync()?.join());
}

#[tauri::command]
pub fn musica_anterior(app: AppHandle) {
    ordenar(app, |s| s.TrySkipPreviousAsync()?.join());
}

// ---------------------------------------------------------------------------
// Hilo
// ---------------------------------------------------------------------------

pub fn iniciar(app: &AppHandle) {
    app.manage(Musica { interno: Mutex::new(Interno::default()) });
    let app = app.clone();
    thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut estaba_visible = false;
        loop {
            let visible = app.state::<Estado>().panel_visible.load(Ordering::SeqCst);
            if visible {
                // Al abrir el panel se emite siempre, para que la interfaz parta de lo real
                refrescar(&app, !estaba_visible);
            }
            estaba_visible = visible;
            thread::sleep(Duration::from_secs(1));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotify_y_navegadores() {
        assert_eq!(nombre_origen("Spotify.exe"), "SPOTIFY");
        assert_eq!(nombre_origen("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "SPOTIFY");
        assert_eq!(nombre_origen("chrome.exe"), "NAVEGADOR");
        assert_eq!(nombre_origen("MSEdge"), "NAVEGADOR");
        assert_eq!(nombre_origen("firefox.exe"), "NAVEGADOR");
    }

    #[test]
    fn resto_recortado() {
        assert_eq!(nombre_origen("vlc.exe"), "VLC");
        assert_eq!(nombre_origen("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "ZUNEMUSIC");
        assert_eq!(nombre_origen(r"C:\Apps\foobar2000.exe"), "FOOBAR2000");
        assert_eq!(nombre_origen(""), "DESCONOCIDO");
        assert!(nombre_origen("unnombremuylargodeaplicacion.exe").chars().count() <= 14);
    }

    #[test]
    fn mime_por_cabecera() {
        assert_eq!(mime_imagen(&[0x89, b'P', b'N', b'G', 1]), "image/png");
        assert_eq!(mime_imagen(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
    }

    #[test]
    fn posicion_no_re_emite_si_avanza_como_se_esperaba() {
        let t0 = Instant::now();
        let base = EstadoMusica {
            titulo: "a".into(), artista: "b".into(), album: "".into(), origen: "X".into(),
            reproduciendo: true, posicion: 10.0, duracion: 100.0,
            puede_anterior: true, puede_siguiente: true, caratula: None,
        };
        let mut n = base.clone();
        n.posicion = 13.0;
        let ant = Some((base.clone(), t0));
        assert!(!cambio_relevante(&ant, &Some(n.clone()), t0 + Duration::from_secs(3)));
        assert!(cambio_relevante(&ant, &Some(n), t0 + Duration::from_secs(10)));
    }
}
