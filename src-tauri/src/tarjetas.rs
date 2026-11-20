//! Tarjetas desacopladas: una tarjeta del panel (METEO, NOTAS, SISTEMA) arrastrada fuera se
//! convierte en una ventana HUD propia (`tarjeta.html?tipo=<tipo>`, etiqueta `tarjeta-<tipo>`).
//!
//! - Recuerda qué ventanas estaban abiertas y su geometría en `app_data_dir()/tarjetas.json`
//!   (mismo antirrebote de 1 s que la ventana de CONSUMO) y las reabre al arrancar.
//! - Durante el arrastre el panel NO se cierra solo (ni por cursor fuera ni por perder el foco):
//!   lo avisa la página con `arrastre_tarjeta`, con una salvaguarda de 15 s.
//! - La ventana «fantasma» (`fantasma.html`) acompaña al cursor fuera del panel. Es transparente
//!   y no recibe ratón ni foco; se crea oculta al arrancar para que mostrarla sea inmediato.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

use crate::consumo::ventana::{ajustar_a_monitores, monitores, traer_al_frente, Antirrebote, Geometria};

const FICHERO: &str = "tarjetas.json";
const FANTASMA: &str = "fantasma";
/// Tipos de tarjeta con ventana propia y su tamaño lógico por defecto.
const TIPOS: [(&str, f64, f64); 3] = [("meteo", 760.0, 520.0), ("notas", 820.0, 560.0), ("sistema", 860.0, 600.0)];
const MIN_ANCHO: f64 = 480.0;
const MIN_ALTO: f64 = 360.0;
/// Dónde queda el cursor respecto a la esquina de la ventana al soltar (dentro de la barra de título).
const AGARRE: (f64, f64) = (40.0, 16.0);
/// Máximo tiempo con el arrastre activo: si la página falla y no lo apaga, se apaga solo.
const MAX_ARRASTRE: Duration = Duration::from_secs(15);
const FANTASMA_MAX: (f64, f64) = (420.0, 300.0);
/// Dónde queda el cursor sobre el fantasma (sobre su cabecera), en píxeles lógicos.
const AGARRE_FANTASMA: (f64, f64) = (60.0, 18.0);

/// Lo que se guarda en disco.
#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Datos {
    #[serde(default)]
    abiertas: Vec<String>,
    #[serde(default)]
    geometrias: HashMap<String, Geometria>,
}

/// Estado compartido (se registra con `app.manage` en `iniciar`).
pub struct Tarjetas {
    datos: Mutex<Datos>,
    /// Desde cuándo está activo el arrastre (None = no hay arrastre).
    arrastre: Mutex<Option<Instant>>,
    /// Ventanas cerradas por el usuario (CloseRequested): solo esas salen de «abiertas».
    /// Al salir de la app las ventanas se destruyen sin pasar por ahí y deben reabrirse.
    cerrando: Mutex<HashSet<String>>,
    rebote: Arc<Antirrebote>,
}

fn bloquear<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Frontera de confianza: solo se aceptan los tipos de la lista.
fn tipo_valido(tipo: &str) -> Option<(&'static str, f64, f64)> {
    TIPOS.iter().copied().find(|(t, ..)| *t == tipo)
}

fn etiqueta(tipo: &str) -> String {
    format!("tarjeta-{tipo}")
}

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FICHERO))
}

fn leer(app: &AppHandle) -> Datos {
    let texto = ruta(app).and_then(|r| std::fs::read_to_string(r).ok());
    let mut d: Datos = texto.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    // El fichero es editable: se descarta lo que no sea un tipo conocido
    d.abiertas.retain(|t| tipo_valido(t).is_some());
    d.abiertas.dedup();
    d
}

fn escribir(app: &AppHandle, d: &Datos) {
    if let Some(r) = ruta(app) {
        let _ = crate::sync::escribir_atomico(&r, d);
    }
}

