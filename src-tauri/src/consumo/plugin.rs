//! El plugin «uso-consumo» de Claude Code (el que escribe `usage.json` con los límites de 5 h y
//! semanal): incrustado en el binario, instalación, estado y diagnóstico. Port de `buscar_claude`,
//! `listar_plugins`, `estado_plugin`, `instalar_plugin`, `diagnostico` y `hay_sesion` de Consumo.
//!
//! Las funciones puras (`estado_plugin`, `diagnostico`, `buscar_claude_en`, `VigiaPlugin::decidir`)
//! no tocan disco ni procesos. Lo que lanza `claude` (lento) solo se llama a petición o cuando hay
//! síntoma de falta de datos; nunca de forma periódica.

use std::ffi::OsStr;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use sysinfo::{Pid, ProcessesToUpdate, System};

pub const PLUGIN: &str = "uso-consumo@consumo-mods";
pub const MARKET: &str = "consumo-mods";
/// Segundos sin que el plugin actualice usage.json con Claude abierto antes de comprobar si carga.
pub const VIEJO: f64 = 6.0 * 3600.0;
/// Segundos con Claude abierto y sin ningún dato antes de comprobar solo si falta algo.
pub const SIN_DATOS: f64 = 90.0;
/// Tiempo máximo de cada orden de instalación.
const TIMEOUT_ORDEN: Duration = Duration::from_secs(90);
/// Tiempo máximo de `claude plugin list`.
const TIMEOUT_LISTA: Duration = Duration::from_secs(40);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// El plugin tal como se incrusta: (ruta relativa, contenido). Misma estructura y bytes que
/// `src-tauri/recursos/consumo-mod`.
pub const PLUGIN_INCRUSTADO: [(&str, &[u8]); 4] = [
    (
        ".claude-plugin/marketplace.json",
        include_bytes!("../../recursos/consumo-mod/.claude-plugin/marketplace.json"),
    ),
    (
        "uso-consumo/.claude-plugin/plugin.json",
        include_bytes!("../../recursos/consumo-mod/uso-consumo/.claude-plugin/plugin.json"),
    ),
    (
        "uso-consumo/hooks/hooks.json",
        include_bytes!("../../recursos/consumo-mod/uso-consumo/hooks/hooks.json"),
    ),
    (
        "uso-consumo/hooks/register.tsx",
        include_bytes!("../../recursos/consumo-mod/uso-consumo/hooks/register.tsx"),
    ),
];

/// Dónde carga Claude el plugin: `~/.claude/consumo-mod` (fuera de AppData: la app de Claude de la
/// Tienda de Windows ve %LOCALAPPDATA% en otro sitio y no lo carga).
pub fn mod_seguro(home: &Path) -> PathBuf {
    home.join(".claude").join("consumo-mod")
}

// ---------- búsqueda de `claude` ----------

/// Ruta del comando claude: en el PATH (con las extensiones habituales de Windows) o en los sitios
/// de siempre (`~/.local/bin/claude.exe`, `%APPDATA%/npm/claude.cmd`). Pura salvo `is_file`.
pub fn buscar_claude_en(path: Option<&OsStr>, home: &Path, appdata: Option<&Path>) -> Option<PathBuf> {
    let mut candidatos: Vec<PathBuf> = Vec::new();
    if let Some(p) = path {
        for dir in std::env::split_paths(p).filter(|d| !d.as_os_str().is_empty()) {
            for ext in ["exe", "cmd", "bat", "com"] {
                candidatos.push(dir.join(format!("claude.{ext}")));
            }
        }
    }
    candidatos.push(home.join(".local").join("bin").join("claude.exe"));
    if let Some(a) = appdata {
        candidatos.push(a.join("npm").join("claude.cmd"));
    }
    candidatos.into_iter().find(|c| c.is_file())
}

pub fn buscar_claude(home: &Path) -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    buscar_claude_en(std::env::var_os("PATH").as_deref(), home, appdata.as_deref()).or_else(|| {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        claude_de_escritorio(&local.join("Claude-Data"))
    })
}

/// El Claude Code que trae la app de escritorio (Tienda de Windows), que no está en el PATH:
/// `<Claude-Data>/claude_*/claude-code/<versión>/<hash>/claude.exe`. Se queda con el más reciente.
pub fn claude_de_escritorio(base: &Path) -> Option<PathBuf> {
    let subdirs = |p: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(p)
            .map(|it| it.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
            .unwrap_or_default()
    };
    let mut mejor: Option<(std::time::SystemTime, PathBuf)> = None;
    for perfil in subdirs(base) {
        let nombre = perfil.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !nombre.starts_with("claude_") {
            continue;
        }
        for version in subdirs(&perfil.join("claude-code")) {
            for hash in subdirs(&version) {
                let exe = hash.join("claude.exe");
                if let Ok(t) = exe.metadata().and_then(|m| m.modified()) {
                    if mejor.as_ref().map_or(true, |(tm, _)| t > *tm) {
                        mejor = Some((t, exe));
                    }
                }
            }
        }
    }
    mejor.map(|(_, p)| p)
}

