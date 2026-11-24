//! Widgets de escritorio: ventanas HUD pegadas al escritorio de Windows (`widget.html?tipo=<tipo>`,
//! etiqueta `widget-<tipo>`), por debajo de todas las demás ventanas y visibles con Win+D.
//!
//! - Recuerdan qué widgets están activos y su geometría en `app_data_dir()/widgets.json` (mismo
//!   antirrebote de 1 s que las tarjetas) y se reabren al arrancar.
//! - Rejilla de 20 px lógicos: 300 ms después del último movimiento o cambio de tamaño la ventana
//!   se encaja sola. Todo por eventos, sin sondeos.
//! - Modo edición (menú de la bandeja): la página muestra asas para mover, redimensionar y quitar.
//! - Si Explorer se reinicia, Progman (dueño de las ventanas) se destruye y arrastra a los widgets:
//!   se recrean pasados unos segundos, salvo que la app esté saliendo.
//! - Barra de edición (`widgets-barra`): ventana propia con el catálogo, la opacidad y LISTO; existe
//!   solo mientras dura la edición.
//! - Widgets tapados: un único hilo (cada 2 s, solo con widgets activos y sin modo Ocultar) mira si
//!   alguna ventana ajena los cubre por completo y avisa a la página cuando cambia, para que no
//!   trabaje mientras no se ve.
//! - Un widget nuevo se coloca en el primer hueco libre (esquina superior derecha, en columnas); si no
//!   cabe, en cascada.
//! - El contenido de CONSUMO y TAREAS llega en otro encargo; de momento la página pinta un marcador.

use std::collections::{HashMap, HashSet};
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

use crate::consumo::ventana::{ajustar_a_monitores, monitores, restante, Antirrebote, Geometria, Rect};

const FICHERO: &str = "widgets.json";
/// (tipo, ancho, alto, ancho mínimo, alto mínimo), en píxeles lógicos y múltiplos de la rejilla.
const TIPOS: [(&str, f64, f64, f64, f64); 5] = [
    ("reloj", 320.0, 160.0, 200.0, 100.0),
    ("sistema", 280.0, 220.0, 200.0, 140.0),
    ("consumo", 280.0, 220.0, 200.0, 160.0),
    ("tareas", 280.0, 300.0, 200.0, 160.0),
    ("cuentas", 280.0, 180.0, 200.0, 100.0),
];
/// Paso de la rejilla, en píxeles lógicos.
const PASO: f64 = 20.0;
/// Margen con el borde del área de trabajo y desplazamiento en cascada, en píxeles lógicos.
const MARGEN: f64 = 20.0;
const CASCADA: f64 = 40.0;
/// Pausa tras el último movimiento antes de encajar en la rejilla.
const ESPERA_ENCAJE: Duration = Duration::from_millis(300);
/// Espera antes de recrear un widget destruido por un reinicio de Explorer y entre reintentos.
const ESPERA_RECREAR: Duration = Duration::from_secs(3);
const REINTENTOS: u32 = 5;
const OPACIDAD_MIN: f64 = 0.2;
const OPACIDAD_MAX: f64 = 1.0;
const OPACIDAD_DEFECTO: f64 = 0.75;
/// Periodo del hilo de vigilancia de widgets tapados.
const PERIODO_VIGILANCIA: Duration = Duration::from_secs(2);
const ETIQUETA_BARRA: &str = "widgets-barra";
/// Tamaño lógico de la barra de edición y margen con el borde superior del área de trabajo.
const BARRA_ANCHO: f64 = 760.0;
const BARRA_ALTO: f64 = 76.0;
const BARRA_MARGEN: f64 = 16.0;

fn opacidad_defecto() -> f64 {
    OPACIDAD_DEFECTO
}

/// Lo que se guarda en disco.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Datos {
    #[serde(default)]
    activos: Vec<String>,
    #[serde(default)]
    geometrias: HashMap<String, Geometria>,
    #[serde(default = "opacidad_defecto")]
    opacidad: f64,
}

impl Default for Datos {
    fn default() -> Self {
        Datos { activos: Vec::new(), geometrias: HashMap::new(), opacidad: OPACIDAD_DEFECTO }
    }
}

/// Lo que ve la página (comando `widgets_estado` y evento `widgets://estado`).
#[derive(Clone, Debug, Serialize)]
pub struct EstadoWidgets {
    activos: Vec<String>,
    opacidad: f64,
    edicion: bool,
}

/// Estado compartido (se registra con `app.manage` en `iniciar`).
pub struct Widgets {
    datos: Mutex<Datos>,
    /// Ventanas quitadas por el usuario (CloseRequested): solo esas salen de «activos».
    /// Al salir de la app o al reiniciarse Explorer las ventanas se destruyen sin pasar por ahí.
    cerrando: Mutex<HashSet<String>>,
    edicion: AtomicBool,
    /// La app está saliendo: no se recrea nada.
    saliendo: AtomicBool,
    encaje: Arc<ReboteEncaje>,
    rebote: Arc<Antirrebote>,
    /// Visibilidad por tipo según el último repaso (si falta, se da por visible).
    visibles: Mutex<HashMap<String, bool>>,
    /// Hay un hilo de vigilancia vivo.
    vigilando: AtomicBool,
    /// Serializa crear y destruir la barra de edición (las tareas pueden solaparse).
    barra: Mutex<()>,
}

fn bloquear<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Frontera de confianza: solo se aceptan los tipos de la lista.
fn tipo_valido(tipo: &str) -> Option<(&'static str, f64, f64, f64, f64)> {
    TIPOS.iter().copied().find(|(t, ..)| *t == tipo)
}

fn etiqueta(tipo: &str) -> String {
    format!("widget-{tipo}")
}

fn acotar_opacidad(o: f64) -> f64 {
    if o.is_finite() {
        o.clamp(OPACIDAD_MIN, OPACIDAD_MAX)
    } else {
        OPACIDAD_DEFECTO
    }
}

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FICHERO))
}

/// El fichero es editable: se descartan tipos desconocidos y repetidos y se acota la opacidad.
fn normalizar(d: &mut Datos) {
    let mut vistos = HashSet::new();
    d.activos.retain(|t| tipo_valido(t).is_some() && vistos.insert(t.clone()));
    d.geometrias.retain(|t, _| tipo_valido(t).is_some());
    d.opacidad = acotar_opacidad(d.opacidad);
}

