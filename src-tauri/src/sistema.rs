//! Módulo SISTEMA: muestreo de CPU, RAM, GPU, discos y batería.
//!
//! Un hilo mide cada segundo y emite `sistema://muestra`, pero solo mientras el panel
//! está visible (o hay una ventana tarjeta-sistema a la vista); si solo mira el widget de escritorio
//! mide cada 3 s; si nadie mira duerme sin medir. El comando `muestra_sistema`
//! devuelve una muestra al momento para pintar en cuanto se abre el panel.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use sysinfo::{Disks, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::estado::Estado;

#[derive(Serialize, Clone, Debug)]
pub struct Disco {
    pub nombre: String,
    pub montaje: String,
    pub libre: u64,
    pub total: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Bateria {
    pub porcentaje: f32,
    pub cargando: bool,
    pub minutos_restantes: Option<u32>,
}

#[derive(Serialize, Clone, Debug)]
pub struct MuestraSistema {
    pub cpu_total: f32,
    pub cpu_nucleos: Vec<f32>,
    pub ram_usada: u64,
    pub ram_total: u64,
    pub gpu_uso: Option<f32>,
    /// Pendiente (mejora futura): leer la temperatura de la GPU. Hoy siempre `None`.
    pub gpu_temp: Option<f32>,
    pub discos: Vec<Disco>,
    pub bateria: Option<Bateria>,
}

/// Muestras que guarda el historial: 10 minutos a una por segundo.
const MAX_HISTORIAL: usize = 600;
/// Segundos entre muestras cuando solo mira el widget de escritorio.
const PERIODO_WIDGET: u64 = 3;
/// Procesos que se devuelven por cada criterio (CPU y RAM).
const TOP_PROCESOS: usize = 8;

/// Muestra compacta para la gráfica de 10 minutos.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PuntoHistorial {
    /// Milisegundos desde la época Unix
    pub t: u64,
    pub cpu: f32,
    pub ram_pct: f32,
    pub gpu: Option<f32>,
}

/// Proceso (o grupo de procesos con el mismo nombre).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Proceso {
    pub nombre: String,
    /// PID del primero del grupo
    pub pid: u32,
    /// Procesos agrupados bajo este nombre
    pub cuantos: u32,
    /// % de CPU normalizado al total de la máquina
    pub cpu: f32,
    pub memoria: u64,
}

/// Añade un punto y recorta el búfer a `MAX_HISTORIAL`.
pub fn anadir_punto(hist: &mut VecDeque<PuntoHistorial>, p: PuntoHistorial) {
    hist.push_back(p);
    while hist.len() > MAX_HISTORIAL {
        hist.pop_front();
    }
}

/// Agrupa por nombre (suma CPU y RAM, cuenta) y devuelve la unión de los `n` que más CPU y
/// los `n` que más RAM gastan, de mayor a menor CPU. Entrada: (nombre, pid, cpu bruta, memoria);
/// la CPU bruta se divide entre `nucleos` para normalizarla al total de la máquina.
pub fn agrupar_procesos(crudos: Vec<(String, u32, f32, u64)>, nucleos: usize, n: usize) -> Vec<Proceso> {
    let div = nucleos.max(1) as f32;
    let mut grupos: HashMap<String, Proceso> = HashMap::new();
    for (nombre, pid, cpu, memoria) in crudos {
        let g = grupos.entry(nombre.clone()).or_insert(Proceso { nombre, pid, cuantos: 0, cpu: 0.0, memoria: 0 });
        g.cuantos += 1;
        g.cpu += cpu / div;
        g.memoria += memoria;
    }
    let mut todos: Vec<Proceso> = grupos.into_values().collect();
    todos.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(a.nombre.cmp(&b.nombre)));
    let mut sel: Vec<Proceso> = todos.iter().take(n).cloned().collect();
    todos.sort_by(|a, b| b.memoria.cmp(&a.memoria).then(a.nombre.cmp(&b.nombre)));
    for p in todos.into_iter().take(n) {
        if !sel.iter().any(|s| s.nombre == p.nombre) {
            sel.push(p);
        }
    }
    sel.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(a.nombre.cmp(&b.nombre)));
    sel
}

