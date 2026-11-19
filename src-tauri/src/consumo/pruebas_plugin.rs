//! Pruebas del plugin y del espacio en disco, portadas de la función `prueba` de Consumo. Todo en
//! carpetas temporales propias; nunca se instala nada ni se toca `~/.claude`.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::disco::*;
use super::plugin::CodigoDiagnostico as C;
use super::plugin::*;

fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("netrunner_plugin_{nombre}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

const PL: &str = "Installed plugins:\n  ❯ uso-consumo@consumo-mods\n    Status: ✔ enabled\n  ❯ otro@x\n    Status: ✔ enabled";
const PF: &str = "Installed plugins:\n  ❯ uso-consumo@consumo-mods\n    Status: ✘ failed to load\n    Error: Path escapes plugin directory\n  ❯ otro@x\n    Status: ✔ enabled";

#[test]
fn estados_del_plugin() {
    let desactivado = PL.replacen("✔ enabled", "✘ disabled", 1);
    assert_eq!(estado_plugin(PL), EstadoPlugin::Ok);
    assert_eq!(estado_plugin(PF), EstadoPlugin::Fallo);
    assert_eq!(estado_plugin(&desactivado), EstadoPlugin::Desactivado);
    assert_eq!(estado_plugin("Installed plugins:"), EstadoPlugin::Falta);
    // «disabled» de OTRO plugin posterior no cuenta
    let otro = "Installed plugins:\n  ❯ uso-consumo@consumo-mods\n    Status: ✔ enabled\n  ❯ otro@x\n    Status: ✘ disabled";
    assert_eq!(estado_plugin(otro), EstadoPlugin::Ok);
}

#[test]
fn codigos_de_diagnostico() {
    let m = Path::new("C:\\u\\.claude\\consumo-mod");
    let cod = |d, c, p: Option<&str>| diagnostico(d, c, p, m).0;
    assert_eq!(cod(true, false, None), C::Ok);
    assert_eq!(cod(false, false, None), C::SinClaude);
    assert_eq!(cod(false, true, None), C::Error);
    assert_eq!(cod(false, true, Some("Installed plugins:\n  ❯ otro@x")), C::SinPlugin);
    assert_eq!(cod(false, true, Some(PL)), C::SinDatos);
    assert_eq!(cod(false, true, Some(PF)), C::FalloCarga);
    assert_eq!(cod(true, true, Some(PF)), C::FalloCarga); // aunque haya datos (viejos)
    assert_eq!(cod(false, true, Some(&PL.replacen("✔ enabled", "✘ disabled", 1))), C::Desactivado);
    assert_eq!(cod(true, true, Some(PL)), C::Ok);
    // textos
    let (_, t) = diagnostico(false, false, None, m);
    assert!(t.starts_with("No encuentro el comando «claude»") && t.contains("consumo-mod") && t.contains("claude plugin install uso-consumo@consumo-mods"));
    assert!(diagnostico(false, true, Some(PL), m).1.contains("Claude aún no ha mandado datos"));
    assert!(diagnostico(false, true, Some(&PL.replacen("✔ enabled", "✘ disabled", 1)), m).1.contains("claude plugin enable uso-consumo@consumo-mods"));
    // se puede instalar solo con sin_plugin / fallo_carga y claude
    assert!(se_puede_instalar(C::SinPlugin, true) && se_puede_instalar(C::FalloCarga, true));
    assert!(!se_puede_instalar(C::SinPlugin, false) && !se_puede_instalar(C::SinDatos, true) && !se_puede_instalar(C::Ok, true));
    // el código viaja como en Python
    assert_eq!(serde_json::to_string(&C::FalloCarga).unwrap(), "\"fallo_carga\"");
    assert_eq!(serde_json::to_string(&C::SinClaude).unwrap(), "\"sin_claude\"");
}

#[test]
fn plugin_incrustado_igual_al_del_repo() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("recursos").join("consumo-mod");
    assert_eq!(PLUGIN_INCRUSTADO.len(), 4);
    for (ruta, bytes) in PLUGIN_INCRUSTADO {
        let en_disco = fs::read(base.join(ruta)).unwrap_or_else(|_| panic!("falta {ruta}"));
        assert!(!bytes.is_empty() && en_disco == bytes, "{ruta} difiere");
    }
    // y no hay ficheros de más en la carpeta del repo
    fn contar(d: &Path) -> usize {
        fs::read_dir(d).unwrap().flatten().map(|e| if e.path().is_dir() { contar(&e.path()) } else { 1 }).sum()
    }
    assert_eq!(contar(&base), 4);
}