fn interpretar(texto: Option<&str>) -> Datos {
    let mut d: Datos = texto.and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default();
    normalizar(&mut d);
    d
}

fn leer(app: &AppHandle) -> Datos {
    let texto = ruta(app).and_then(|r| std::fs::read_to_string(r).ok());
    interpretar(texto.as_deref())
}

fn escribir(app: &AppHandle, d: &Datos) {
    if let Some(r) = ruta(app) {
        let _ = crate::sync::escribir_atomico(&r, d);
    }
}

// --- Rejilla ---

/// Redondea al múltiplo de `paso` más cercano.
fn redondear(valor: i64, paso: i64) -> i64 {
    (valor as f64 / paso as f64).round() as i64 * paso
}

/// Encaja la geometría en la rejilla: x e y relativas al origen del monitor y ancho y alto, al
/// múltiplo de `paso` más cercano, sin bajar de `min`. Todo en píxeles físicos.
fn encajar(g: Geometria, origen: (i32, i32), paso: u32, min: (u32, u32)) -> Geometria {
    let paso = paso.max(1) as i64;
    let x = origen.0 as i64 + redondear(g.x as i64 - origen.0 as i64, paso);
    let y = origen.1 as i64 + redondear(g.y as i64 - origen.1 as i64, paso);
    let ancho = redondear(g.ancho as i64, paso).max(min.0 as i64);
    let alto = redondear(g.alto as i64, paso).max(min.1 as i64);
    Geometria { x: x as i32, y: y as i32, ancho: ancho as u32, alto: alto as u32 }
}

/// Esquina superior derecha del área de trabajo, desplazada en cascada según el índice del tipo
/// para que los widgets nuevos no queden uno encima de otro. `tam` es el tamaño lógico.
fn esquina_en_cascada(area: Rect, escala: f64, indice: usize, tam: (f64, f64)) -> Geometria {
    let ancho = (tam.0 * escala).round() as u32;
    let alto = (tam.1 * escala).round() as u32;
    let desp = (CASCADA * escala).round() as i32 * indice as i32;
    let margen = (MARGEN * escala).round() as i32;
    Geometria {
        x: area.x + area.ancho as i32 - ancho as i32 - margen - desp,
        y: area.y + margen + desp,
        ancho,
        alto,
    }
}

/// Primer hueco libre del área para un widget de tamaño `tam` (físico): desde la esquina superior
/// derecha (a `margen` del borde), recorriendo columnas de arriba abajo y luego hacia la izquierda,
/// con el `paso` de la rejilla. Un hueco vale si no se acerca a menos de `margen` de ningún ocupado.
fn buscar_hueco(area: Rect, ocupados: &[Geometria], tam: (u32, u32), paso: u32, margen: u32) -> Option<Geometria> {
    let paso = paso.max(1) as i64;
    let margen = margen as i64;
    let (ancho, alto) = (tam.0 as i64, tam.1 as i64);
    let (izq, arriba) = (area.x as i64 + margen, area.y as i64 + margen);
    let (der, abajo) = (area.x as i64 + area.ancho as i64 - margen, area.y as i64 + area.alto as i64 - margen);
    let libre = |x: i64, y: i64| {
        ocupados.iter().all(|o| {
            let (ox, oy) = (o.x as i64, o.y as i64);
            x + ancho + margen <= ox
                || ox + o.ancho as i64 + margen <= x
                || y + alto + margen <= oy
                || oy + o.alto as i64 + margen <= y
        })
    };
    let mut x = der - ancho;
    while x >= izq {
        let mut y = arriba;
        while y + alto <= abajo {
            if libre(x, y) {
                return Some(Geometria { x: x as i32, y: y as i32, ancho: tam.0, alto: tam.1 });
            }
            y += paso;
        }
        x -= paso;
    }
    None
}

/// Geometría de los widgets abiertos (los que ya tienen ventana).
fn geometrias_abiertas(app: &AppHandle) -> Vec<Geometria> {
    let mut v = Vec::new();
    for (tipo, ..) in TIPOS {
        let Some(w) = app.get_webview_window(&etiqueta(tipo)) else { continue };
        if let (Ok(p), Ok(t)) = (w.outer_position(), w.inner_size()) {
            if t.width > 0 && t.height > 0 {
                v.push(Geometria { x: p.x, y: p.y, ancho: t.width, alto: t.height });
            }
        }
    }
    v
}

/// Encaja la ventana del tipo en la rejilla si no lo está ya (aplicar solo si cambia evita bucles).
fn encajar_ventana(app: &AppHandle, tipo: &str) {
    let Some((_, _, _, min_ancho, min_alto)) = tipo_valido(tipo) else { return };
    let Some(v) = app.get_webview_window(&etiqueta(tipo)) else { return };
    if v.is_minimized().unwrap_or(true) || v.is_maximized().unwrap_or(true) {
        return;
    }
    let (Ok(p), Ok(t)) = (v.outer_position(), v.inner_size()) else { return };
    if t.width == 0 || t.height == 0 {
        return;
    }
    let escala = v.scale_factor().unwrap_or(1.0);
    let origen = v.current_monitor().ok().flatten().map_or((0, 0), |m| (m.position().x, m.position().y));
    let paso = (PASO * escala).round() as u32;
    let min = ((min_ancho * escala).round() as u32, (min_alto * escala).round() as u32);
    let actual = Geometria { x: p.x, y: p.y, ancho: t.width, alto: t.height };
    let nueva = encajar(actual, origen, paso, min);
    if nueva.ancho != actual.ancho || nueva.alto != actual.alto {
        let _ = v.set_size(PhysicalSize::new(nueva.ancho, nueva.alto));
    }
    if nueva.x != actual.x || nueva.y != actual.y {
        let _ = v.set_position(PhysicalPosition::new(nueva.x, nueva.y));
    }
}

/// Antirrebote del encaje: como mucho un hilo vivo, que encaja cuando pasan 300 ms sin eventos,
/// pide el guardado en disco y termina. No hay hilo girando mientras no se toca nada.
#[derive(Default)]
struct ReboteEncaje {
    ultimo: Mutex<Option<Instant>>,
    pendientes: Mutex<HashSet<String>>,
    hilo: AtomicBool,
}

