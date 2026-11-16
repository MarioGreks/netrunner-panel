//! Módulo SISTEMA: muestreo de CPU, RAM, GPU, discos y batería.
//!
//! Un hilo mide cada segundo y emite `sistema://muestra`, pero solo mientras el panel
//! está visible; con el panel oculto duerme sin medir. El comando `muestra_sistema`
//! devuelve una muestra al momento para pintar en cuanto se abre el panel.

use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use sysinfo::{Disks, System};
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
        }
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

/// Registra el muestreador y arranca el hilo de emisión.
pub fn iniciar(app: &AppHandle) {
    app.manage(Mutex::new(Muestreador::nuevo()));
    let handle = app.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        let visible = handle
            .state::<Estado>()
            .panel_visible
            .load(Ordering::Relaxed);
        if !visible {
            continue; // oculto: no se mide
        }
        let muestra = handle.state::<Mutex<Muestreador>>().lock().unwrap().medir();
        let _ = handle.emit("sistema://muestra", muestra);
    });
}

#[cfg(test)]
mod pruebas {
    use super::agregar_gpu;

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
