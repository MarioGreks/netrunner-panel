//! Posicionamiento y control de las dos ventanas (panel y pestaña de borde).
//!
//! Todo se calcula en píxeles FÍSICOS: el área de trabajo y el cursor ya vienen en
//! físicos, y las medidas lógicas del diseño (420 px, 8×140, 760×480) se multiplican
//! por el factor de escala del monitor principal.

use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::config::Borde;
use crate::estado::Estado;

const ANCHO_PANEL: f64 = 420.0;
const PANEL_SUPERIOR: (f64, f64) = (760.0, 480.0);
/// Miniventana del borde (ancho, alto) en píxeles lógicos.
const MINI: (f64, f64) = (220.0, 96.0);
/// Separación (lógica) entre la miniventana y la barra de tareas en los bordes laterales.
const MARGEN_MINI: f64 = 8.0;
const MARGEN_BORDE: f64 = 2.0;
/// Fracción central del borde que abre el panel (0.6 = el 60 % del medio).
const ZONA_CENTRAL: f64 = 0.6;
const SONDEO: Duration = Duration::from_millis(60);
const ESPERA_APERTURA: Duration = Duration::from_millis(250);
/// Tiempo con el cursor fuera del panel antes de cerrarlo (evita cierres por un roce).
const ESPERA_CIERRE: Duration = Duration::from_millis(120);
/// Si tras pedir el cierre el panel sigue visible este tiempo, se vuelve a pedir.
const REARME_CIERRE: Duration = Duration::from_millis(600);

type Rect = (PhysicalPosition<i32>, PhysicalSize<u32>);

/// Geometría física de ambas ventanas y del monitor: (panel, pestaña, monitor completo).
struct Geometria {
    panel: Rect,
    pestana: Rect,
    monitor: Rect,
}

fn redondear(v: f64) -> i32 {
    v.round() as i32
}

fn calcular(app: &AppHandle, borde: Borde) -> Option<Geometria> {
    let m = app.primary_monitor().ok().flatten()?;
    let k = m.scale_factor();
    let area = m.work_area();
    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width as i32, area.size.height as i32);

    let fisico = |v: f64| redondear(v * k).max(1) as u32;

    let (panel, pestana) = match borde {
        Borde::Right | Borde::Left => {
            let pw = fisico(ANCHO_PANEL);
            let (tw, th) = (fisico(MINI.0), fisico(MINI.1));
            let x = if borde == Borde::Right { ax + aw - pw as i32 } else { ax };
            let tx = if borde == Borde::Right { ax + aw - tw as i32 } else { ax };
            (
                (PhysicalPosition::new(x, ay), PhysicalSize::new(pw, ah as u32)),
                (
                    // Abajo del borde, justo encima de la barra de tareas: no tapa
                    // el centro de las ventanas ni la X de las maximizadas
                    PhysicalPosition::new(tx, ay + ah - th as i32 - fisico(MARGEN_MINI) as i32),
                    PhysicalSize::new(tw, th),
                ),
            )
        }
        Borde::Top => {
            let (pw, ph) = (fisico(PANEL_SUPERIOR.0), fisico(PANEL_SUPERIOR.1));
            // Arriba la miniventana va centrada y pegada al borde superior
            let (tw, th) = (fisico(MINI.0), fisico(MINI.1));
            (
                (PhysicalPosition::new(ax + (aw - pw as i32) / 2, ay), PhysicalSize::new(pw, ph)),
                (
                    PhysicalPosition::new(ax + (aw - tw as i32) / 2, ay),
                    PhysicalSize::new(tw, th),
                ),
            )
        }
    };

    Some(Geometria {
        panel,
        pestana,
        monitor: (*m.position(), *m.size()),
    })
}

fn aplicar(app: &AppHandle, etiqueta: &str, r: Rect) {
    if let Some(v) = app.get_webview_window(etiqueta) {
        let _ = v.set_size(r.1);
        let _ = v.set_position(r.0);
    }
}

/// Recoloca panel y pestaña según la configuración actual.
pub fn reposicionar(app: &AppHandle) {
    let borde = app.state::<Estado>().config.lock().unwrap().edge;
    if let Some(g) = calcular(app, borde) {
        aplicar(app, "panel", g.panel);
        aplicar(app, "tab", g.pestana);
    }
}

/// Estado inicial: miniventana visible, panel oculto (nunca se muestra el panel al arrancar).
pub fn preparar(app: &AppHandle) {
    reposicionar(app);
    if let Some(t) = app.get_webview_window("tab") {
        let _ = t.show();
    }
}

pub fn mostrar_panel(app: &AppHandle) {
    let estado = app.state::<Estado>();
    if estado.panel_visible.swap(true, Ordering::SeqCst) {
        // Ya visible: solo devolverle el foco (p. ej. segunda instancia)
        if let Some(p) = app.get_webview_window("panel") {
            let _ = p.set_focus();
        }
        return;
    }
    reposicionar(app);
    if let Some(p) = app.get_webview_window("panel") {
        let _ = p.show();
        let _ = p.set_focus();
    }
    // La miniventana se esconde mientras el panel está abierto
    if let Some(t) = app.get_webview_window("tab") {
        let _ = t.hide();
    }
    let _ = app.emit("panel://mostrar", ());
}