impl ReboteEncaje {
    fn tocar(self: &Arc<Self>, app: &AppHandle, tipo: &str) {
        bloquear(&self.pendientes).insert(tipo.to_string());
        let mut u = bloquear(&self.ultimo);
        *u = Some(Instant::now());
        if self.hilo.swap(true, Ordering::SeqCst) {
            return;
        }
        drop(u);
        let (a, app) = (self.clone(), app.clone());
        thread::spawn(move || loop {
            let espera = {
                let mut u = bloquear(&a.ultimo);
                match u.and_then(|t| restante(t, Instant::now(), ESPERA_ENCAJE)) {
                    Some(r) => r,
                    None => {
                        *u = None;
                        a.hilo.store(false, Ordering::SeqCst);
                        drop(u);
                        let tipos: Vec<String> = bloquear(&a.pendientes).drain().collect();
                        for t in &tipos {
                            encajar_ventana(&app, t);
                        }
                        if let Some(est) = app.try_state::<Widgets>() {
                            est.rebote.tocar(&app, guardar_todas);
                        }
                        return;
                    }
                }
            };
            thread::sleep(espera);
        });
    }
}

// --- Estado y avisos ---

pub fn edicion_activa(app: &AppHandle) -> bool {
    app.try_state::<Widgets>().is_some_and(|e| e.edicion.load(Ordering::SeqCst))
}

fn estado_actual(app: &AppHandle) -> EstadoWidgets {
    let Some(est) = app.try_state::<Widgets>() else {
        return EstadoWidgets { activos: Vec::new(), opacidad: OPACIDAD_DEFECTO, edicion: false };
    };
    let d = bloquear(&est.datos);
    EstadoWidgets {
        activos: d.activos.clone(),
        opacidad: d.opacidad,
        edicion: est.edicion.load(Ordering::SeqCst),
    }
}

fn avisar_cambio(app: &AppHandle) {
    let _ = app.emit("widgets://estado", estado_actual(app));
}

