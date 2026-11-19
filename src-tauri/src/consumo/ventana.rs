//! Ventana flotante de CONSUMO (`consumo.html`): ventana HUD «normal» sin marco de Windows (la
//! página dibuja su propia barra de título), redimensionable, con botón en la barra de tareas.
//! Una sola instancia: si ya está abierta, se trae al frente. Al cerrarla se destruye.
//!
//! Recuerda su posición y tamaño en `app_data_dir()/consumo_ventana.json` (se guardan 1 s después
//! del último movimiento, no en cada evento) y los restaura comprobando que caen en un monitor.
//! Con la ventana minimizada o destruida, el origen "ventana" de la visibilidad queda apagado.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

pub const ETIQUETA: &str = "consumo";
const FICHERO: &str = "consumo_ventana.json";
const ANCHO: f64 = 900.0;
const ALTO: f64 = 640.0;
const MIN_ANCHO: f64 = 640.0;
const MIN_ALTO: f64 = 480.0;
/// Sin cambios durante este tiempo antes de guardar la geometría.
const ESPERA_GUARDAR: Duration = Duration::from_secs(1);
/// Altura (px físicos) de la franja de título que debe quedar a la vista en algún monitor.
const FRANJA: i64 = 40;

/// Posición y tamaño de la ventana, en píxeles físicos.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Geometria {
    pub x: i32,
    pub y: i32,
    pub ancho: u32,
    pub alto: u32,
}

/// Un monitor: origen y tamaño en píxeles físicos.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub ancho: u32,
    pub alto: u32,
}

/// Largo del solape de dos tramos (inicio, largo) en un eje.
fn solape(a: i64, la: i64, b: i64, lb: i64) -> i64 {
    ((a + la).min(b + lb) - a.max(b)).max(0)
}

/// Ajusta la geometría guardada a los monitores actuales. None si la franja de título no se vería
/// en ninguno (hay que centrarla); si no, la geometría con el tamaño dentro del monitor y de los
/// mínimos, y la posición dentro de ese monitor.
pub fn ajustar_a_monitores(g: Geometria, monitores: &[Rect], min: (u32, u32)) -> Option<Geometria> {
    let (gx, gy) = (g.x as i64, g.y as i64);
    // El monitor donde más se ve la franja de título
    let (mejor, area) = monitores
        .iter()
        .map(|m| {
            let w = solape(gx, g.ancho as i64, m.x as i64, m.ancho as i64);
            let h = solape(gy, FRANJA, m.y as i64, m.alto as i64);
            (m, (w, h))
        })
        .max_by_key(|(_, (w, h))| w * h)?;
    if area.0 < 100 || area.1 < 20 {
        return None;
    }
    let ancho = g.ancho.max(min.0).min(mejor.ancho.max(1));
    let alto = g.alto.max(min.1).min(mejor.alto.max(1));
    let x = gx.clamp(mejor.x as i64, mejor.x as i64 + (mejor.ancho - ancho) as i64);
    let y = gy.clamp(mejor.y as i64, mejor.y as i64 + (mejor.alto - alto) as i64);
    Some(Geometria { x: x as i32, y: y as i32, ancho, alto })
}

/// Cuánto falta para guardar: None si ya pasó el tiempo de espera desde el último cambio.
pub fn restante(ultimo: Instant, ahora: Instant, espera: Duration) -> Option<Duration> {
    let pasado = ahora.saturating_duration_since(ultimo);
    (pasado < espera).then(|| espera - pasado)
}

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FICHERO))
}

fn leer(app: &AppHandle) -> Option<Geometria> {
    let texto = std::fs::read_to_string(ruta(app)?).ok()?;
    serde_json::from_str(&texto).ok()
}

/// Guarda la geometría actual (nunca minimizada ni maximizada: en Windows la posición sería
/// -32000 y se perdería la buena).
fn guardar(app: &AppHandle) {
    let Some(v) = app.get_webview_window(ETIQUETA) else { return };
    if v.is_minimized().unwrap_or(true) || v.is_maximized().unwrap_or(true) {
        return;
    }
    let (Ok(p), Ok(t)) = (v.outer_position(), v.inner_size()) else { return };
    if t.width == 0 || t.height == 0 {
        return;
    }
    let g = Geometria { x: p.x, y: p.y, ancho: t.width, alto: t.height };
    if let Some(r) = ruta(app) {
        let _ = crate::sync::escribir_atomico(&r, &g);
    }
}

/// Antirrebote: como mucho un hilo vivo, que guarda cuando pasa 1 s sin cambios y termina.
#[derive(Default)]
struct Antirrebote {
    ultimo: Mutex<Option<Instant>>,
    hilo: AtomicBool,
}

impl Antirrebote {
    fn tocar(self: &Arc<Self>, app: &AppHandle) {
        let mut g = self.ultimo.lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(Instant::now());
        if self.hilo.swap(true, Ordering::SeqCst) {
            return;
        }
        drop(g);
        let (a, app) = (self.clone(), app.clone());
        thread::spawn(move || loop {
            let espera = {
                let mut g = a.ultimo.lock().unwrap_or_else(|e| e.into_inner());
                match g.and_then(|u| restante(u, Instant::now(), ESPERA_GUARDAR)) {
                    Some(r) => r,
                    None => {
                        *g = None;
                        a.hilo.store(false, Ordering::SeqCst);
                        drop(g);
                        guardar(&app);
                        return;
                    }
                }
            };
            thread::sleep(espera);
        });
    }

    /// Guarda ya lo pendiente (al cerrar).
    fn volcar(&self, app: &AppHandle) {
        let pendiente = self.ultimo.lock().unwrap_or_else(|e| e.into_inner()).take().is_some();
        if pendiente {
            guardar(app);
        }
    }
}