#[test]
fn escribir_plugin_crea_la_estructura() {
    let d = carpeta("escribir").join(".claude").join("consumo-mod");
    escribir_plugin(&d).unwrap();
    for (ruta, bytes) in PLUGIN_INCRUSTADO {
        assert_eq!(fs::read(d.join(ruta)).unwrap(), bytes);
    }
    // sobrescribe lo que haya (como copytree dirs_exist_ok)
    fs::write(d.join("uso-consumo/hooks/hooks.json"), "roto").unwrap();
    escribir_plugin(&d).unwrap();
    assert_eq!(fs::read(d.join("uso-consumo/hooks/hooks.json")).unwrap(), PLUGIN_INCRUSTADO[2].1);
    assert_eq!(mod_seguro(Path::new("C:\\u")), Path::new("C:\\u\\.claude\\consumo-mod"));
}

#[test]
fn busca_claude_en_path_y_sitios_habituales() {
    let d = carpeta("claude");
    let (bin, home) = (d.join("bin"), d.join("home"));
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    let sin: Option<&OsStr> = None;
    assert_eq!(buscar_claude_en(sin, &home, None), None);
    // en ~/.local/bin
    fs::write(home.join(".local/bin/claude.exe"), "x").unwrap();
    assert_eq!(buscar_claude_en(sin, &home, None), Some(home.join(".local/bin/claude.exe")));
    // el del PATH (con extensión .cmd) gana
    fs::write(bin.join("claude.cmd"), "x").unwrap();
    let path = std::env::join_paths([d.join("nada"), bin.clone()]).unwrap();
    assert_eq!(buscar_claude_en(Some(&path), &home, None), Some(bin.join("claude.cmd")));
    // en %APPDATA%/npm
    let (h2, ap) = (d.join("h2"), d.join("ap"));
    fs::create_dir_all(ap.join("npm")).unwrap();
    fs::write(ap.join("npm/claude.cmd"), "x").unwrap();
    assert_eq!(buscar_claude_en(sin, &h2, Some(&ap)), Some(ap.join("npm/claude.cmd")));
}

#[test]
fn ordenes_con_tope_de_tiempo() {
    let cmd = Path::new("C:\\Windows\\System32\\cmd.exe");
    if !cmd.is_file() {
        return;
    }
    let s = ejecutar(cmd, &[OsStr::new("/c"), OsStr::new("echo hola")], Duration::from_secs(20), true).unwrap();
    assert_eq!(s.trim(), "hola");
    // sin captura no devuelve nada
    assert_eq!(ejecutar(cmd, &[OsStr::new("/c"), OsStr::new("echo hola")], Duration::from_secs(20), false).unwrap(), "");
    // una orden larga se mata al pasar del tope
    let t = Instant::now();
    let r = ejecutar(cmd, &[OsStr::new("/c"), OsStr::new("ping -n 30 127.0.0.1 >nul")], Duration::from_secs(1), true);
    assert!(r.is_none() && t.elapsed() < Duration::from_secs(10));
    // un programa que no existe: None
    assert!(ejecutar(Path::new("C:\\no\\existe.exe"), &[], Duration::from_secs(1), true).is_none());
}

#[test]
fn sesiones_y_claude_abierto() {
    let d = carpeta("sesiones");
    assert!(pids_de_sesiones(&d.join("no_existe")).is_empty());
    fs::write(d.join("a.json"), format!("{{\"pid\": {}}}", std::process::id())).unwrap();
    fs::write(d.join("b.json"), "no es json").unwrap();
    fs::write(d.join("c.json"), "{\"pid\": 0}").unwrap();
    fs::write(d.join("d.json"), "\u{feff}{\"pid\": 4242}").unwrap();
    let mut p = pids_de_sesiones(&d);
    p.sort();
    let mut esperado = vec![4242, std::process::id()];
    esperado.sort();
    assert_eq!(p, esperado);
    // con el pid de este mismo proceso, hay sesión viva (sin enumerar procesos)
    assert!(hay_sesion(&d));
}