/// Guarda la geometría de todas las ventanas abiertas (nunca minimizadas ni maximizadas: en
/// Windows la posición sería -32000 y se perdería la buena) y la lista de activos.
fn guardar_todas(app: &AppHandle) {
    let Some(estado) = app.try_state::<Widgets>() else { return };
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

/// Factor de escala del monitor bajo el punto (físico); 1.0 si no se sabe.
fn escala_en(app: &AppHandle, x: f64, y: f64) -> f64 {
    app.monitor_from_point(x, y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .map_or(1.0, |m| m.scale_factor())
}

// --- Pegado al escritorio (Windows) ---

/// ¿Existe la ventana Progman (el escritorio)? Sin ella no hay a qué pegarse.
#[cfg(windows)]
fn progman_existe() -> bool {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    // SAFETY: llamada de solo lectura con una cadena estática terminada en cero.
    unsafe { FindWindowW(w!("Progman"), PCWSTR::null()).is_ok() }
}

#[cfg(not(windows))]
fn progman_existe() -> bool {
    true
}

/// Mensaje propio de la subclase: restaurar el widget sin activarlo.
#[cfg(windows)]
const WM_RESTAURAR: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x4E;

/// Subclase del widget. Cada cambio de orden Z pasa por aquí antes de aplicarse y se redirige a
/// «justo encima de Progman» (su dueño): así, aunque el clic o la activación pidan subir la
/// ventana, nunca queda por encima de otras aplicaciones. No se usa `HWND_BOTTOM`: con Win+D
/// Progman cambia de sitio y el widget podía colarse POR DEBAJO de él (tapado por el fondo).
#[cfg(windows)]
unsafe extern "system" fn subclase_al_fondo(
    hwnd: windows::Win32::Foundation::HWND,
    mensaje: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _id: usize,
    _datos: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::DefSubclassProc;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindow, PostMessageW, ShowWindow, GW_HWNDPREV, GW_OWNER, HWND_TOP, SIZE_MINIMIZED, SWP_NOZORDER,
        SW_SHOWNOACTIVATE, WINDOWPOS, WM_SIZE, WM_WINDOWPOSCHANGING,
    };
    if mensaje == WM_WINDOWPOSCHANGING && lparam.0 != 0 {
        // SAFETY: en WM_WINDOWPOSCHANGING, lparam apunta a un WINDOWPOS válido y modificable.
        let pos = unsafe { &mut *(lparam.0 as *mut WINDOWPOS) };
        if pos.flags & SWP_NOZORDER == SWP_NOZORDER {
            // Sin cambio de orden Z: nada que corregir
        } else if let Ok(dueno) = unsafe { GetWindow(hwnd, GW_OWNER) } {
            // SAFETY: consultas de solo lectura sobre el dueño (Progman).
            match unsafe { GetWindow(dueno, GW_HWNDPREV) } {
                // Ya está justo encima de Progman: no se mueve
                Ok(previa) if previa == hwnd => pos.flags |= SWP_NOZORDER,
                // Insertar «después de» la ventana que precede a Progman = justo encima de él
                Ok(previa) => pos.hwndInsertAfter = previa,
                // Progman está arriba del todo (p. ej. con Win+D): el widget va por encima
                Err(_) => pos.hwndInsertAfter = HWND_TOP,
            }
        }
    }
    // Un widget nunca se queda minimizado (p. ej. si nace con «Mostrar escritorio» activo): se
    // restaura después, con un mensaje propio, porque no conviene llamar a ShowWindow dentro de WM_SIZE.
    if mensaje == WM_SIZE && wparam.0 == SIZE_MINIMIZED as usize {
        // SAFETY: mensaje a la propia ventana; si falla, se queda minimizada (no es grave).
        let _ = unsafe { PostMessageW(Some(hwnd), WM_RESTAURAR, WPARAM(0), LPARAM(0)) };
    }
    if mensaje == WM_RESTAURAR {
        // SAFETY: ventana propia y viva (estamos dentro de su procedimiento).
        let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
        return LRESULT(0);
    }
    // SAFETY: se reenvía el mensaje tal cual a la cadena de subclases.
    unsafe { DefSubclassProc(hwnd, mensaje, wparam, lparam) }
}

/// Pega la ventana al escritorio: Progman pasa a ser su dueño (no su padre), sale de Alt+Tab y de
/// la barra de tareas, y queda al fondo con una subclase que lo mantiene así. Si no hay Progman
/// la ventana sigue siendo una ventana normal al fondo (no es un error). Debe llamarse antes de
/// mostrarla. `SetWindowSubclass` exige el hilo que creó la ventana, de ahí `run_on_main_thread`.
#[cfg(windows)]
fn pegar_al_escritorio(v: &WebviewWindow, mostrar: bool) {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::SetWindowSubclass;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWLP_HWNDPARENT, GWL_EXSTYLE,
        HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE, ShowWindow, WS_EX_APPWINDOW,
        WS_EX_TOOLWINDOW,
    };
    let Ok(h) = v.hwnd() else { return };
    let crudo = h.0 as isize; // HWND no es Send: viaja como entero
    let _ = v.run_on_main_thread(move || {
        let hwnd = HWND(crudo as *mut _);
        // SAFETY: `hwnd` es la ventana de este widget, viva (aún no se ha mostrado ni cerrado) y
        // creada en este hilo. Todas las llamadas son de ventana propia salvo FindWindowW, de
        // solo lectura. Los fallos se ignoran a propósito: la ventana queda como una normal.
        unsafe {
            if let Ok(progman) = FindWindowW(w!("Progman"), PCWSTR::null()) {
                // GWLP_HWNDPARENT con ventana de nivel superior fija el DUEÑO, no el padre
                SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, progman.0 as isize);
            }
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let ex = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
            let _ = SetWindowSubclass(hwnd, Some(subclase_al_fondo), 0x4E52, 0);
            let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            // Se muestra aquí, ya con dueño y subclase, y sin activarla: un widget nunca roba el foco
            // (con el modo Ocultar activo se queda sin mostrar hasta que se desactive)
            if mostrar {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
    });
}

#[cfg(not(windows))]
fn pegar_al_escritorio(_v: &WebviewWindow, _mostrar: bool) {}

// --- Ventanas ---

/// Crea la ventana del widget (si ya existe no hace nada) y lo da por activo.
fn abrir(app: &AppHandle, tipo: &str) -> Result<(), String> {
    let (tipo, ancho, alto, min_ancho, min_alto) =
        tipo_valido(tipo).ok_or_else(|| format!("tipo de widget no válido: {tipo}"))?;
    let etq = etiqueta(tipo);
    if app.get_webview_window(&etq).is_some() {
        return Ok(());
    }
    olvidar_visibilidad(app, tipo); // un widget nuevo parte de «a la vista»
    let v = WebviewWindowBuilder::new(app, &etq, WebviewUrl::App(format!("widget.html?tipo={tipo}").into()))
        .title(format!("{} // NETRUNNER", tipo.to_uppercase()))
        .inner_size(ancho, alto)
        .min_inner_size(min_ancho, min_alto)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(true)
        .always_on_top(false)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    let estado = app.state::<Widgets>();
    let guardada = bloquear(&estado.datos).geometrias.get(tipo).copied();
    let escala_min = guardada.map_or(1.0, |g| escala_en(app, g.x as f64, g.y as f64));
    let min = ((min_ancho * escala_min) as u32, (min_alto * escala_min) as u32);
    let g = guardada
        .and_then(|g| ajustar_a_monitores(g, &monitores(app), min))
        .or_else(|| {
            let m = app.primary_monitor().ok().flatten()?;
            let (area, escala) = (m.work_area(), m.scale_factor());
            let rect = Rect {
                x: area.position.x,
                y: area.position.y,
                ancho: area.size.width,
                alto: area.size.height,
            };
            let tam = ((ancho * escala).round() as u32, (alto * escala).round() as u32);
            let paso = (PASO * escala).round() as u32;
            let margen = (MARGEN * escala).round() as u32;
            // Primero un hueco libre; si no lo hay, la cascada de siempre
            buscar_hueco(rect, &geometrias_abiertas(app), tam, paso, margen).or_else(|| {
                let indice = TIPOS.iter().position(|(t, ..)| *t == tipo).unwrap_or(0);
                Some(esquina_en_cascada(rect, escala, indice, (ancho, alto)))
            })
        });
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
            let Some(est) = app2.try_state::<Widgets>() else { return };
            est.encaje.tocar(&app2, &tipo_s);
        }
        WindowEvent::CloseRequested { api, .. } => {
            let Some(est) = app2.try_state::<Widgets>() else { return };
            // Solo la ✕ (quitar_widget, que marca «cerrando») quita un widget. Cualquier otra
            // petición de cierre (Alt+F4, WM_CLOSE de otro proceso o del sistema) se ignora: si no,
            // el widget saldría de «activos» y no volvería al arrancar.
            if !bloquear(&est.cerrando).contains(&tipo_s) {
                api.prevent_close();
                return;
            }
            guardar_todas(&app2); // la ventana aún existe: se guarda su geometría final
        }
        WindowEvent::Destroyed => {
            let Some(est) = app2.try_state::<Widgets>() else { return };
            bloquear(&est.visibles).remove(&tipo_s);
            if bloquear(&est.cerrando).remove(&tipo_s) {
                // Lo quitó el usuario
                {
                    let mut d = bloquear(&est.datos);
                    d.activos.retain(|t| t != &tipo_s);
                    escribir(&app2, &d);
                }
                avisar_cambio(&app2);
            } else if !est.saliendo.load(Ordering::SeqCst)
                && bloquear(&est.datos).activos.iter().any(|t| t == &tipo_s)
            {
                // Destruida sin que nadie la quitara (Explorer se reinició): se recrea
                recrear_tras_pausa(app2.clone(), tipo_s.clone());
            }
        }
        _ => {}
    });

    {
        let mut d = bloquear(&estado.datos);
        if !d.activos.iter().any(|t| t == tipo) {
            d.activos.push(tipo.to_string());
        }
        escribir(app, &d);
    }
    avisar_cambio(app);

    // Como `mostrar_mini`: WebView2 puede pintar de negro lo transparente tras ocultar/redimensionar
    let _ = v.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
    // La pega al escritorio y la muestra sin activarla (salvo en modo Ocultar)
    let mostrar = !crate::ventanas::esta_oculto();
    pegar_al_escritorio(&v, mostrar);
    #[cfg(not(windows))]
    if mostrar {
        let _ = v.show();
    }
    asegurar_vigilancia(app);
    Ok(())
}