// ---------- órdenes externas ----------

/// Ejecuta `programa args` sin ventana y con tope de tiempo (si se pasa, mata el proceso y
/// devuelve None; también None si no arranca). Con `capturar` devuelve la salida estándar.
pub(crate) fn ejecutar(programa: &Path, args: &[&OsStr], tope: Duration, capturar: bool) -> Option<String> {
    let mut cmd = Command::new(programa);
    cmd.args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(if capturar { Stdio::piped() } else { Stdio::null() })
        .creation_flags(CREATE_NO_WINDOW);
    let mut hijo = cmd.spawn().ok()?;
    let rx = hijo.stdout.take().map(|mut so| {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = so.read_to_end(&mut b);
            let _ = tx.send(b);
        });
        rx
    });
    let inicio = Instant::now();
    loop {
        match hijo.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if inicio.elapsed() < tope => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = hijo.kill();
                let _ = hijo.wait();
                return None;
            }
        }
    };
    // Si un proceso nieto sigue con la tubería abierta, no se espera más de unos segundos.
    let stdout = rx
        .and_then(|r| r.recv_timeout(Duration::from_secs(3)).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    Some(stdout)
}

/// Lo que dice `claude plugin list`, o None si no se pudo consultar.
pub fn listar_plugins(claude: &Path) -> Option<String> {
    ejecutar(claude, &[OsStr::new("plugin"), OsStr::new("list")], TIMEOUT_LISTA, true)
}

// ---------- estado y diagnóstico (puros) ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstadoPlugin {
    Falta,
    /// Está pero no carga.
    Fallo,
    Desactivado,
    Ok,
}

/// Estado del plugin según lo que dice `claude plugin list`.
pub fn estado_plugin(plugins: &str) -> EstadoPlugin {
    let Some((_, resto)) = plugins.split_once("uso-consumo") else {
        return EstadoPlugin::Falta;
    };
    let bloque = resto.split('❯').next().unwrap_or("");
    if bloque.contains("failed to load") {
        EstadoPlugin::Fallo
    } else if bloque.contains("disabled") {
        EstadoPlugin::Desactivado
    } else {
        EstadoPlugin::Ok
    }
}

/// Por qué Consumo no tiene datos (o que todo va bien). Serializa como el código en minúsculas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodigoDiagnostico {
    Ok,
    FalloCarga,
    SinClaude,
    Error,
    SinPlugin,
    Desactivado,
    SinDatos,
}

/// (código, texto) de por qué no hay datos. `hay_datos`: usage.json tiene alguna ventana;
/// `claude`: si se encontró el comando; `plugins`: lo que dijo `claude plugin list` (None si no
/// se pudo consultar); `mod_dir`: dónde queda el plugin, para el texto de instalación a mano.
pub fn diagnostico(
    hay_datos: bool,
    claude: bool,
    plugins: Option<&str>,
    mod_dir: &Path,
) -> (CodigoDiagnostico, String) {
    use CodigoDiagnostico::*;
    let manual = format!(
        "claude plugin marketplace add \"{}\"\nclaude plugin install {PLUGIN}",
        mod_dir.display()
    );
    let estado = plugins.map(estado_plugin);
    if estado == Some(EstadoPlugin::Fallo) {
        // Aunque haya datos (viejos): sin plugin no se actualizan.
        return (FalloCarga, "El plugin uso-consumo no carga (Claude no encuentra su carpeta), así que los límites no se actualizan.".into());
    }
    if hay_datos {
        return (Ok, "Todo bien: Consumo recibe tus límites de Claude.".into());
    }
    if !claude {
        return (SinClaude, format!("No encuentro el comando «claude». Instala el plugin a mano:\n{manual}"));
    }
    if plugins.is_none() {
        return (Error, "No pude consultar los plugins de Claude.\nPrueba más tarde.".into());
    }
    match estado {
        Some(EstadoPlugin::Falta) => (SinPlugin, "Falta el plugin uso-consumo, que es el que le pasa tus límites a Consumo.".into()),
        Some(EstadoPlugin::Desactivado) => (Desactivado, format!("El plugin uso-consumo está desactivado. Actívalo con:\nclaude plugin enable {PLUGIN}")),
        _ => (
            SinDatos,
            "El plugin está instalado, pero Claude aún no ha mandado datos.\nCierra Claude del todo, ábrelo y escribe un mensaje en una sesión de Code.\n(Con una API key no hay límites de plan que leer.)".into(),
        ),
    }
}

/// Con estos códigos y `claude` disponible se puede ofrecer instalar/arreglar el plugin.
pub fn se_puede_instalar(codigo: CodigoDiagnostico, claude: bool) -> bool {
    claude && matches!(codigo, CodigoDiagnostico::SinPlugin | CodigoDiagnostico::FalloCarga)
}

// ---------- instalación ----------

/// Escribe el plugin incrustado en `destino` (sobrescribe lo que haya, como `copytree`).
pub fn escribir_plugin(destino: &Path) -> std::io::Result<()> {
    for (ruta, bytes) in PLUGIN_INCRUSTADO {
        let f = destino.join(ruta);
        if let Some(p) = f.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(f, bytes)?;
    }
    Ok(())
}