#[test]
fn diagnostico_automatico_una_sola_vez() {
    let mut v = VigiaPlugin::default();
    let (sin, ahora) = (false, 1_000_000.0);
    let mut llamadas = 0;
    // con datos recientes: nada y ni se pregunta por Claude
    assert!(!v.decidir(true, ahora - 60.0, ahora, || panic!("no debe consultar")));
    // sin datos y Claude cerrado: no
    assert!(!v.decidir(sin, 0.0, ahora, || {
        llamadas += 1;
        false
    }));
    assert_eq!(llamadas, 1);
    // dentro de los 2 min no se vuelve a consultar
    assert!(!v.decidir(sin, 0.0, ahora + 60.0, || panic!("cacheado")));
    // Claude se abre: empieza la cuenta aquí
    assert!(!v.decidir(sin, 0.0, ahora + 130.0, || true));
    assert!(!v.decidir(sin, 0.0, ahora + 190.0, || panic!("cacheado"))); // 60 s < 90 s
    assert!(v.decidir(sin, 0.0, ahora + 250.0, || true)); // 120 s: toca
    // ya diagnosticado: no se repite aunque siga sin datos (ni consulta nada)
    assert!(!v.decidir(sin, 0.0, ahora + 310.0, || panic!("no debe consultar")));
    assert!(!v.decidir(sin, 0.0, ahora + 5.0 * 3600.0, || panic!("no debe consultar")));
    // pasadas 6 h se puede repetir
    assert!(!v.decidir(sin, 0.0, ahora + 6.0 * 3600.0 + 400.0, || true));
    assert!(v.decidir(sin, 0.0, ahora + 6.0 * 3600.0 + 520.0, || true));
    // vuelven datos y luego faltan otra vez: de nuevo
    let t = ahora + 7.0 * 3600.0;
    assert!(!v.decidir(true, t - 10.0, t, || panic!("no")));
    assert!(!v.decidir(sin, 0.0, t + 10.0, || true));
    assert!(v.decidir(sin, 0.0, t + 130.0, || true));
}

#[test]
fn datos_viejos_con_claude_abierto_tambien_diagnostican() {
    let mut v = VigiaPlugin::default();
    let ahora = 2_000_000.0;
    let viejo = ahora - 7.0 * 3600.0; // hay ventanas pero ts de hace 7 h
    assert!(!v.decidir(true, viejo, ahora, || true));
    assert!(v.decidir(true, viejo, ahora + 100.0, || true));
    // de hace 5 h: aún no es sospechoso
    let mut w = VigiaPlugin::default();
    assert!(!w.decidir(true, ahora - 5.0 * 3600.0, ahora, || panic!("no")));
}

#[test]
fn texto_y_nivel_del_espacio() {
    let t = texto_datos;
    assert_eq!(t(1.234, 0.0), ("◔ 1,23 GB".to_string(), None, NivelDisco::Ok));
    assert_eq!(t(1.234, -3.0).2, NivelDisco::Ok);
    let (txt, fr, n) = t(2.5, 5.0);
    assert_eq!((txt.as_str(), fr, n), ("◔ 2,50 / 5 GB", Some(0.5), NivelDisco::Ok));
    assert_eq!(t(4.25, 5.0), ("◔ 4,25 / 5 GB".to_string(), Some(0.85), NivelDisco::Aviso));
    assert_eq!(t(4.0, 5.0).2, NivelDisco::Aviso); // justo el 80 %
    assert_eq!(t(5.0, 5.0).2, NivelDisco::Aviso); // igual al máximo no es «lleno»
    let (txt, fr, n) = t(6.0, 5.0);
    assert_eq!((txt.as_str(), fr, n), ("◔ 6,00 / 5 GB", Some(1.0), NivelDisco::Lleno));
    assert_eq!(t(1.0, 12.5).0, "◔ 1,00 / 12,5 GB");
    let d = datos_disco(6.0, 5.0, Some(10 * 1024 * 1024 * 1024));
    assert_eq!((d.gb, d.libre, d.nivel), (Some(6.0), Some(10.0), NivelDisco::Lleno));
    let j = serde_json::to_value(&d).unwrap();
    assert_eq!(j["gbMax"], 5.0);
    assert_eq!(j["nivel"], "lleno");
    assert!(serde_json::to_value(DatosDisco::default()).unwrap()["gb"].is_null());
}

#[test]
fn gb_max_de_los_ajustes() {
    let d = carpeta("ajustes");
    let f = d.join("consumo_ajustes.json");
    assert_eq!(leer_gb_max(&f), 0.0); // sin fichero
    for (texto, esperado) in [
        ("{\"gb_max\": 50}", 50.0),
        ("{\"gb_max\": 2.5}", 2.5),
        ("\u{feff}{\"gb_max\": 7}", 7.0),
        ("{\"gb_max\": -1}", 0.0),
        ("{\"gb_max\": \"x\"}", 0.0),
        ("{\"gb_max\": null}", 0.0),
        ("{\"otra\": 3}", 0.0),
        ("roto", 0.0),
        ("{\"gb_max\": 99999999}", 1e6),
    ] {
        fs::write(&f, texto).unwrap();
        assert_eq!(leer_gb_max(&f), esperado, "{texto}");
    }
}