/// Espera a que Explorer vuelva a tener escritorio y recrea el widget. Hasta `REINTENTOS` intentos
/// con pausa; al último se crea de todos modos (quedará como ventana normal al fondo). Se
/// abandona si la app sale o si el usuario lo quitó mientras tanto.
fn recrear_tras_pausa(app: AppHandle, tipo: String) {
    thread::spawn(move || {
        for intento in 1..=REINTENTOS {
            thread::sleep(ESPERA_RECREAR);
            let Some(est) = app.try_state::<Widgets>() else { return };
            let vigente = !est.saliendo.load(Ordering::SeqCst)
                && bloquear(&est.datos).activos.iter().any(|t| t == &tipo);
            if !vigente {
                return;
            }
            if progman_existe() || intento == REINTENTOS {
                let _ = abrir(&app, &tipo);
                return;
            }
        }
    });
}

/// Marca que la app está saliendo (desde `RunEvent::ExitRequested` o `Exit`): a partir de ahí no
/// se recrea ningún widget.
pub fn marcar_salida(app: &AppHandle) {
    if let Some(est) = app.try_state::<Widgets>() {
        est.saliendo.store(true, Ordering::SeqCst);
    }
}

/// Activa o desactiva el modo edición, sincroniza el check de la bandeja y avisa a las páginas.
pub fn fijar_edicion(app: &AppHandle, activa: bool) {
    let Some(est) = app.try_state::<Widgets>() else { return };
    // Editar con la app oculta no tendría sentido (barra sin widgets): se desoculta antes
    if activa && crate::ventanas::esta_oculto() {
        crate::ventanas::fijar_oculto(app, false);
    }
    est.edicion.store(activa, Ordering::SeqCst);
    crate::bandeja::actualizar_edicion(app, activa);
    avisar_cambio(app);
    sincronizar_barra(app);
}

/// Crea o destruye la barra según el modo edición. Se hace fuera del hilo que llama: crear
/// ventanas desde un comando síncrono o el menú de la bandeja (hilo principal) bloquearía Windows.
fn sincronizar_barra(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(est) = app.try_state::<Widgets>() else { return };
        let _paso = bloquear(&est.barra);
        // El modo se lee aquí, con el cerrojo, para que el último cambio gane
        if est.edicion.load(Ordering::SeqCst) {
            if app.get_webview_window(ETIQUETA_BARRA).is_none() {
                let _ = crear_barra(&app);
            }
        } else if let Some(v) = app.get_webview_window(ETIQUETA_BARRA) {
            let _ = v.destroy();
        }
    });
}

/// Barra de edición: arriba y centrada en el monitor principal, siempre visible y con el foco
/// (para que Esc funcione).
fn crear_barra(app: &AppHandle) -> Result<(), String> {
    let v = WebviewWindowBuilder::new(app, ETIQUETA_BARRA, WebviewUrl::App("widgets-barra.html".into()))
        .title("WIDGETS // EDICIÓN")
        .inner_size(BARRA_ANCHO, BARRA_ALTO)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(m) = app.primary_monitor().ok().flatten() {
        let (area, escala) = (m.work_area(), m.scale_factor());
        let ancho = (BARRA_ANCHO * escala).round() as i32;
        let x = area.position.x + (area.size.width as i32 - ancho) / 2;
        let y = area.position.y + (BARRA_MARGEN * escala).round() as i32;
        let _ = v.set_position(PhysicalPosition::new(x, y));
    } else {
        let _ = v.center();
    }
    let _ = v.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
    // Alt+F4 u otro cierre ajeno equivale a salir de la edición (`destroy` no pasa por aquí)
    let app2 = app.clone();
    v.on_window_event(move |e| {
        if let WindowEvent::CloseRequested { .. } = e {
            if edicion_activa(&app2) {
                fijar_edicion(&app2, false);
            }
        }
    });
    let _ = v.show();
    let _ = v.set_focus();
    Ok(())
}

// --- Modo «Ocultar Netrunner Panel» ---

/// Esconde todos los widgets o los vuelve a mostrar sin activarlos. Al ocultar se sale antes de la
/// edición y todos pasan a «no visibles» al instante.
pub fn fijar_oculto(app: &AppHandle, valor: bool) {
    if app.try_state::<Widgets>().is_none() {
        return;
    }
    if valor && edicion_activa(app) {
        fijar_edicion(app, false);
    }
    for (tipo, ..) in TIPOS {
        let Some(v) = app.get_webview_window(&etiqueta(tipo)) else { continue };
        if valor {
            let _ = v.hide();
        } else {
            mostrar_sin_activar(&v);
        }
    }
    revisar(app);
    if !valor {
        asegurar_vigilancia(app);
    }
}