/// Geometría con la que se abre la ventana. Con cursor: la esquina queda para que el cursor caiga
/// en la barra de título (tamaño guardado o el de por defecto). Sin cursor: la guardada, o None
/// (el llamador la centra).
fn geometria_inicial(
    cursor: Option<(f64, f64)>,
    guardada: Option<Geometria>,
    defecto: (f64, f64),
    escala: f64,
) -> Option<Geometria> {
    let Some((cx, cy)) = cursor else { return guardada };
    let (ancho, alto) = guardada
        .map(|g| (g.ancho, g.alto))
        .unwrap_or(((defecto.0 * escala) as u32, (defecto.1 * escala) as u32));
    Some(Geometria {
        x: (cx - AGARRE.0 * escala).round() as i32,
        y: (cy - AGARRE.1 * escala).round() as i32,
        ancho,
        alto,
    })
}

/// Factor de escala del monitor bajo el punto (físico); 1.0 si no se sabe.
fn escala_en(app: &AppHandle, x: f64, y: f64) -> f64 {
    app.monitor_from_point(x, y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .map_or(1.0, |m| m.scale_factor())
}

fn lista_abiertas(app: &AppHandle) -> Vec<String> {
    bloquear(&app.state::<Tarjetas>().datos).abiertas.clone()
}

fn avisar_cambio(app: &AppHandle) {
    let _ = app.emit("tarjetas://cambio", lista_abiertas(app));
}

/// Guarda la geometría de todas las ventanas abiertas (nunca minimizadas ni maximizadas: en
/// Windows la posición sería -32000 y se perdería la buena) y la lista de abiertas.
fn guardar_todas(app: &AppHandle) {
    let Some(estado) = app.try_state::<Tarjetas>() else { return };
    let mut d = bloquear(&estado.datos);
    for (tipo, ..) in TIPOS {
        let Some(v) = app.get_webview_window(&etiqueta(tipo)) else { continue };
        if v.is_minimized().unwrap_or(true) || v.is_maximized().unwrap_or(true) {
            continue;
        }
        let (Ok(p), Ok(t)) = (v.outer_position(), v.inner_size()) else { continue };
        if t.width > 0 && t.height > 0 {
            d.geometrias.insert(tipo.into(), Geometria { x: p.x, y: p.y, ancho: t.width, alto: t.height });
        }
    }
    escribir(app, &d);
}

/// Crea la ventana de la tarjeta (o la trae al frente si ya existe). `cursor` es el punto físico
/// donde se soltó; sin él se usa la geometría guardada o se centra.
fn abrir(app: &AppHandle, tipo: &str, cursor: Option<(f64, f64)>) -> Result<(), String> {
    let (tipo, ancho, alto) = tipo_valido(tipo).ok_or_else(|| format!("tipo de tarjeta no válido: {tipo}"))?;
    let etq = etiqueta(tipo);
    if let Some(v) = app.get_webview_window(&etq) {
        traer_al_frente(&v);
        return Ok(());
    }
    let v = WebviewWindowBuilder::new(app, &etq, WebviewUrl::App(format!("tarjeta.html?tipo={tipo}").into()))
        .title(format!("{} // NETRUNNER", tipo.to_uppercase()))
        .inner_size(ancho, alto)
        .min_inner_size(MIN_ANCHO, MIN_ALTO)
        .decorations(false)
        .resizable(true)
        .transparent(false)
        .skip_taskbar(false)
        .always_on_top(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    let estado = app.state::<Tarjetas>();
    let guardada = bloquear(&estado.datos).geometrias.get(tipo).copied();
    let escala = cursor.map_or(1.0, |(x, y)| escala_en(app, x, y));
    let min = ((MIN_ANCHO * escala) as u32, (MIN_ALTO * escala) as u32);
    let g = geometria_inicial(cursor, guardada, (ancho, alto), escala)
        .and_then(|g| ajustar_a_monitores(g, &monitores(app), min));
    match g {
        Some(g) => {
            let _ = v.set_size(PhysicalSize::new(g.ancho, g.alto));
            let _ = v.set_position(PhysicalPosition::new(g.x, g.y));
        }
        None => {
            let _ = v.center();
        }
    }

    let app2 = app.clone();
    let tipo_s = tipo.to_string();
    v.on_window_event(move |e| match e {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            let Some(est) = app2.try_state::<Tarjetas>() else { return };
            est.rebote.tocar(&app2, guardar_todas);
        }
        WindowEvent::CloseRequested { .. } => {
            let Some(est) = app2.try_state::<Tarjetas>() else { return };
            bloquear(&est.cerrando).insert(tipo_s.clone());
            guardar_todas(&app2); // la ventana aún existe: se guarda su geometría final
        }
        WindowEvent::Destroyed => {
            let Some(est) = app2.try_state::<Tarjetas>() else { return };
            if bloquear(&est.cerrando).remove(&tipo_s) {
                {
                    let mut d = bloquear(&est.datos);
                    d.abiertas.retain(|t| t != &tipo_s);
                    escribir(&app2, &d);
                }
                avisar_cambio(&app2);
            }
        }
        _ => {}
    });

    {
        let mut d = bloquear(&estado.datos);
        if !d.abiertas.iter().any(|t| t == tipo) {
            d.abiertas.push(tipo.to_string());
        }
        escribir(app, &d);
    }
    avisar_cambio(app);
    traer_al_frente(&v);
    Ok(())
}

/// `x` e `y`: posición física del cursor al soltar. Se prefiere la lectura directa del cursor
/// (`cursor_position`, ya en físicos): `screenX × devicePixelRatio` de la página falla con
/// monitores de distinta escala. Los argumentos quedan como respaldo.
#[tauri::command]
pub async fn abrir_tarjeta(app: AppHandle, tipo: String, x: Option<f64>, y: Option<f64>) -> Result<(), String> {
    let cursor = match (x, y) {
        (Some(x), Some(y)) => Some(app.cursor_position().map_or((x, y), |c| (c.x, c.y))),
        _ => None,
    };
    abrir(&app, &tipo, cursor)
}

#[tauri::command]
pub fn tarjetas_abiertas(app: AppHandle) -> Vec<String> {
    lista_abiertas(&app)
}

#[tauri::command]
pub fn cerrar_tarjeta(app: AppHandle, tipo: String) -> Result<(), String> {
    let (tipo, ..) = tipo_valido(&tipo).ok_or_else(|| format!("tipo de tarjeta no válido: {tipo}"))?;
    if let Some(v) = app.get_webview_window(&etiqueta(tipo)) {
        v.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// --- Arrastre: pausa del cierre automático del panel ---

/// ¿Hay un arrastre en curso? Lo consultan el hilo de detección y el evento de foco del panel.
/// Pasados `MAX_ARRASTRE` se apaga solo (y esconde el fantasma).
pub fn arrastre_activo(app: &AppHandle) -> bool {
    let Some(estado) = app.try_state::<Tarjetas>() else { return false };
    let mut a = bloquear(&estado.arrastre);
    match *a {
        Some(t) if t.elapsed() > MAX_ARRASTRE => {
            *a = None;
            drop(a);
            fantasma_ocultar(app.clone());
            false
        }
        Some(_) => true,
        None => false,
    }
}

/// El hilo de detección reinicia solo su cuenta de salida mientras hay arrastre, así que al
/// terminar el panel se cierra con normalidad si el cursor ya está fuera.
#[tauri::command]
pub fn arrastre_tarjeta(app: AppHandle, activo: bool) {
    let Some(estado) = app.try_state::<Tarjetas>() else { return };
    *bloquear(&estado.arrastre) = activo.then(Instant::now);
    if !activo {
        fantasma_ocultar(app);
    }
}

// --- Ventana fantasma ---

/// Crea la ventana fantasma oculta. Transparente, sin foco, sin ratón y sin sombra.
fn crear_fantasma(app: &AppHandle) {
    if app.get_webview_window(FANTASMA).is_some() {
        return;
    }
    let v = WebviewWindowBuilder::new(app, FANTASMA, WebviewUrl::App("fantasma.html".into()))
        .title("Netrunner Panel · fantasma")
        .inner_size(FANTASMA_MAX.0, FANTASMA_MAX.1)
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .focused(false)
        .focusable(false)
        .build();
    if let Ok(v) = v {
        let _ = v.set_ignore_cursor_events(true);
    }
}

/// Coloca el fantasma con el cursor (físico) sobre su cabecera.
fn colocar_fantasma(app: &AppHandle, cx: f64, cy: f64) {
    if let Some(v) = app.get_webview_window(FANTASMA) {
        let e = escala_en(app, cx, cy);
        let _ = v.set_position(PhysicalPosition::new(
            (cx - AGARRE_FANTASMA.0 * e).round() as i32,
            (cy - AGARRE_FANTASMA.1 * e).round() as i32,
        ));
    }
}

/// `ancho` y `alto`: tamaño lógico de la tarjeta (se limita a 420×300).
#[tauri::command]
pub fn fantasma_mostrar(app: AppHandle, titulo: String, ancho: f64, alto: f64) {
    let Some(v) = app.get_webview_window(FANTASMA) else { return };
    let (a, h) = (ancho.clamp(120.0, FANTASMA_MAX.0), alto.clamp(60.0, FANTASMA_MAX.1));
    let _ = v.set_size(LogicalSize::new(a, h));
    if let Ok(c) = app.cursor_position() {
        colocar_fantasma(&app, c.x, c.y);
    }
    let _ = app.emit_to(FANTASMA, "fantasma://titulo", titulo);
    // Como `mostrar_mini`: WebView2 puede pintar de negro lo transparente tras ocultar/redimensionar
    let _ = v.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
    let _ = v.show();
}

/// Sin argumentos se usa la posición real del cursor (ver `abrir_tarjeta`).
#[tauri::command]
pub fn fantasma_mover(app: AppHandle, x: Option<f64>, y: Option<f64>) {
    let punto = app.cursor_position().ok().map(|c| (c.x, c.y)).or(x.zip(y));
    if let Some((x, y)) = punto {
        colocar_fantasma(&app, x, y);
    }
}

#[tauri::command]
pub fn fantasma_ocultar(app: AppHandle) {
    if let Some(v) = app.get_webview_window(FANTASMA) {
        let _ = v.hide();
    }
}

/// Registra el estado, crea la ventana fantasma y reabre las tarjetas que estaban abiertas.
/// Las ventanas se crean fuera del hilo principal, como exige Tauri en Windows.
pub fn iniciar(app: &AppHandle) {
    let mut datos = leer(app);
    let previas = std::mem::take(&mut datos.abiertas);
    app.manage(Tarjetas {
        datos: Mutex::new(datos),
        arrastre: Mutex::new(None),
        cerrando: Mutex::new(HashSet::new()),
        rebote: Arc::new(Antirrebote::default()),
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        crear_fantasma(&app);
        for tipo in previas {
            let _ = abrir(&app, &tipo, None);
        }
    });
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn g(x: i32, y: i32, ancho: u32, alto: u32) -> Geometria {
        Geometria { x, y, ancho, alto }
    }

    #[test]
    fn solo_se_aceptan_los_tipos_conocidos() {
        assert_eq!(tipo_valido("meteo"), Some(("meteo", 760.0, 520.0)));
        assert!(tipo_valido("sistema").is_some() && tipo_valido("notas").is_some());
        for malo in ["", "consumo", "METEO", "../meteo", "meteo "] {
            assert!(tipo_valido(malo).is_none(), "{malo:?}");
        }
    }

    #[test]
    fn la_ventana_nace_con_el_cursor_en_la_barra_de_titulo() {
        // Escala 1.5: 40 × 1.5 = 60 px a la izquierda y 16 × 1.5 = 24 px arriba
        let r = geometria_inicial(Some((1000.0, 500.0)), None, (760.0, 520.0), 1.5);
        assert_eq!(r, Some(g(940, 476, 1140, 780)));
        // Con tamaño guardado se conserva
        let r = geometria_inicial(Some((1000.0, 500.0)), Some(g(0, 0, 900, 700)), (760.0, 520.0), 1.0);
        assert_eq!(r, Some(g(960, 484, 900, 700)));
    }

    #[test]
    fn sin_cursor_se_usa_lo_guardado_o_nada() {
        assert_eq!(geometria_inicial(None, Some(g(5, 6, 700, 500)), (760.0, 520.0), 1.0), Some(g(5, 6, 700, 500)));
        assert_eq!(geometria_inicial(None, None, (760.0, 520.0), 1.0), None);
    }

    #[test]
    fn el_json_se_lee_y_escribe_con_campos_ausentes() {
        let d: Datos = serde_json::from_str("{}").unwrap();
        assert_eq!(d, Datos::default());
        let mut d = Datos::default();
        d.abiertas.push("meteo".into());
        d.geometrias.insert("meteo".into(), g(10, 20, 760, 520));
        let texto = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Datos>(&texto).unwrap(), d);
    }
}