#[test]
fn medir_carpetas_y_espacio_libre() {
    let d = carpeta("medir");
    let (proy, h1, h2) = (d.join("projects"), d.join("sync/historial"), d.join("local/historial"));
    for c in [&proy.join("a/b"), &h1, &h2] {
        fs::create_dir_all(c).unwrap();
    }
    fs::write(proy.join("a/x.jsonl"), vec![0u8; 1000]).unwrap();
    fs::write(proy.join("a/b/y.jsonl"), vec![0u8; 500]).unwrap();
    fs::write(h1.join("s.csv"), vec![0u8; 2000]).unwrap();
    fs::write(h2.join("l.csv"), vec![0u8; 4000]).unwrap();
    assert_eq!(tam_carpeta(&proy), 1500);
    assert_eq!(tam_carpeta(&d.join("no_existe")), 0);
    let gb = |n: u64| n as f64 / (1024.0 * 1024.0 * 1024.0);
    assert_eq!(medir_datos(&proy, std::slice::from_ref(&h1), None), gb(3500));
    assert_eq!(medir_datos(&proy, &[h1.clone(), d.join("otra")], None), gb(3500));
    assert_eq!(medir_datos(&proy, std::slice::from_ref(&h1), Some(&d.join("local"))), gb(7500));
    assert_eq!(medir_datos(&d.join("no_existe"), &[], None), 0.0);
    // carpetas_historial: las de sincronización activas, o la local si no hay
    let sync = vec![d.join("sync"), d.join("sync2")];
    assert_eq!(carpetas_historial(&sync, &d.join("barra")), vec![d.join("sync/historial"), d.join("sync2/historial")]);
    assert_eq!(carpetas_historial(&[], &d.join("barra")), vec![d.join("barra/historial")]);
    // espacio libre: de la carpeta o de la existente más cercana por encima
    let libre = espacio_libre(&d).expect("espacio libre");
    assert!(libre > 0);
    assert!(espacio_libre(&d.join("no/existe/aun")).is_some());
}

#[test]
fn medir_no_sigue_enlaces_de_carpeta() {
    use std::os::windows::fs::symlink_dir;
    let d = carpeta("enlaces");
    fs::create_dir_all(d.join("real")).unwrap();
    fs::write(d.join("real/f"), vec![0u8; 100]).unwrap();
    fs::create_dir_all(d.join("raiz")).unwrap();
    fs::write(d.join("raiz/g"), vec![0u8; 10]).unwrap();
    // sin privilegios de enlaces simbólicos no se puede crear: se da por válida
    if symlink_dir(d.join("real"), d.join("raiz/enlace")).is_ok() {
        assert_eq!(tam_carpeta(&d.join("raiz")), 10);
    }
}

/// Humo de SOLO LECTURA con lo real: `cargo test humo_plugin_real -- --ignored --nocapture`.
/// Llama a `claude plugin list` (solo consulta); NO instala nada.
#[test]
#[ignore]
fn humo_plugin_real() {
    let home = PathBuf::from(super::transcripciones::carpeta_home());
    let t = Instant::now();
    let claude = buscar_claude(&home);
    println!("buscar_claude: {claude:?} ({:?})", t.elapsed());
    let t = Instant::now();
    let plugins = claude.as_deref().and_then(listar_plugins);
    println!("listar_plugins ({:?}):\n{}", t.elapsed(), plugins.as_deref().unwrap_or("(None)"));
    let est = plugins.as_deref().map(estado_plugin);
    println!("estado_plugin: {est:?}");
    let uso = super::limites::leer_uso(&home.join(".claude/statusbar/usage.json"));
    let hay = uso.semanal.is_some() || uso.cinco_horas.is_some();
    let t = Instant::now();
    let (cod, texto) = diagnostico(hay, claude.is_some(), plugins.as_deref(), &mod_seguro(&home));
    println!("diagnostico: {cod:?} - {texto} ({:?}); puede instalar: {}", t.elapsed(), se_puede_instalar(cod, claude.is_some()));
    let t = Instant::now();
    let viva = hay_sesion(&home.join(".claude/statusbar/sessions.d"));
    println!("hay_sesion: {viva} ({:?})", t.elapsed());
    let t = Instant::now();
    let proy = super::transcripciones::raiz_proyectos();
    let barra = home.join(".claude/statusbar");
    let eq = super::equipos::Equipos::real(&barra, &home);
    let hist = carpetas_historial(&eq.activas(), &barra);
    let gb = medir_datos(&proy, &hist, None);
    let dur = t.elapsed();
    let max = leer_gb_max(&barra.join("consumo_ajustes.json"));
    let d = datos_disco(gb, max, espacio_libre(&proy));
    println!("medir_datos: {gb:.3} GB en {dur:?}; carpetas historial: {hist:?}");
    println!("{d:?}");
    println!("carpeta consumo-mod existe: {}", mod_seguro(&home).join("uso-consumo").is_dir());
}