/// Muestra el widget sin activarlo y lo devuelve al fondo (no roba el foco ni rompe el pegado).
#[cfg(windows)]
fn mostrar_sin_activar(v: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE,
    };
    let Ok(h) = v.hwnd() else { return };
    let crudo = h.0 as isize; // HWND no es Send: viaja como entero
    let _ = v.run_on_main_thread(move || {
        let hwnd = HWND(crudo as *mut _);
        // SAFETY: ventana de este widget, creada en este hilo; si se destruyó mientras tanto, las
        // llamadas fallan sin efecto y se ignoran.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    });
}

#[cfg(not(windows))]
fn mostrar_sin_activar(v: &WebviewWindow) {
    let _ = v.show();
}

// --- Widgets tapados ---

/// Rectángulo en píxeles físicos: (izquierda, arriba, derecha, abajo), como un RECT.
type Marco = (i32, i32, i32, i32);

/// ¿Cubre `exterior` por completo a `interior`? Bordes iguales cuentan como cubrir.
#[cfg_attr(not(windows), allow(dead_code))]
fn contiene(exterior: Marco, interior: Marco) -> bool {
    exterior.0 <= interior.0 && exterior.1 <= interior.1 && exterior.2 >= interior.2 && exterior.3 >= interior.3
}

/// Ventanas del propio escritorio y de la barra de tareas: nunca tapan un widget.
#[cfg_attr(not(windows), allow(dead_code))]
fn clase_ignorada(clase: &str) -> bool {
    matches!(clase, "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

#[derive(Clone, Serialize)]
struct AvisoVisible {
    tipo: String,
    visible: bool,
}

/// ¿Está el widget a la vista? Con el modo Ocultar, no. Sin dato todavía, sí.
pub fn a_la_vista(app: &AppHandle, tipo: &str) -> bool {
    if crate::ventanas::esta_oculto() {
        return false;
    }
    app.try_state::<Widgets>()
        .map_or(true, |e| bloquear(&e.visibles).get(tipo).copied().unwrap_or(true))
}

/// ¿Está alguien mirando el widget? Su ventana existe, es visible, no está minimizada y no está
/// tapada ni oculta. (`a_la_vista` por sí sola da «sí» también cuando el widget no está abierto.)
pub fn mirando(app: &AppHandle, tipo: &str) -> bool {
    app.get_webview_window(&etiqueta(tipo))
        .is_some_and(|v| v.is_visible().unwrap_or(false) && !v.is_minimized().unwrap_or(false))
        && a_la_vista(app, tipo)
}

#[tauri::command]
pub fn widget_a_la_vista(app: AppHandle, tipo: String) -> bool {
    a_la_vista(&app, &tipo)
}

fn olvidar_visibilidad(app: &AppHandle, tipo: &str) {
    if let Some(est) = app.try_state::<Widgets>() {
        bloquear(&est.visibles).remove(tipo);
    }
}

fn debe_vigilar(app: &AppHandle) -> bool {
    !crate::ventanas::esta_oculto()
        && app.try_state::<Widgets>().is_some_and(|e| !bloquear(&e.datos).activos.is_empty())
}

/// Arranca el hilo de vigilancia si hace falta y no hay ya uno. El hilo repasa al empezar y cada
/// 2 s, y termina solo cuando no queda widget activo o entra el modo Ocultar.
fn asegurar_vigilancia(app: &AppHandle) {
    let Some(est) = app.try_state::<Widgets>() else { return };
    if !debe_vigilar(app) || est.vigilando.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    thread::spawn(move || loop {
        let Some(est) = app.try_state::<Widgets>() else { return };
        if est.saliendo.load(Ordering::SeqCst) {
            est.vigilando.store(false, Ordering::SeqCst);
            return;
        }
        if !debe_vigilar(&app) {
            est.vigilando.store(false, Ordering::SeqCst);
            // Pudo activarse un widget justo entre la comprobación y el aviso de «ya no vigilo»
            if !debe_vigilar(&app) || est.vigilando.swap(true, Ordering::SeqCst) {
                return;
            }
        }
        revisar(&app);
        thread::sleep(PERIODO_VIGILANCIA);
    });
}

/// Calcula la visibilidad de cada widget abierto y avisa de los que cambian.
fn revisar(app: &AppHandle) {
    let Some(est) = app.try_state::<Widgets>() else { return };
    let oculto = crate::ventanas::esta_oculto();
    let abiertos: Vec<&str> = TIPOS
        .iter()
        .map(|(t, ..)| *t)
        .filter(|t| app.get_webview_window(&etiqueta(t)).is_some())
        .collect();
    let nuevos: Vec<(&str, bool)> = if oculto {
        abiertos.iter().map(|t| (*t, false)).collect()
    } else {
        calcular_visibles(app, &abiertos)
    };
    let mut cambios = Vec::new();
    {
        let mut mapa = bloquear(&est.visibles);
        mapa.retain(|t, _| abiertos.contains(&t.as_str()));
        for (tipo, visible) in nuevos {
            if mapa.insert(tipo.to_string(), visible).unwrap_or(true) != visible {
                cambios.push(AvisoVisible { tipo: tipo.to_string(), visible });
            }
        }
    }
    for c in cambios {
        let _ = app.emit("widgets://visible", c);
    }
}

#[cfg(not(windows))]
fn calcular_visibles<'a>(_app: &AppHandle, tipos: &[&'a str]) -> Vec<(&'a str, bool)> {
    tipos.iter().map(|t| (*t, true)).collect()
}

/// Estado de un widget durante el recorrido de ventanas.
#[cfg(windows)]
struct Objetivo<'a> {
    tipo: &'a str,
    hwnd: windows::Win32::Foundation::HWND,
    marco: Marco,
    /// Ya se llegó a su ventana (o a una que lo tapa): no hace falta seguir con él.
    resuelto: bool,
    tapado: bool,
}

#[cfg(windows)]
struct Recorrido<'a, 'b> {
    objetivos: &'b mut Vec<Objetivo<'a>>,
    pid: u32,
}

#[cfg(windows)]
unsafe extern "system" fn paso_enum(
    hwnd: windows::Win32::Foundation::HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::core::BOOL {
    use windows::core::BOOL;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        GWL_EXSTYLE, WS_EX_TRANSPARENT,
    };
    // SAFETY: lparam es el puntero a `Recorrido` que pasa `calcular_visibles`, vivo y exclusivo
    // durante toda la llamada a EnumWindows (síncrona, en este mismo hilo).
    let rec = unsafe { &mut *(lparam.0 as *mut Recorrido) };
    // De arriba abajo: al llegar al HWND de un widget, todo lo que lo podía tapar ya se vio
    if let Some(o) = rec.objetivos.iter_mut().find(|o| o.hwnd == hwnd) {
        o.resuelto = true;
        return BOOL::from(rec.objetivos.iter().any(|o| !o.resuelto));
    }
    // SAFETY: consultas de solo lectura sobre un HWND que EnumWindows acaba de dar; si la
    // ventana muere a mitad, las llamadas fallan y se descarta como candidata.
    let candidata = unsafe {
        // Las superposiciones que dejan pasar el ratón (WS_EX_TRANSPARENT: capturas, overlays de
        // GPU o de grabación) suelen cubrir toda la pantalla sin tapar nada de verdad
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || ex & WS_EX_TRANSPARENT.0 != 0 {
            false
        } else {
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut u32 as *mut _,
                std::mem::size_of::<u32>() as u32,
            );
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let mut buf = [0u16; 64];
            let n = GetClassNameW(hwnd, &mut buf).max(0) as usize;
            cloaked == 0 && pid != rec.pid && !clase_ignorada(&String::from_utf16_lossy(&buf[..n]))
        }
    };
    if candidata {
        let mut r = RECT::default();
        // SAFETY: `r` es un RECT local válido.
        if unsafe { GetWindowRect(hwnd, &mut r) }.is_ok() {
            let marco = (r.left, r.top, r.right, r.bottom);
            for o in rec.objetivos.iter_mut().filter(|o| !o.resuelto) {
                if contiene(marco, o.marco) {
                    o.tapado = true;
                    o.resuelto = true;
                }
            }
        }
    }
    BOOL::from(rec.objetivos.iter().any(|o| !o.resuelto))
}