/// Suma el uso de todas las instancias del motor 3D de la GPU y lo limita a 0..=100.
/// Las instancias con valores no finitos o negativos se ignoran.
pub fn agregar_gpu(instancias: &[f64]) -> f32 {
    let suma: f64 = instancias
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .sum();
    suma.clamp(0.0, 100.0) as f32
}

/// Consulta PDH abierta una sola vez y reutilizada entre muestras.
#[cfg(windows)]
struct ConsultaGpu {
    consulta: windows::Win32::System::Performance::PDH_HQUERY,
    contador: windows::Win32::System::Performance::PDH_HCOUNTER,
}

// Los manejadores PDH son punteros opacos sin afinidad de hilo; el acceso va siempre
// detrás del `Mutex` de `Muestreador`, así que mover la consulta entre hilos es seguro.
#[cfg(windows)]
unsafe impl Send for ConsultaGpu {}

#[cfg(windows)]
impl ConsultaGpu {
    fn abrir() -> Option<Self> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Performance::*;

        let ruta: Vec<u16> = "\\GPU Engine(*engtype_3D)\\Utilization Percentage\0"
            .encode_utf16()
            .collect();
        let mut consulta = PDH_HQUERY::default();
        let mut contador = PDH_HCOUNTER::default();
        unsafe {
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut consulta) != 0 {
                return None;
            }
            if PdhAddEnglishCounterW(consulta, PCWSTR(ruta.as_ptr()), 0, &mut contador) != 0 {
                PdhCloseQuery(consulta);
                return None;
            }
            // Primera lectura: los contadores de tasa necesitan dos para dar valor
            PdhCollectQueryData(consulta);
        }
        Some(ConsultaGpu { consulta, contador })
    }

    fn leer(&mut self) -> Option<f32> {
        use windows::Win32::System::Performance::*;

        const PDH_MORE_DATA: u32 = 0x800007D2;
        unsafe {
            if PdhCollectQueryData(self.consulta) != 0 {
                return None;
            }
            let mut bytes: u32 = 0;
            let mut cuantos: u32 = 0;
            let r = PdhGetFormattedCounterArrayW(
                self.contador,
                PDH_FMT_DOUBLE,
                &mut bytes,
                &mut cuantos,
                None,
            );
            if r != PDH_MORE_DATA || bytes == 0 {
                return None;
            }
            // Buffer alineado a 8 bytes para los elementos del contador
            let mut buffer: Vec<u64> = vec![0; (bytes as usize).div_ceil(8)];
            let items = buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            let r = PdhGetFormattedCounterArrayW(
                self.contador,
                PDH_FMT_DOUBLE,
                &mut bytes,
                &mut cuantos,
                Some(items),
            );
            if r != 0 {
                return None;
            }
            let valores: Vec<f64> = std::slice::from_raw_parts(items, cuantos as usize)
                .iter()
                .filter(|it| it.FmtValue.CStatus == 0)
                .map(|it| it.FmtValue.Anonymous.doubleValue)
                .collect();
            if valores.is_empty() {
                return None;
            }
            Some(agregar_gpu(&valores))
        }
    }
}

#[cfg(windows)]
impl Drop for ConsultaGpu {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Performance::PdhCloseQuery(self.consulta);
        }
    }
}

#[cfg(not(windows))]
struct ConsultaGpu;

#[cfg(not(windows))]
impl ConsultaGpu {
    fn abrir() -> Option<Self> {
        None
    }
    fn leer(&mut self) -> Option<f32> {
        None
    }
}

/// Objeto de medición reutilizado entre muestras.
pub struct Muestreador {
    sys: Option<System>,
    discos: Option<Disks>,
    gpu: Option<ConsultaGpu>,
    historial: VecDeque<PuntoHistorial>,
}