/// Escribe el plugin en `destino` y lo (re)instala desde ahí: desinstala, quita y añade el
/// marketplace e instala (90 s por orden). True si después carga bien. Solo a petición del usuario.
pub fn instalar_plugin(claude: &Path, destino: &Path) -> bool {
    if escribir_plugin(destino).is_err() {
        return false;
    }
    let destino_os = destino.as_os_str();
    let ordenes: [Vec<&OsStr>; 4] = [
        vec![OsStr::new("plugin"), OsStr::new("uninstall"), OsStr::new(PLUGIN)],
        vec![OsStr::new("plugin"), OsStr::new("marketplace"), OsStr::new("remove"), OsStr::new(MARKET)],
        vec![OsStr::new("plugin"), OsStr::new("marketplace"), OsStr::new("add"), destino_os],
        vec![OsStr::new("plugin"), OsStr::new("install"), OsStr::new(PLUGIN)],
    ];
    for o in &ordenes {
        // Como en Python: una orden que no arranca o se pasa de tiempo aborta la instalación.
        if ejecutar(claude, o, TIMEOUT_ORDEN, false).is_none() {
            return false;
        }
    }
    listar_plugins(claude).is_some_and(|p| estado_plugin(&p) == EstadoPlugin::Ok)
}

// ---------- ¿Claude abierto? ----------

/// Los pid de los ficheros `*.json` de `sessions.d` (los que escribe Claude Code por sesión).
pub fn pids_de_sesiones(carpeta: &Path) -> Vec<u32> {
    let Ok(rd) = std::fs::read_dir(carpeta) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let t = std::fs::read_to_string(e.path()).ok()?;
            let v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()?;
            v.get("pid").and_then(Value::as_u64).filter(|p| *p > 0).map(|p| p as u32)
        })
        .collect()
}

/// ¿Hay algún Claude abierto? Primero lo barato: los pid de `sessions.d` (se comprueba solo esos
/// procesos); si no hay ninguno vivo, enumera procesos buscando `claude.exe` (caro: solo se llama
/// cuando ya hay síntoma de falta de datos).
pub fn hay_sesion(sesiones: &Path) -> bool {
    let pids = pids_de_sesiones(sesiones);
    if !pids.is_empty() {
        let lista: Vec<Pid> = pids.iter().map(|p| Pid::from_u32(*p)).collect();
        if let Ok(mut sys) = System::new() {
            sys.refresh_processes(ProcessesToUpdate::Some(&lista), true);
            if lista.iter().any(|p| sys.process(*p).is_some()) {
                return true;
            }
        }
    }
    let Ok(mut sys) = System::new() else {
        return false;
    };
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let hay = sys
        .processes()
        .values()
        .any(|p| p.name().to_string_lossy().eq_ignore_ascii_case("claude.exe"));
    hay
}

// ---------- cuándo diagnosticar solo ----------

/// Lo que recuerda el diagnóstico automático (tiempos en s desde la época).
#[derive(Debug, Default)]
pub struct VigiaPlugin {
    /// Desde cuándo hay síntoma con Claude abierto.
    t_sin_datos: Option<f64>,
    /// Cuándo se diagnosticó solo la última vez (no se repite hasta que vuelvan datos o pasen 6 h).
    t_diag: Option<f64>,
    /// Última consulta de `hay_sesion` y su resultado (para no enumerar procesos cada minuto).
    t_viva: Option<f64>,
    viva: bool,
}

/// Se reevalúa `hay_sesion` como mucho cada tanto.
const CADA_VIVA: f64 = 120.0;

impl VigiaPlugin {
    /// Decide si toca diagnosticar ahora: con Claude abierto y sin datos (o viejos) desde hace
    /// `SIN_DATOS`, y no se ha diagnosticado ya (o hace más de `VIEJO`). `viva` solo se llama si
    /// hay síntoma y hace falta. `hay_datos`: alguna ventana; `ts_local`: `ts` de usage.json.
    pub fn decidir(
        &mut self,
        hay_datos: bool,
        ts_local: f64,
        ahora: f64,
        viva: impl FnOnce() -> bool,
    ) -> bool {
        let sintoma = !hay_datos || ahora - ts_local > VIEJO;
        if !sintoma {
            self.t_sin_datos = None;
            self.t_diag = None; // han vuelto datos: la próxima falta se diagnostica de nuevo
            return false;
        }
        if self.t_diag.is_some_and(|t| ahora - t < VIEJO) {
            return false; // ya se diagnosticó: ni siquiera se mira si Claude está abierto
        }
        if self.t_viva.is_none_or(|t| ahora - t >= CADA_VIVA) {
            self.viva = viva();
            self.t_viva = Some(ahora);
        }
        if !self.viva {
            self.t_sin_datos = None;
            return false;
        }
        let desde = *self.t_sin_datos.get_or_insert(ahora);
        if ahora - desde >= SIN_DATOS {
            self.t_diag = Some(ahora);
            self.t_sin_datos = None;
            return true;
        }
        false
    }
}