/// Un solo recorrido del orden Z para todos los widgets. Un widget cuya ventana no está visible o
/// está minimizada (aún sin mostrar, o a punto de restaurarse) cuenta como a la vista: no hay
/// nada fiable que decir y así no se corta el arranque.
#[cfg(windows)]
fn calcular_visibles<'a>(app: &AppHandle, tipos: &[&'a str]) -> Vec<(&'a str, bool)> {
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowRect, IsIconic, IsWindowVisible};
    let mut objetivos: Vec<Objetivo> = Vec::new();
    for &tipo in tipos {
        let Some(hwnd) = app.get_webview_window(&etiqueta(tipo)).and_then(|v| v.hwnd().ok()) else { continue };
        let mut r = RECT::default();
        // SAFETY: consultas de solo lectura sobre la ventana propia; si falla, queda «a la vista».
        let usable = unsafe {
            IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() && GetWindowRect(hwnd, &mut r).is_ok()
        };
        objetivos.push(Objetivo {
            tipo,
            hwnd,
            marco: (r.left, r.top, r.right, r.bottom),
            resuelto: !usable,
            tapado: false,
        });
    }
    if objetivos.iter().any(|o| !o.resuelto) {
        let mut rec = Recorrido { objetivos: &mut objetivos, pid: std::process::id() };
        // SAFETY: el puntero a `rec` solo se usa dentro de esta llamada síncrona. Que el callback
        // devuelva FALSE para parar hace que EnumWindows dé error: se ignora a propósito.
        let _ = unsafe { EnumWindows(Some(paso_enum), LPARAM(&mut rec as *mut _ as isize)) };
    }
    objetivos.iter().map(|o| (o.tipo, !o.tapado)).collect()
}

#[tauri::command]
pub fn widgets_estado(app: AppHandle) -> EstadoWidgets {
    estado_actual(&app)
}

#[tauri::command]
pub fn fijar_edicion_widgets(app: AppHandle, activa: bool) {
    fijar_edicion(&app, activa);
}

/// `async` para que la ventana se cree fuera del hilo principal (como exige Tauri en Windows).
#[tauri::command]
pub async fn anadir_widget(app: AppHandle, tipo: String) -> Result<(), String> {
    abrir(&app, &tipo)
}

#[tauri::command]
pub fn quitar_widget(app: AppHandle, tipo: String) -> Result<(), String> {
    let (tipo, ..) = tipo_valido(&tipo).ok_or_else(|| format!("tipo de widget no válido: {tipo}"))?;
    let est = app.state::<Widgets>();
    if let Some(v) = app.get_webview_window(&etiqueta(tipo)) {
        // La geometría se guarda en CloseRequested; «Destroyed» lo saca de activos
        bloquear(&est.cerrando).insert(tipo.to_string());
        v.close().map_err(|e| e.to_string())?;
    } else {
        {
            let mut d = bloquear(&est.datos);
            d.activos.retain(|t| t != tipo);
            escribir(&app, &d);
        }
        avisar_cambio(&app);
    }
    Ok(())
}

#[tauri::command]
pub fn fijar_opacidad_widgets(app: AppHandle, valor: f64, guardar: Option<bool>) {
    let Some(est) = app.try_state::<Widgets>() else { return };
    {
        let mut d = bloquear(&est.datos);
        d.opacidad = acotar_opacidad(valor);
        // Mientras se arrastra el deslizador solo se aplica en vivo; el guardado llega al soltar
        if guardar.unwrap_or(true) {
            escribir(&app, &d);
        }
    }
    avisar_cambio(&app);
}