impl Muestreador {
    fn nuevo() -> Self {
        let mut sys = System::new().ok();
        if let Some(s) = sys.as_mut() {
            s.refresh_cpu_usage();
            s.refresh_memory();
        }
        Muestreador {
            sys,
            discos: Disks::new_with_refreshed_list().ok(),
            gpu: ConsultaGpu::abrir(),
            historial: VecDeque::with_capacity(MAX_HISTORIAL + 1),
        }
    }

    /// Guarda la muestra en el historial. Solo lo llama el hilo (una vez por segundo), no
    /// `muestra_sistema`, para que las consultas sueltas no ensucien la escala de tiempo.
    fn registrar(&mut self, m: &MuestraSistema) {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let ram_pct = if m.ram_total > 0 { m.ram_usada as f32 / m.ram_total as f32 * 100.0 } else { 0.0 };
        anadir_punto(&mut self.historial, PuntoHistorial { t, cpu: m.cpu_total, ram_pct, gpu: m.gpu_uso });
    }

    /// Refresca solo CPU y memoria de los procesos. La CPU por proceso necesita dos lecturas
    /// separadas en el tiempo: la primera llamada puede dar 0.
    fn procesos(&mut self) -> Vec<Proceso> {
        let Some(s) = self.sys.as_mut() else { return Vec::new() };
        s.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let crudos = s
            .processes()
            .iter()
            .map(|(pid, p)| (p.name().to_string_lossy().into_owned(), pid.as_u32(), p.cpu_usage(), p.memory()))
            .collect();
        agrupar_procesos(crudos, s.cpus().len(), TOP_PROCESOS)
    }

    fn medir(&mut self) -> MuestraSistema {
        if let Some(s) = self.sys.as_mut() {
            s.refresh_cpu_usage();
            s.refresh_memory();
        }
        if let Some(d) = self.discos.as_mut() {
            d.refresh(true);
        }

        let discos = self
            .discos
            .iter()
            .flat_map(|d| d.iter())
            .filter(|d| d.total_space() > 0)
            .map(|d| Disco {
                nombre: d.name().to_string_lossy().into_owned(),
                montaje: d.mount_point().to_string_lossy().into_owned(),
                libre: d.available_space(),
                total: d.total_space(),
            })
            .collect();

        MuestraSistema {
            cpu_total: self.sys.as_ref().map_or(0.0, |s| s.global_cpu_usage()),
            cpu_nucleos: self
                .sys
                .as_ref()
                .map_or_else(Vec::new, |s| s.cpus().iter().map(|c| c.usage()).collect()),
            ram_usada: self.sys.as_ref().map_or(0, |s| s.used_memory()),
            ram_total: self.sys.as_ref().map_or(0, |s| s.total_memory()),
            gpu_uso: self.gpu.as_mut().and_then(|g| g.leer()),
            // Mejora futura: temperatura de GPU (NVML/ADLX/WMI); de momento no disponible
            gpu_temp: None,
            discos,
            bateria: leer_bateria(),
        }
    }
}

/// Primera batería del equipo, o `None` en un sobremesa. El gestor se crea en cada
/// llamada para que `Muestreador` pueda compartirse entre hilos.
fn leer_bateria() -> Option<Bateria> {
    use starship_battery::units::ratio::percent;
    use starship_battery::units::time::minute;
    use starship_battery::{Manager, State};

    let gestor = Manager::new().ok()?;
    let bat = gestor.batteries().ok()?.flatten().next()?;
    let estado = bat.state();
    let cargando = matches!(estado, State::Charging);
    let tiempo = if cargando {
        bat.time_to_full()
    } else {
        bat.time_to_empty()
    };
    Some(Bateria {
        porcentaje: bat.state_of_charge().get::<percent>(),
        cargando,
        minutos_restantes: tiempo.map(|t| t.get::<minute>().round() as u32),
    })
}

/// Una muestra al momento, para pintar en cuanto se abre el panel.
#[tauri::command]
pub fn muestra_sistema(muestreador: State<'_, Mutex<Muestreador>>) -> MuestraSistema {
    muestreador.lock().unwrap().medir()
}