fn monitores(app: &AppHandle) -> Vec<Rect> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Rect {
            x: m.position().x,
            y: m.position().y,
            ancho: m.size().width,
            alto: m.size().height,
        })
        .collect()
}

fn traer_al_frente(v: &WebviewWindow) {
    if v.is_minimized().unwrap_or(false) {
        let _ = v.unminimize();
    }
    let _ = v.show();
    let _ = v.set_focus();
}

/// Abre la ventana de CONSUMO o, si ya está abierta, la trae al frente. Es `async` para que la
/// ventana se cree fuera del hilo principal (como exige Tauri en Windows).
#[tauri::command]
pub async fn abrir_ventana_consumo(app: AppHandle) -> Result<(), String> {
    if let Some(v) = app.get_webview_window(ETIQUETA) {
        traer_al_frente(&v);
        return Ok(());
    }
    let v = WebviewWindowBuilder::new(&app, ETIQUETA, WebviewUrl::App("consumo.html".into()))
        .title("CONSUMO // NETRUNNER")
        .inner_size(ANCHO, ALTO)
        .min_inner_size(MIN_ANCHO, MIN_ALTO)
        .decorations(false)
        .resizable(true)
        .transparent(false)
        .skip_taskbar(false)
        .always_on_top(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    // Geometría guardada si aún cae en algún monitor; si no, centrada
    let guardada = leer(&app)
        .and_then(|g| ajustar_a_monitores(g, &monitores(&app), (MIN_ANCHO as u32, MIN_ALTO as u32)));
    match guardada {
        Some(g) => {
            let _ = v.set_size(PhysicalSize::new(g.ancho, g.alto));
            let _ = v.set_position(PhysicalPosition::new(g.x, g.y));
        }
        None => {
            let _ = v.center();
        }
    }

    let rebote = Arc::new(Antirrebote::default());
    let app2 = app.clone();
    let minimizada = Arc::new(AtomicBool::new(false));
    v.on_window_event(move |e| match e {
        WindowEvent::Moved(_) => rebote.tocar(&app2),
        WindowEvent::Resized(_) => {
            // Minimizar también es un cambio de tamaño: se avisa del cambio de visibilidad
            if let Some(v) = app2.get_webview_window(ETIQUETA) {
                let min = v.is_minimized().unwrap_or(false);
                if minimizada.swap(min, Ordering::SeqCst) != min {
                    avisar_visibilidad(&app2, !min);
                }
            }
            rebote.tocar(&app2);
        }
        WindowEvent::CloseRequested { .. } => rebote.volcar(&app2),
        WindowEvent::Destroyed => {
            let _ = super::fijar_vista(&app2.state::<super::Detalle>(), "ventana", false);
        }
        _ => {}
    });

    traer_al_frente(&v);
    Ok(())
}

/// Ventana minimizada o restaurada: se apaga/enciende el origen "ventana" y se avisa a la página
/// para que pare sus temporizadores (la página también mira `document.visibilityState`).
fn avisar_visibilidad(app: &AppHandle, visible: bool) {
    let _ = super::fijar_vista(&app.state::<super::Detalle>(), "ventana", visible);
    let _ = app.emit_to(ETIQUETA, "consumo://ventana", visible);
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const M1: Rect = Rect { x: 0, y: 0, ancho: 1920, alto: 1080 };
    const M2: Rect = Rect { x: 1920, y: 0, ancho: 1280, alto: 1024 };
    const MIN: (u32, u32) = (640, 480);

    fn g(x: i32, y: i32, ancho: u32, alto: u32) -> Geometria {
        Geometria { x, y, ancho, alto }
    }

    #[test]
    fn dentro_de_un_monitor_se_respeta() {
        assert_eq!(ajustar_a_monitores(g(100, 80, 900, 640), &[M1], MIN), Some(g(100, 80, 900, 640)));
        assert_eq!(ajustar_a_monitores(g(2000, 50, 900, 640), &[M1, M2], MIN), Some(g(2000, 50, 900, 640)));
    }

    #[test]
    fn monitor_desconectado_o_fuera_de_pantalla_da_none() {
        assert_eq!(ajustar_a_monitores(g(2000, 50, 900, 640), &[M1], MIN), None);
        assert_eq!(ajustar_a_monitores(g(-32000, -32000, 900, 640), &[M1], MIN), None);
        assert_eq!(ajustar_a_monitores(g(100, 100, 900, 640), &[], MIN), None);
        assert_eq!(ajustar_a_monitores(g(100, 1070, 900, 640), &[M1], MIN), None);
    }

    #[test]
    fn se_recoloca_dentro_del_monitor_y_con_tamano_valido() {
        assert_eq!(ajustar_a_monitores(g(1500, 100, 900, 640), &[M1], MIN), Some(g(1020, 100, 900, 640)));
        assert_eq!(ajustar_a_monitores(g(0, 0, 4000, 3000), &[M1], MIN), Some(g(0, 0, 1920, 1080)));
        assert_eq!(ajustar_a_monitores(g(10, 10, 100, 100), &[M1], MIN), Some(g(10, 10, 640, 480)));
    }

    #[test]
    fn antirrebote_espera_un_segundo_sin_cambios() {
        let t0 = Instant::now();
        let e = Duration::from_secs(1);
        assert_eq!(restante(t0, t0 + Duration::from_millis(300), e), Some(Duration::from_millis(700)));
        assert_eq!(restante(t0, t0 + Duration::from_secs(1), e), None);
        assert_eq!(restante(t0, t0 + Duration::from_secs(5), e), None);
    }
}
