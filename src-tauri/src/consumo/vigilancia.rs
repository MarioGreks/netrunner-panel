//! Plugin «uso-consumo» y espacio en disco, a nivel de aplicación: comandos, diagnóstico
//! automático (se engancha al hilo muestreador) y medida de disco (se engancha al hilo lector).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::disco::{
    carpetas_historial, datos_disco, espacio_libre, leer_gb_max, medir_datos, DatosDisco, NivelDisco,
};
use super::limites::leer_uso;
use super::plugin::{
    buscar_claude, diagnostico, hay_sesion, instalar_plugin, listar_plugins, mod_seguro,
    se_puede_instalar, CodigoDiagnostico, VigiaPlugin, PLUGIN,
};
use super::transcripciones::raiz_proyectos;
use super::{ahora_s, bloquear, Consumo};

/// Como mucho una medida de disco (recorre carpetas) cada tanto.
const CADA_MEDIDA: Duration = Duration::from_secs(30 * 60);

/// Estado del plugin y del espacio de datos.
pub struct Vigilancia {
    home: PathBuf,
    vigia: Mutex<VigiaPlugin>,
    /// Última medida de disco.
    datos: Mutex<DatosDisco>,
    t_medida: Mutex<Option<Instant>>,
    /// El aviso de máximo de GB ya salió en este arranque.
    avisado_disco: AtomicBool,
    /// Hay una instalación en curso.
    instalando: AtomicBool,
}

impl Vigilancia {
    pub fn nuevo(home: PathBuf) -> Vigilancia {
        Vigilancia {
            home,
            vigia: Mutex::new(VigiaPlugin::default()),
            datos: Mutex::new(DatosDisco::default()),
            t_medida: Mutex::new(None),
            avisado_disco: AtomicBool::new(false),
            instalando: AtomicBool::new(false),
        }
    }

    fn statusbar(&self) -> PathBuf {
        self.home.join(".claude").join("statusbar")
    }
}

/// Resultado del diagnóstico del plugin.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticoConsumo {
    pub codigo: CodigoDiagnostico,
    pub texto: String,
    /// Se puede ofrecer instalar/arreglar el plugin (sin_plugin o fallo_carga, con `claude`).
    pub puede_instalar: bool,
}

/// Averigua por qué no hay datos: llama a `claude plugin list` (lento, hasta 40 s).
pub fn diagnosticar(home: &Path) -> DiagnosticoConsumo {
    let claude = buscar_claude(home);
    let plugins = claude.as_deref().and_then(listar_plugins);
    let uso = leer_uso(&home.join(".claude").join("statusbar").join("usage.json"));
    let hay_datos = uso.semanal.is_some() || uso.cinco_horas.is_some();
    let (codigo, texto) = diagnostico(hay_datos, claude.is_some(), plugins.as_deref(), &mod_seguro(home));
    DiagnosticoConsumo { codigo, texto, puede_instalar: se_puede_instalar(codigo, claude.is_some()) }
}

/// Diagnóstico a petición (también si todo va bien). Corre fuera del hilo de la interfaz.
#[tauri::command]
pub async fn diagnostico_consumo(app: AppHandle) -> DiagnosticoConsumo {
    let home = app.state::<Vigilancia>().home.clone();
    tauri::async_runtime::spawn_blocking(move || diagnosticar(&home))
        .await
        .unwrap_or_else(|_| DiagnosticoConsumo {
            codigo: CodigoDiagnostico::Error,
            texto: "No pude consultar los plugins de Claude.\nPrueba más tarde.".into(),
            puede_instalar: false,
        })
}

/// Instala (o arregla) el plugin: solo la pide el usuario. Texto de resultado como `arreglar`.
#[tauri::command]
pub async fn instalar_plugin_consumo(app: AppHandle) -> Result<String, String> {
    let home = app.state::<Vigilancia>().home.clone();
    let destino = mod_seguro(&home);
    let manual = format!("claude plugin marketplace add \"{}\"\nclaude plugin install {PLUGIN}", destino.display());
    let Some(claude) = buscar_claude(&home) else {
        return Err(format!("No encuentro el comando «claude». Instala el plugin a mano:\n{manual}"));
    };
    if app.state::<Vigilancia>().instalando.swap(true, Ordering::SeqCst) {
        return Err("Ya se está instalando el plugin.".into());
    }
    let ok = tauri::async_runtime::spawn_blocking(move || instalar_plugin(&claude, &destino))
        .await
        .unwrap_or(false);
    app.state::<Vigilancia>().instalando.store(false, Ordering::SeqCst);
    if ok {
        Ok("Plugin instalado. Cierra Claude del todo, ábrelo y escribe un mensaje:\nen unos segundos habrá datos.".into())
    } else {
        Err(format!("No se pudo instalar el plugin. Prueba a mano:\n{manual}"))
    }
}

/// Lo último medido del espacio de datos (se mide a los ~30 s de arrancar y luego como mucho cada
/// 30 min). Sin medir aún: `gb` es None.
#[tauri::command]
pub fn datos_consumo(vig: State<'_, Vigilancia>) -> DatosDisco {
    bloquear(&vig.datos).clone()
}

/// Diagnóstico automático (en el hilo muestreador): con Claude abierto y sin datos desde hace
/// 90 s, o con datos de más de 6 h, diagnostica una vez y avisa con `consumo://diagnostico` si no
/// es ok. Nunca instala nada.
pub fn vigilar_plugin(app: &AppHandle) {
    let c = app.state::<Consumo>();
    let v = app.state::<Vigilancia>();
    let (hay, ts) = {
        let g = bloquear(&c.interno);
        (g.uso.semanal.is_some() || g.uso.cinco_horas.is_some(), g.uso_local.ts)
    };
    let sesiones = v.statusbar().join("sessions.d");
    let toca = bloquear(&v.vigia).decidir(hay, ts, ahora_s(), || hay_sesion(&sesiones));
    if toca {
        let d = diagnosticar(&v.home);
        if d.codigo != CodigoDiagnostico::Ok {
            let _ = app.emit("consumo://diagnostico", d);
        }
    }
}

/// Mide el espacio de datos si pasaron 30 min desde la última medida y avisa con
/// `consumo://datos` si pasa del máximo (una vez por arranque).
pub fn medir_disco(app: &AppHandle) {
    let v = app.state::<Vigilancia>();
    {
        let mut t = bloquear(&v.t_medida);
        if t.is_some_and(|t| t.elapsed() < CADA_MEDIDA) {
            return;
        }
        *t = Some(Instant::now());
    }
    let sync = bloquear(&app.state::<Consumo>().equipos).activas();
    let barra = v.statusbar();
    let proyectos = raiz_proyectos();
    let gb = medir_datos(&proyectos, &carpetas_historial(&sync, &barra), None);
    let gb_max = leer_gb_max(&barra.join("consumo_ajustes.json"));
    let datos = datos_disco(gb, gb_max, espacio_libre(&proyectos));
    *bloquear(&v.datos) = datos.clone();
    if datos.nivel == NivelDisco::Lleno && !v.avisado_disco.swap(true, Ordering::SeqCst) {
        let _ = app.emit("consumo://datos", datos);
    }
}