/// Últimos 10 minutos de CPU, RAM y GPU, para pintar la gráfica al abrir la ventana.
#[tauri::command]
pub fn historial_sistema(muestreador: State<'_, Mutex<Muestreador>>) -> Vec<PuntoHistorial> {
    muestreador.lock().unwrap().historial.iter().cloned().collect()
}

/// Procesos que más CPU y RAM gastan, agrupados por nombre.
#[tauri::command]
pub fn procesos_sistema(muestreador: State<'_, Mutex<Muestreador>>) -> Vec<Proceso> {
    muestreador.lock().unwrap().procesos()
}

/// ¿Hay una ventana tarjeta-sistema a la vista (existe, visible y sin minimizar)?
fn ventana_sistema_a_la_vista(app: &AppHandle) -> bool {
    app.get_webview_window("tarjeta-sistema")
        .is_some_and(|v| v.is_visible().unwrap_or(false) && !v.is_minimized().unwrap_or(false))
}

/// Registra el muestreador y arranca el hilo de emisión.
pub fn iniciar(app: &AppHandle) {
    app.manage(Mutex::new(Muestreador::nuevo()));
    let handle = app.clone();
    let mut segundos: u64 = 0;
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        segundos = segundos.wrapping_add(1);
        let visible = handle
            .state::<Estado>()
            .panel_visible
            .load(Ordering::Relaxed);
        if !visible && !ventana_sistema_a_la_vista(&handle) {
            // Solo mira el widget: basta una muestra cada 3 s
            if !crate::widgets::mirando(&handle, "sistema") || segundos % PERIODO_WIDGET != 0 {
                continue; // nadie mira (o toca esperar): no se mide
            }
        }
        let muestra = {
            let estado = handle.state::<Mutex<Muestreador>>();
            let mut m = estado.lock().unwrap();
            let muestra = m.medir();
            m.registrar(&muestra);
            muestra
        };
        let _ = handle.emit("sistema://muestra", muestra);
    });
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn pr(n: &str, pid: u32, cpu: f32, mem: u64) -> (String, u32, f32, u64) {
        (n.to_string(), pid, cpu, mem)
    }

    #[test]
    fn historial_se_recorta_a_600() {
        let mut h = VecDeque::new();
        for i in 0..650u64 {
            anadir_punto(&mut h, PuntoHistorial { t: i, cpu: 0.0, ram_pct: 0.0, gpu: None });
        }
        assert_eq!(h.len(), 600);
        assert_eq!(h.front().unwrap().t, 50);
        assert_eq!(h.back().unwrap().t, 649);
    }

    #[test]
    fn agrupa_suma_normaliza_y_ordena() {
        let r = agrupar_procesos(
            vec![pr("chrome.exe", 1, 20.0, 100), pr("chrome.exe", 2, 20.0, 200), pr("a.exe", 3, 10.0, 50)],
            4,
            8,
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].nombre, "chrome.exe");
        assert_eq!((r[0].cuantos, r[0].cpu, r[0].memoria), (2, 10.0, 300));
        assert_eq!(r[1].cpu, 2.5);
    }

    #[test]
    fn une_top_cpu_y_top_ram_sin_duplicar() {
        let r = agrupar_procesos(
            vec![pr("a", 1, 50.0, 1), pr("b", 2, 40.0, 1), pr("c", 3, 0.0, 999), pr("d", 4, 0.0, 0)],
            1,
            2,
        );
        let nombres: Vec<&str> = r.iter().map(|p| p.nombre.as_str()).collect();
        assert_eq!(nombres, ["a", "b", "c"]);
    }

    #[test]
    fn suma_las_instancias() {
        assert_eq!(agregar_gpu(&[10.0, 20.5, 4.5]), 35.0);
    }

    #[test]
    fn limita_a_cien() {
        assert_eq!(agregar_gpu(&[60.0, 70.0]), 100.0);
    }

    #[test]
    fn vacio_y_valores_invalidos() {
        assert_eq!(agregar_gpu(&[]), 0.0);
        assert_eq!(agregar_gpu(&[f64::NAN, -5.0, 12.0]), 12.0);
    }
}