/// Oculta la ventana (lo llama el frontend al acabar la animación de salida).
pub fn ocultar_panel_ya(app: &AppHandle) {
    let estado = app.state::<Estado>();
    estado.panel_visible.store(false, Ordering::SeqCst);
    if let Some(p) = app.get_webview_window("panel") {
        let _ = p.hide();
    }
    if let Some(t) = app.get_webview_window("tab") {
        let _ = t.show();
    }
}

/// Pide al frontend que haga la animación de salida.
pub fn pedir_ocultar(app: &AppHandle) {
    if app.state::<Estado>().panel_visible.load(Ordering::SeqCst) {
        let _ = app.emit("panel://ocultar", ());
    }
}

#[tauri::command]
pub fn alternar_panel(app: AppHandle) {
    if app.state::<Estado>().panel_visible.load(Ordering::SeqCst) {
        pedir_ocultar(&app);
    } else {
        mostrar_panel(&app);
    }
}

#[tauri::command]
pub fn ocultar_panel(app: AppHandle) {
    ocultar_panel_ya(&app);
}

/// ¿Está el cursor pegado al borde configurado, dentro del tramo del panel?
fn en_zona_de_borde(g: &Geometria, borde: Borde, cx: f64, cy: f64) -> bool {
    let (mp, ms) = g.monitor;
    let (pp, ps) = g.panel;
    let (izq, der) = (mp.x as f64, mp.x as f64 + ms.width as f64);
    let arriba = mp.y as f64;
    // Solo el tramo central (ZONA_CENTRAL del lado del panel) abre: así las esquinas,
    // donde están la X de las ventanas maximizadas y el reloj, no lo disparan.
    let margen_x = ps.width as f64 * (1.0 - ZONA_CENTRAL) / 2.0;
    let margen_y = ps.height as f64 * (1.0 - ZONA_CENTRAL) / 2.0;
    let (px0, px1) = (pp.x as f64 + margen_x, pp.x as f64 + ps.width as f64 - margen_x);
    let (py0, py1) = (pp.y as f64 + margen_y, pp.y as f64 + ps.height as f64 - margen_y);
    match borde {
        Borde::Right => cx >= der - 1.0 - MARGEN_BORDE && cx < der + 1.0 && cy >= py0 && cy < py1,
        Borde::Left => cx <= izq + MARGEN_BORDE && cx >= izq - 1.0 && cy >= py0 && cy < py1,
        Borde::Top => cy <= arriba + MARGEN_BORDE && cy >= arriba - 1.0 && cx >= px0 && cx < px1,
    }
}

/// ¿Está el cursor sobre el panel (con 1 px de holgura)?
fn sobre_panel(g: &Geometria, cx: f64, cy: f64) -> bool {
    let (pp, ps) = g.panel;
    let (x0, y0) = (pp.x as f64 - 1.0, pp.y as f64 - 1.0);
    let (x1, y1) = (pp.x as f64 + ps.width as f64 + 1.0, pp.y as f64 + ps.height as f64 + 1.0);
    cx >= x0 && cx < x1 && cy >= y0 && cy < y1
}

/// Hilo de detección: abre el panel tras ≥250 ms con el cursor en el borde y lo
/// cierra tras ≥120 ms con el cursor fuera del panel. Si el cursor vuelve durante la
/// animación de salida, se cancela el cierre.
/// Tras cerrarse, exige salir de la zona antes de volver a armarse.
pub fn iniciar_deteccion(app: AppHandle) {
    thread::spawn(move || {
        let mut desde: Option<Instant> = None;
        let mut fuera_desde: Option<Instant> = None;
        // Momento en que se pidió el cierre. Si el panel sigue visible pasado un rato
        // (el frontend lo ignoró, p. ej. con un diálogo nativo abierto), se rearma.
        let mut cerrando: Option<Instant> = None;
        let mut armado = true;
        loop {
            thread::sleep(SONDEO);
            let Ok(c) = app.cursor_position() else { continue };
            let estado = app.state::<Estado>();
            let borde = estado.config.lock().unwrap().edge;
            let Some(g) = calcular(&app, borde) else { continue };

            // --- Cierre al sacar el ratón ---
            if estado.panel_visible.load(Ordering::SeqCst) {
                if sobre_panel(&g, c.x, c.y) {
                    fuera_desde = None;
                    if cerrando.take().is_some() {
                        // Volvió durante la animación de salida: se cancela
                        let _ = app.emit("panel://mostrar", ());
                    }
                } else if let Some(t) = cerrando {
                    if t.elapsed() >= REARME_CIERRE {
                        cerrando = None; // cierre ignorado: volver a pedirlo
                    }
                } else {
                    let t0 = *fuera_desde.get_or_insert_with(Instant::now);
                    if t0.elapsed() >= ESPERA_CIERRE {
                        cerrando = Some(Instant::now());
                        fuera_desde = None;
                        pedir_ocultar(&app);
                    }
                }
            } else {
                cerrando = None;
                fuera_desde = None;
            }

            let dentro = en_zona_de_borde(&g, borde, c.x, c.y);
            let pausado = estado.config.lock().unwrap().borde_pausado;

            if !dentro {
                armado = true;
                desde = None;
                continue;
            }
            // Con la apertura por borde en pausa no se abre (el cierre sigue funcionando)
            if estado.panel_visible.load(Ordering::SeqCst) || !armado || pausado {
                desde = None;
                continue;
            }
            let t0 = *desde.get_or_insert_with(Instant::now);
            if t0.elapsed() >= ESPERA_APERTURA {
                armado = false;
                desde = None;
                mostrar_panel(&app);
            }
        }
    });
}