/// Registra el estado y abre los widgets activos. Las ventanas se crean fuera del hilo
/// principal, como exige Tauri en Windows.
pub fn iniciar(app: &AppHandle) {
    let mut datos = leer(app);
    let previos = datos.activos.clone();
    datos.activos.clear(); // `abrir` los va dando de alta
    app.manage(Widgets {
        datos: Mutex::new(datos),
        cerrando: Mutex::new(HashSet::new()),
        edicion: AtomicBool::new(false),
        saliendo: AtomicBool::new(false),
        encaje: Arc::new(ReboteEncaje::default()),
        rebote: Arc::new(Antirrebote::default()),
        visibles: Mutex::new(HashMap::new()),
        vigilando: AtomicBool::new(false),
        barra: Mutex::new(()),
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for tipo in previos {
            let _ = abrir(&app, &tipo);
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
        assert_eq!(tipo_valido("reloj"), Some(("reloj", 320.0, 160.0, 200.0, 100.0)));
        for bueno in ["sistema", "consumo", "tareas", "cuentas"] {
            assert!(tipo_valido(bueno).is_some(), "{bueno}");
        }
        for malo in ["", "meteo", "RELOJ", "../reloj", "reloj ", "widget-reloj"] {
            assert!(tipo_valido(malo).is_none(), "{malo:?}");
        }
    }

    #[test]
    fn los_tamanos_de_los_tipos_son_multiplos_de_la_rejilla_y_respetan_el_minimo() {
        for (t, a, h, ma, mh) in TIPOS {
            for v in [a, h, ma, mh] {
                assert_eq!(v % PASO, 0.0, "{t}");
            }
            assert!(a >= ma && h >= mh, "{t}");
        }
    }

    #[test]
    fn encajar_redondea_al_multiplo_mas_cercano() {
        let r = encajar(g(109, 131, 289, 211), (0, 0), 20, (0, 0));
        assert_eq!(r, g(100, 140, 280, 220));
        // Justo en la mitad se redondea hacia arriba
        assert_eq!(encajar(g(110, 10, 310, 190), (0, 0), 20, (0, 0)), g(120, 20, 320, 200));
        // Ya encajada: no cambia
        assert_eq!(encajar(g(100, 140, 280, 220), (0, 0), 20, (0, 0)), g(100, 140, 280, 220));
    }

    #[test]
    fn encajar_no_baja_de_los_minimos() {
        let r = encajar(g(0, 0, 130, 70), (0, 0), 20, (200, 100));
        assert_eq!((r.ancho, r.alto), (200, 100));
    }

    #[test]
    fn encajar_es_relativo_al_origen_del_monitor() {
        // Monitor a la izquierda del principal, con origen negativo y no múltiplo del paso
        let origen = (-1917, -3);
        let r = encajar(g(-1917 + 109, -3 + 131, 289, 211), origen, 20, (0, 0));
        assert_eq!(r, g(-1917 + 100, -3 + 140, 280, 220));
        // Idempotente
        assert_eq!(encajar(r, origen, 20, (0, 0)), r);
    }

    #[test]
    fn encajar_con_escala_usa_el_paso_fisico() {
        // Escala 1.5: paso 30, mínimo 300×150
        let r = encajar(g(44, 46, 310, 140), (0, 0), 30, (300, 150));
        assert_eq!(r, g(30, 60, 300, 150));
        // Paso 0 no divide por cero
        assert_eq!(encajar(g(5, 5, 50, 50), (0, 0), 0, (0, 0)), g(5, 5, 50, 50));
    }

    #[test]
    fn la_cascada_baja_y_se_aleja_de_la_esquina() {
        let area = Rect { x: 0, y: 0, ancho: 1920, alto: 1040 };
        assert_eq!(esquina_en_cascada(area, 1.0, 0, (320.0, 160.0)), g(1580, 20, 320, 160));
        assert_eq!(esquina_en_cascada(area, 1.0, 2, (280.0, 220.0)), g(1540, 100, 280, 220));
        // Escala 1.5 y área con origen
        let area = Rect { x: 100, y: 50, ancho: 2880, alto: 1560 };
        assert_eq!(esquina_en_cascada(area, 1.5, 1, (320.0, 160.0)), g(2410, 140, 480, 240));
    }

    fn area() -> Rect {
        Rect { x: 0, y: 0, ancho: 1920, alto: 1040 }
    }

    #[test]
    fn hueco_en_area_vacia_es_la_esquina_superior_derecha() {
        assert_eq!(buscar_hueco(area(), &[], (320, 160), 20, 20), Some(g(1580, 20, 320, 160)));
    }

    #[test]
    fn hueco_con_un_ocupado_va_debajo() {
        let o = [g(1580, 20, 320, 160)];
        assert_eq!(buscar_hueco(area(), &o, (320, 160), 20, 20), Some(g(1580, 200, 320, 160)));
    }

    #[test]
    fn hueco_con_columna_llena_pasa_a_la_izquierda() {
        let o = [g(1580, 20, 320, 1000)];
        assert_eq!(buscar_hueco(area(), &o, (320, 160), 20, 20), Some(g(1240, 20, 320, 160)));
    }

    #[test]
    fn hueco_con_area_llena_es_none() {
        let o = [g(0, 0, 1920, 1040)];
        assert_eq!(buscar_hueco(area(), &o, (320, 160), 20, 20), None);
        // Un widget más grande que el área tampoco cabe
        assert_eq!(buscar_hueco(area(), &[], (2000, 160), 20, 20), None);
    }

    #[test]
    fn el_json_se_lee_con_campos_ausentes() {
        let d = interpretar(Some("{}"));
        assert_eq!(d, Datos::default());
        assert_eq!(d.opacidad, 0.75);
        let d = interpretar(Some(r#"{"activos":["reloj"]}"#));
        assert_eq!(d.activos, vec!["reloj"]);
        assert!(d.geometrias.is_empty() && d.opacidad == 0.75);
        // Ilegible o ausente: valores por defecto
        assert_eq!(interpretar(Some("no es json")), Datos::default());
        assert_eq!(interpretar(None), Datos::default());
    }

    #[test]
    fn la_lectura_descarta_tipos_desconocidos_y_duplicados() {
        let texto = r#"{
            "activos": ["reloj", "meteo", "reloj", "tareas", "../x", "tareas"],
            "geometrias": {
                "reloj": {"x": 1, "y": 2, "ancho": 320, "alto": 160},
                "otro": {"x": 1, "y": 2, "ancho": 10, "alto": 10}
            },
            "opacidad": 5.0
        }"#;
        let d = interpretar(Some(texto));
        assert_eq!(d.activos, vec!["reloj", "tareas"]);
        assert_eq!(d.geometrias.len(), 1);
        assert_eq!(d.geometrias["reloj"], g(1, 2, 320, 160));
        assert_eq!(d.opacidad, 1.0);
    }

    #[test]
    fn el_json_se_escribe_y_se_lee_igual() {
        let mut d = Datos::default();
        d.activos.push("sistema".into());
        d.geometrias.insert("sistema".into(), g(10, 20, 280, 220));
        d.opacidad = 0.5;
        let texto = serde_json::to_string(&d).unwrap();
        assert_eq!(interpretar(Some(&texto)), d);
    }

    #[test]
    fn contiene_acepta_bordes_iguales_y_rechaza_un_pixel_fuera() {
        let w = (100, 50, 300, 250);
        assert!(contiene(w, w));
        assert!(contiene((0, 0, 1920, 1080), w));
        assert!(!contiene((101, 50, 300, 250), w));
        assert!(!contiene((100, 51, 300, 250), w));
        assert!(!contiene((100, 50, 299, 250), w));
        assert!(!contiene((100, 50, 300, 249), w));
        assert!(contiene((99, 49, 301, 251), w));
    }

    #[test]
    fn contiene_con_coordenadas_negativas() {
        // Monitor a la izquierda del principal
        let w = (-1800, -20, -1500, 180);
        assert!(contiene((-1920, -30, 0, 1050), w));
        assert!(contiene(w, w));
        assert!(!contiene((-1799, -30, 0, 1050), w));
        assert!(!contiene((-1920, -30, -1501, 1050), w));
        // Tapar solo en parte no es tapar
        assert!(!contiene((-1700, -30, 0, 1050), w));
    }

    #[test]
    fn el_escritorio_y_la_barra_de_tareas_no_tapan() {
        for c in ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"] {
            assert!(clase_ignorada(c), "{c}");
        }
        assert!(!clase_ignorada("Chrome_WidgetWin_1"));
        assert!(!clase_ignorada(""));
    }

    #[test]
    fn la_opacidad_se_acota() {
        assert_eq!(acotar_opacidad(0.0), 0.2);
        assert_eq!(acotar_opacidad(-3.0), 0.2);
        assert_eq!(acotar_opacidad(0.5), 0.5);
        assert_eq!(acotar_opacidad(7.0), 1.0);
        assert_eq!(acotar_opacidad(f64::NAN), 0.75);
        assert_eq!(acotar_opacidad(f64::INFINITY), 0.75);
    }
}
