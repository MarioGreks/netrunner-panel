//! Pruebas del lector de transcripciones y del contexto por sesión, portadas de la función
//! `prueba` de Consumo (detalle.py y consumo.pyw). Todo en carpetas temporales propias: nunca
//! se toca `~/.claude`.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{Local, TimeZone};
use serde_json::json;

use super::contexto::*;
use super::rutas::*;
use super::transcripciones::*;

fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("netrunner_detalle_{nombre}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn cerca(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

fn uso() -> Uso {
    Uso {
        input_tokens: 10.0,
        cache_creation_input_tokens: 100.0,
        cache_read_input_tokens: 1000.0,
        output_tokens: 10.0,
        cache_creation: None,
    }
}

fn uso_json() -> serde_json::Value {
    json!({"input_tokens": 10, "cache_creation_input_tokens": 100, "cache_read_input_tokens": 1000, "output_tokens": 10})
}

/// 5 de noviembre de 2026 a mediodía (hora local), en s.
fn ahora() -> f64 {
    Local.with_ymd_and_hms(2026, 11, 5, 12, 0, 0).unwrap().timestamp() as f64
}

/// Instante `h` horas después de `ahora()`, en formato ISO con zona.
fn ts(h: f64) -> String {
    let t = ahora() + h * 3600.0;
    Local.timestamp_opt(t as i64, 0).unwrap().to_rfc3339()
}

fn linea(id: &str, t: &str, modelo: &str, cwd: &str, tipo: &str) -> String {
    format!(
        "{}\n",
        json!({"type": tipo, "timestamp": t, "cwd": cwd, "sessionId": "s1",
               "message": {"id": id, "model": modelo, "usage": uso_json()}})
    )
}

fn lin2(id: &str, ses: &str, cwd: &str, h: f64) -> String {
    format!(
        "{}\n",
        json!({"type": "assistant", "timestamp": ts(h), "cwd": cwd, "sessionId": ses,
               "message": {"id": id, "model": "claude-sonnet-5-5", "usage": uso_json()}})
    )
}

fn escribir(ruta: &Path, texto: &str) {
    fs::create_dir_all(ruta.parent().unwrap()).unwrap();
    fs::write(ruta, texto).unwrap();
}

fn anadir(ruta: &Path, texto: &str) {
    OpenOptions::new().append(true).open(ruta).unwrap().write_all(texto.as_bytes()).unwrap();
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn pesos_y_proyectos_simples() {
    assert_eq!(peso_mensaje("claude-sonnet-5-5", &uso()), 285.0);
    assert!(cerca(peso_mensaje("claude-opus-5-5", &uso()), 285.0 * 1.7));
    assert!(cerca(peso_mensaje("claude-haiku-4-5", &uso()), 285.0 * 0.33));
    assert_eq!(proyecto_de("C:\\Github\\Consumo-App\\"), "Consumo-App");
    assert_eq!(proyecto_de("/a/b"), "b");
    assert_eq!(proyecto_de(""), SIN_CARPETA);
    assert_eq!(proyecto_de("C:\\x\\scratch-workspaces\\y"), SIN_CARPETA);
    assert_eq!(proyecto_de("C:\\x\\Repo\\.claude\\worktrees\\a"), "Repo");
}

#[test]
fn leer_linea_basico() {
    let r = leer_linea(&linea("m1", &ts(0.0), "claude-sonnet-5-5", "C:\\p\\A", "assistant")).unwrap();
    assert_eq!((r.clave.as_str(), r.cwd.as_str(), r.ses.as_str(), r.modelo), ("m1", "C:\\p\\A", "s1", "sonnet"));
    assert_eq!((r.peso, r.tokens, r.bloques), (285.0, 1120, Bloques(0)));
    assert!((r.t - ahora()).abs() < 1.0);
    // coste a precio de API: entrada, salida, lectura de caché y escritura de caché (5 min a 1,25x y 1 h a 2x)
    assert!(cerca(r.coste, (10.0 * 2.0 + 10.0 * 10.0 + 1000.0 * 0.2 + 100.0 * 2.0 * 1.25) / 1e6));
    assert!(cerca(
        coste_mensaje("?", &uso()),
        (10.0 * 4.0 + 10.0 * 20.0 + 1000.0 * 0.2 + 100.0 * 4.0 * 1.25) / 1e6
    ));
    let u = Uso {
        input_tokens: 10.0,
        cache_creation_input_tokens: 100.0,
        cache_creation: Some(CreacionCache { ephemeral_1h_input_tokens: 40.0 }),
        ..Default::default()
    };
    assert!(cerca(
        coste_mensaje("claude-sonnet-5-5", &u),
        (10.0 * 2.0 + 60.0 * 2.0 * 1.25 + 40.0 * 2.0 * 2.0) / 1e6
    ));
    assert_eq!(coste_mensaje("x", &Uso::default()), 0.0);
    // 1000 tokens de caché a Sonnet 5.5: 1000 * (2 - 0,2) / 1e6
    assert_eq!(r.cache.0, 1000.0);
    assert!(cerca(r.cache.1, 1000.0 * 1.8 / 1e6));
    // bloques: pensar + Edit + texto, y un elemento que no es un objeto
    let mut con = serde_json::from_str::<serde_json::Value>(&linea("m9", &ts(0.0), "claude-sonnet-5-5", "C:\\p\\A", "assistant")).unwrap();
    con["message"]["content"] = json!([{"type": "thinking"}, {"type": "tool_use", "name": "Edit"}, {"type": "text", "text": "x"}, "raro"]);
    let b = leer_linea(&con.to_string()).unwrap().bloques;
    assert_eq!(b, Bloques::de_nombres(&["thinking", "tool:Edit", "text"]));
    assert_eq!(b.tarea(), "codigo");
    // lo que no es del modelo o no se entiende
    assert!(leer_linea(&linea("m1", &ts(0.0), "<synthetic>", "C:\\p", "assistant")).is_none());
    assert!(leer_linea(&linea("m1", &ts(0.0), "claude-sonnet-5-5", "C:\\p", "user")).is_none());
    assert!(leer_linea("{\"type\":\"user\"}\n").is_none());
    assert!(leer_linea("\"usage\" roto").is_none());
    assert!(leer_linea("{\"type\":\"assistant\",\"message\":{\"usage\":{}},\"timestamp\":\"mal\"}").is_none());
}

#[test]
fn tareas_por_bloques() {
    let t = |n: &[&str]| tarea_de(n);
    assert_eq!(t(&["tool:Edit", "text"]), "codigo");
    assert_eq!(t(&["tool:Bash"]), "comandos");
    assert_eq!(t(&["tool:PowerShell", "tool:Read"]), "comandos");
    assert_eq!(t(&["tool:Read"]), "explorar");
    assert_eq!(t(&["tool:Grep", "thinking"]), "explorar");
    assert_eq!(t(&["tool:WebFetch"]), "web");
    assert_eq!(t(&["tool:Agent"]), "herramientas");
    assert_eq!(t(&["text"]), "mensajes");
    assert_eq!(t(&["thinking"]), "pensar");
    assert_eq!(t(&[]), "");
}

#[test]
fn tiempo_activo() {
    assert_eq!(segundos_activos(&[], PAUSA_ACTIVA), Vec::<f64>::new());
    assert_eq!(segundos_activos(&[5.0], PAUSA_ACTIVA), vec![0.0]);
    assert_eq!(
        segundos_activos(&[0.0, 30.0, 630.0, 640.0, 1241.0], PAUSA_ACTIVA),
        vec![0.0, 30.0, 0.0, 10.0, 0.0]
    );
}

#[test]
fn lector_incremental() {
    let td = carpeta("lector");
    let f = td.join("p").join("a.jsonl");
    escribir(
        &f,
        &(linea("m1", &ts(0.0), "claude-sonnet-5-5", "C:\\p\\A", "assistant")
            + &linea("m1", &ts(0.0), "claude-sonnet-5-5", "C:\\p\\A", "assistant")
            + &linea("m2", &ts(-1.0), "claude-opus-5-5", "C:\\p\\A", "assistant")
            + &linea("viejo", &ts(-24.0 * 36.0), "claude-sonnet-5-5", "C:\\p\\A", "assistant")),
    );
    let mut lec = Lector::new(td.clone()).con_es_repo(|_| false); // sin repos: la última carpeta del cwd
    lec.actualizar(Some(ahora()));
    // m1 repetido cuenta una vez y el de hace 36 días (se retienen 35) se descarta
    assert_eq!(lec.num_mensajes(), 2);
    let costes = lec.costes();
    assert!(costes.keys().all(|k| k.1 == "A"));
    let total: f64 = costes.values().sum();
    assert!(cerca(
        total,
        (10.0 * 2.0 + 10.0 * 10.0 + 1000.0 * 0.2 + 100.0 * 2.0 * 1.25) / 1e6 + (10.0 * 4.0 + 10.0 * 20.0 + 1000.0 * 0.2 + 100.0 * 4.0 * 1.25) / 1e6
    ));
    let cache = lec.cache();
    assert_eq!(cache.values().map(|v| v.0).sum::<f64>(), 2000.0);
    // línea a medias: no se lee todavía
    let nueva = linea("m3", &ts(1.0), "claude-sonnet-5-5", "C:\\p\\A", "assistant");
    anadir(&f, &nueva[..30]);
    lec.actualizar(Some(ahora()));
    assert_eq!(lec.num_mensajes(), 2);
    anadir(&f, &nueva[30..]);
    lec.actualizar(Some(ahora()));
    assert_eq!(lec.num_mensajes(), 3);
    let c = lec.cubos();
    assert_eq!(c.len(), 3);
    assert_eq!(c.iter().map(|x| x.peso).sum::<i64>(), 285 * 2 + 484); // 285 x 1,7 = 484,5 -> 484
    assert_eq!(c.iter().map(|x| x.tokens).sum::<u64>(), 3 * 1120);
    assert!(c.windows(2).all(|w| w[0].inicio <= w[1].inicio));
    assert!(c.iter().all(|x| x.inicio % CUBO == 0 && x.tarea.is_empty()));
    // nada nuevo: no se vuelve a leer ni se duplica
    lec.actualizar(Some(ahora()));
    assert_eq!(lec.num_mensajes(), 3);
    // el fichero encoge (se reescribe): otra vez desde el principio, sin duplicar por el id
    escribir(&f, &linea("m1", &ts(0.0), "claude-sonnet-5-5", "C:\\p\\A", "assistant"));
    lec.actualizar(Some(ahora()));
    assert_eq!(lec.num_mensajes(), 3);
    // los mensajes caducan al avanzar la retención
    lec.actualizar(Some(ahora() + 40.0 * 86400.0));
    assert_eq!(lec.num_mensajes(), 0);
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn bloques_de_lineas_del_mismo_id_se_juntan() {
    let td = carpeta("bloques");
    let mut a = serde_json::from_str::<serde_json::Value>(&lin2("x", "S", "C:\\p\\A", 0.0)).unwrap();
    let mut b = a.clone();
    a["message"]["content"] = json!([{"type": "thinking"}]);
    b["message"]["content"] = json!([{"type": "tool_use", "name": "Bash"}]);
    escribir(&td.join("q").join("S.jsonl"), &format!("{a}\n{b}\n"));
    let mut lec = Lector::new(td.clone()).con_es_repo(|_| false);
    lec.actualizar(Some(ahora()));
    let c = lec.cubos();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].tarea, "comandos");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn proyecto_por_repo_inyectado() {
    let repos: HashSet<&str> = ["C:\\x\\Repo", "C:\\x\\Viajes", "C:\\Users\\yo", "C:\\x\\Raiz"].into();
    let er = |c: &str| repos.contains(nt_normpath(c).as_str());
    let pr = |cwd: &str, ses: &str| proyecto_repo(cwd, ses, &er, &mut CacheRepos::new(), "C:\\Users\\yo");
    for c in ["C:\\x\\Repo", "C:\\x\\Repo\\sub", "C:\\x\\Repo\\docs\\", "C:/x/Repo/sub"] {
        assert_eq!(pr(c, ""), "Repo", "{c}");
    }
    // worktree: se mira antes que .git
    assert_eq!(pr("C:\\x\\Repo\\.claude\\worktrees\\agent-1", ""), "Repo");
    assert_eq!(pr("/x/Repo/.claude/worktrees/a/sub", ""), "Repo");
    assert_eq!(pr("C:\\x", ""), "x");
    assert_eq!(pr("C:\\x\\Viajes\\src", ""), "Viajes");
    assert_eq!(pr("", ""), SIN_CARPETA);
    assert_eq!(pr("C:\\x\\scratch-workspaces\\y", ""), SIN_CARPETA);
    // fuera de un repo: el de la sesión
    assert_eq!(pr("C:\\x", "C:\\x\\Repo"), "Repo");
    assert_eq!(pr("C:\\x\\scratch-workspaces\\y", "C:\\x\\Repo\\a"), "Repo");
    assert_eq!(pr("C:\\x\\Viajes", "C:\\x\\Repo"), "Viajes");
    assert_eq!(pr("C:\\x", "C:\\y"), "x");
    // la carpeta de usuario no cuenta como repo
    assert_eq!(pr("C:\\Users\\yo\\cosa", ""), "cosa");
    assert_eq!(pr("C:\\Users\\yo\\.claude", ""), ".claude");
    assert_eq!(proyecto_repo("Z:\\nada\\aqui", "", &|_| false, &mut CacheRepos::new(), ""), "aqui");
    assert_eq!(repo_de("C:\\x", &|_| false, &mut CacheRepos::new(), ""), None);
}

#[test]
fn repo_de_usa_cache() {
    let visto = std::cell::RefCell::new(Vec::<String>::new());
    let mut cache = CacheRepos::new();
    let a = |c: &str| {
        visto.borrow_mut().push(c.to_string());
        c == "C:\\x\\Repo"
    };
    assert_eq!(repo_de("C:\\x\\Repo\\sub", &a, &mut cache, "C:\\u").as_deref(), Some("Repo"));
    let b = |c: &str| {
        visto.borrow_mut().push(c.to_string());
        false
    };
    repo_de("C:\\x\\Repo\\sub", &b, &mut cache, "C:\\u"); // segunda vez: de la caché, no mira el disco
    assert_eq!(*visto.borrow(), vec!["C:\\x\\Repo\\sub".to_string(), "C:\\x\\Repo".to_string()]);
    assert_eq!(cache.len(), 1);
}

#[test]
fn rutas_estilo_ntpath() {
    assert_eq!(nt_normpath("C:/x//Repo/./sub/.."), "C:\\x\\Repo");
    assert_eq!(nt_dirname("C:\\x\\Repo"), "C:\\x");
    assert_eq!(nt_dirname("C:\\x"), "C:\\");
    assert_eq!(nt_dirname("C:\\"), "C:\\");
    assert_eq!(nt_basename("C:\\x\\Repo"), "Repo");
    assert_eq!(nt_basename("C:\\"), "");
    assert_eq!(buscar_cwd(r#"{"a":1,"cwd": "C:\\x\\Repo","b":2}"#).as_deref(), Some("C:\\x\\Repo"));
    assert_eq!(buscar_cwd(r#"{"cwd":"sin cierre"#), None);
    assert_eq!(buscar_cwd("nada"), None);
}

#[test]
fn proyecto_con_disco_real_y_subagentes() {
    let td0 = carpeta("repos");
    let td = td0.join("tr");
    let w = td0.join("w");
    for c in ["Repo\\sub", "Repo\\docs", "Viajes\\src", "nada\\y"] {
        fs::create_dir_all(w.join(c)).unwrap();
    }
    for c in ["Repo", "Viajes"] {
        fs::create_dir_all(w.join(c).join(".git")).unwrap();
    }
    // el worktree tiene un fichero .git; ni existe la carpeta: no hace falta
    let wt = w.join("Repo").join(".claude").join("worktrees").join("agent-1");
    let home = s(&td0);
    let mut cache = CacheRepos::new();
    let mut rd = |p: &Path| repo_de(&s(p), &tiene_git, &mut cache, &home);
    let esperado: Vec<(PathBuf, Option<&str>)> = vec![
        (w.join("Repo"), Some("Repo")),
        (w.join("Repo").join("sub"), Some("Repo")),
        (w.join("Repo").join("docs"), Some("Repo")),
        (w.join("Viajes"), Some("Viajes")),
        (w.join("Viajes").join("src"), Some("Viajes")),
        (w.join("nada"), None),
        (w.join("nada").join("y"), None),
        (wt.clone(), Some("Repo")),
        (w.join("Repo").join("borrada"), Some("Repo")),
    ];
    for (p, r) in &esperado {
        assert_eq!(rd(p).as_deref(), *r, "{}", p.display());
    }

    let (r, rs, rd_, vs, n, ny) = (
        s(&w.join("Repo")),
        s(&w.join("Repo").join("sub")),
        s(&w.join("Repo").join("docs")),
        s(&w.join("Viajes").join("src")),
        s(&w.join("nada")),
        s(&w.join("nada").join("y")),
    );
    let v = s(&w.join("Viajes"));
    let sub = td.join("q").join("S1").join("subagents");
    escribir(&sub.join("agent-a.jsonl"), &lin2("a1", "S1", &rd_, 0.0));
    escribir(&sub.join("agent-b.jsonl"), &lin2("b1", "S1", &s(&wt), 0.0));
    escribir(&td.join("q").join("S2.jsonl"), &(lin2("s2a", "S2", &n, 0.0) + &lin2("s2b", "S2", &vs, 1.0))); // empieza fuera de un repo y trabaja en Viajes
    escribir(&td.join("q").join("S3.jsonl"), &(lin2("s3a", "S3", &r, 0.0) + &lin2("s3b", "S3", &n, 1.0))); // un mensaje cae fuera de todo repo
    escribir(&td.join("q").join("S4.jsonl"), &(lin2("s4a", "S4", &ny, 0.0) + &lin2("s4b", "S4", &n, 1.0))); // ningún repo: la última carpeta
    escribir(&td.join("q").join("S5.jsonl"), &lin2("n1", "", &n, 0.0)); // sin sessionId
    escribir(&td.join("q").join("S6").join("subagents").join("agent-c.jsonl"), &lin2("c1", "S6", &n, 0.0)); // sin principal: su propio cwd

    let mut lec = Lector::new(td.clone()).con_home(&home);
    lec.actualizar(Some(ahora())); // el subagente de S1 se lee ANTES que su principal (aún no existe)
    let por = |lec: &mut Lector, ses: &str| -> HashSet<String> {
        lec.cubos().into_iter().filter(|x| x.sesion == ses).map(|x| x.proyecto).collect()
    };
    let set = |v: &[&str]| -> HashSet<String> { v.iter().map(|x| x.to_string()).collect() };
    assert_eq!(por(&mut lec, "S1"), set(&["Repo"]));
    assert_eq!(por(&mut lec, "S2"), set(&["Viajes", "nada"]));
    assert_eq!(por(&mut lec, "S3"), set(&["Repo"]));
    assert_eq!(por(&mut lec, "S4"), set(&["y", "nada"]));
    assert_eq!(por(&mut lec, "S6"), set(&["nada"]));
    assert_eq!(por(&mut lec, ""), set(&["nada"]));

    escribir(&td.join("q").join("S1.jsonl"), &(lin2("p1", "S1", &r, 0.0) + &lin2("p2", "S1", &rs, 1.0))); // el agente hace cd
    lec.actualizar(Some(ahora()));
    assert_eq!(por(&mut lec, "S1"), set(&["Repo"]));
    assert_eq!(lec.cwd_sesion("S1"), Some(r.as_str()));
    assert_eq!(lec.cwd_sesion("S2"), Some(n.as_str()));
    // costes, cache y cubos comparten claves
    let cubos = lec.cubos();
    let de_cubos: HashSet<(i64, String, String, &str, &str)> =
        cubos.iter().map(|x| (x.inicio, x.proyecto.clone(), x.sesion.clone(), x.modelo, x.tarea)).collect();
    assert_eq!(lec.costes().keys().cloned().collect::<HashSet<_>>(), de_cubos);
    assert_eq!(lec.cache().keys().cloned().collect::<HashSet<_>>(), de_cubos);
    // el primer cwd del principal se fija la primera vez: un cd posterior no lo cambia
    anadir(&td.join("q").join("S1.jsonl"), &lin2("p3", "S1", &v, 2.0));
    lec.actualizar(Some(ahora()));
    assert_eq!(lec.cwd_sesion("S1"), Some(r.as_str()));
    assert_eq!(por(&mut lec, "S1"), set(&["Repo", "Viajes"])); // p3 está en su propio repo
    let _ = fs::remove_dir_all(&td0);
}

#[test]
fn tiempos_por_sesion() {
    let td = carpeta("tiempos");
    // sesión A: 0, +30 s, +40 min (pausa), +10 s; un subagente de A cuenta en la misma sesión
    let mut l = lin2("a1", "A", "C:\\p\\X", 0.0);
    for (id, seg) in [("a2", 30.0), ("a3", 2430.0), ("a4", 2440.0)] {
        l += &lin2(id, "A", "C:\\p\\X", seg / 3600.0);
    }
    escribir(&td.join("q").join("A.jsonl"), &l);
    escribir(&td.join("q").join("A").join("subagents").join("agent-1.jsonl"), &lin2("b1", "A", "C:\\p\\X", 40.0 / 3600.0));
    let mut lec = Lector::new(td.clone()).con_es_repo(|_| false);
    lec.actualizar(Some(ahora()));
    let total: f64 = lec.tiempos().values().sum();
    // orden por instante: 0, 30, 40, 2430, 2440 -> 0 + 30 + 10 + 0 (pausa) + 10
    assert!((total - 50.0).abs() < 1.0, "{total}");
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn cwd_primero_y_ultimo() {
    let td = carpeta("cwd");
    let f = td.join("a.jsonl");
    escribir(&f, "{\"type\":\"user\",\"cwd\":\"C:\\\\scratch\"}\n{\"type\":\"queue-operation\"}\n{\"type\":\"user\",\"cwd\":\"C:\\\\Github\\\\Consumo-App\"}\n");
    assert_eq!(cwd_de(&f, 300_000), "C:\\Github\\Consumo-App");
    assert_eq!(cwd_de(&td.join("no.jsonl"), 300_000), "");
    escribir(&f, "{\"type\":\"queue-operation\"}\n");
    assert_eq!(cwd_de(&f, 300_000), "");
    escribir(&f, "{\"type\":\"queue-operation\"}\n{\"type\":\"user\",\"cwd\":\"C:\\\\x\\\\Repo\"}\n{\"type\":\"user\",\"cwd\":\"C:\\\\x\\\\Repo\\\\sub\"}\n");
    assert_eq!(primer_cwd(&f, 300_000), "C:\\x\\Repo");
    assert_eq!(cwd_de(&f, 300_000), "C:\\x\\Repo\\sub");
    assert_eq!(primer_cwd(&td.join("no.jsonl"), 300_000), "");
    let _ = fs::remove_dir_all(&td);
}

// --- Contexto por sesión ---

fn linea_ctx(entrada: u64, modelo: &str, cwd: &str) -> String {
    format!(
        "{}\n",
        json!({"type": "assistant", "cwd": cwd, "message": {"model": modelo,
               "usage": {"input_tokens": entrada, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0, "output_tokens": 7}}})
    )
}

fn fijar_mtime(ruta: &Path, t: SystemTime) {
    let f = OpenOptions::new().write(true).open(ruta).unwrap();
    f.set_modified(t).unwrap();
}

#[test]
fn contexto_de_la_cola() {
    let td = carpeta("ctx_de");
    let f = td.join("a.jsonl");
    escribir(
        &f,
        &(linea_ctx(100, "claude-sonnet-5-5", "C:\\p\\A")
            + &linea_ctx(300, "claude-sonnet-5-5", "C:\\p\\A")
            + &linea_ctx(999, "<synthetic>", "C:\\p\\A")
            + "{\"type\":\"user\",\"message\":{\"content\":\"hola\"}}\n"),
    );
    assert_eq!(contexto_de(&f, 400_000), 300); // el sintético no cuenta; la salida tampoco
    assert_eq!(contexto_de(&td.join("no.jsonl"), 400_000), 0);
    escribir(&f, "{\"type\":\"user\"}\n");
    assert_eq!(contexto_de(&f, 400_000), 0);
    // cola corta: la primera línea del trozo viene cortada y se salta
    let l = linea_ctx(55, "claude-opus-5-5", "C:\\p\\A");
    escribir(&f, &(linea_ctx(11, "claude-opus-5-5", "C:\\p\\A") + &l));
    assert_eq!(contexto_de(&f, l.len() as u64 + 5), 55);
    assert_eq!(contexto_de(&f, 10), 0); // trozo sin ninguna línea entera
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn casillas_estables() {
    let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let c = vec![None; 3];
    let c = asignar_casillas(&c, &v(&["a", "b"]));
    assert_eq!(c, vec![Some("a".into()), Some("b".into()), None]);
    // 'a' muere: 'b' conserva su casilla y la nueva coge la primera libre
    let c = asignar_casillas(&c, &v(&["b", "c"]));
    assert_eq!(c, vec![Some("c".into()), Some("b".into()), None]);
    // sin casillas libres, la sesión sobrante se queda fuera
    let c = asignar_casillas(&[None], &v(&["a", "b"]));
    assert_eq!(c, vec![Some("a".into())]);
}

#[test]
fn niveles_y_nombres() {
    assert_eq!(nivel_ctx(0.0), NivelContexto::Bien);
    assert_eq!(nivel_ctx(0.49), NivelContexto::Bien);
    assert_eq!(nivel_ctx(0.5), NivelContexto::Medio);
    assert_eq!(nivel_ctx(0.79), NivelContexto::Medio);
    assert_eq!(nivel_ctx(0.8), NivelContexto::Alto);
    assert_eq!(nivel_ctx(7.0), NivelContexto::Alto);
    assert_eq!(nombre_ctx(1_000_000), "1 M");
    assert_eq!(nombre_ctx(500_000), "500 k");
    assert_eq!(nombre_ctx(200_000), "200 k");
}

fn guardar_chat(dir: &Path, fichero: &str, cuerpo: serde_json::Value) {
    escribir(&dir.join(fichero), &cuerpo.to_string());
}

#[test]
fn chats_de_escritorio() {
    let td = carpeta("chats");
    let base = td.join("Claude-Data");
    let otra = td.join("Claude");
    let dir = base.join("claude-code-sessions").join("a").join("b");
    guardar_chat(&dir, "local_uno.json", json!({"cliSessionId": "s1", "title": "Uno"}));
    guardar_chat(
        &otra.join("perfil").join("claude-code-sessions").join("a").join("b"),
        "local_dos.json",
        json!({"cliSessionId": "s2", "isArchived": true}),
    );
    guardar_chat(&dir, "local_roto.json", json!("no es un objeto"));
    escribir(&dir.join("local_malo.json"), "{ no json");
    escribir(&dir.join("otro.json"), "{\"cliSessionId\":\"s9\"}");
    let mut chats = Chats::new(vec![base, otra], Duration::ZERO);
    chats.refrescar();
    let c1 = chats.de("s1").unwrap();
    assert_eq!((c1.id.as_str(), c1.archivado, c1.titulo.as_str()), ("local_uno", false, "Uno"));
    let c2 = chats.de("s2").unwrap();
    assert_eq!((c2.id.as_str(), c2.archivado, c2.titulo.as_str()), ("local_dos", true, ""));
    assert!(chats.de("s9").is_none() && chats.de("nada").is_none());
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn sesiones_vivas_por_actividad() {
    let td = carpeta("vivas");
    let ahora_t = SystemTime::now();
    let a = td.join("p").join("a.jsonl");
    let b = td.join("p").join("b.jsonl");
    let vieja = td.join("q").join("vieja.jsonl");
    for f in [&a, &b, &vieja] {
        escribir(f, "{}\n");
    }
    escribir(&td.join("q").join("nota.txt"), "x");
    fijar_mtime(&a, ahora_t - Duration::from_secs(100));
    fijar_mtime(&b, ahora_t - Duration::from_secs(10));
    fijar_mtime(&vieja, ahora_t - Duration::from_secs(3600));
    let t = ahora_t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
    let v = sesiones_vivas(t, &td, CTX_VIVA);
    assert_eq!(v.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), vec!["b", "a"]); // la más reciente primero
    assert!(sesiones_vivas(t, &td.join("no_existe"), CTX_VIVA).is_empty());
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn calcular_contexto_de_sesiones() {
    let td = carpeta("ctx_calc");
    let raiz = td.join("proyectos");
    let base = td.join("Claude-Data");
    let ahora_t = SystemTime::now();
    let t = ahora_t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
    let ses = |id: &str| raiz.join("p").join(format!("{id}.jsonl"));
    escribir(&ses("s1"), &linea_ctx(300_000, "claude-opus-5-5", "C:\\w\\Uno"));
    escribir(&ses("s2"), &linea_ctx(900_000, "claude-opus-5-5", "C:\\w\\Dos"));
    escribir(&ses("s3"), &linea_ctx(1_000, "claude-opus-5-5", "C:\\w\\Tres")); // archivada
    escribir(&ses("s4"), &linea_ctx(1_000, "claude-opus-5-5", "C:\\w\\Cuatro")); // inactiva
    fijar_mtime(&ses("s1"), ahora_t - Duration::from_secs(5));
    fijar_mtime(&ses("s2"), ahora_t - Duration::from_secs(1));
    fijar_mtime(&ses("s3"), ahora_t - Duration::from_secs(3));
    fijar_mtime(&ses("s4"), ahora_t - Duration::from_secs(7200));
    let dir_chat = base.join("claude-code-sessions").join("a").join("b");
    guardar_chat(&dir_chat, "local_c1.json", json!({"cliSessionId": "s1", "title": "Mi chat"}));
    guardar_chat(&dir_chat, "local_c3.json", json!({"cliSessionId": "s3", "isArchived": true}));
    let mut ctx = Contexto::new(raiz.clone())
        .con_es_repo(|_| false)
        .con_home("C:\\nada")
        .con_chats(Chats::new(vec![base], Duration::ZERO));
    let r = ctx.calcular(t, CTX_MAX);
    let ids: Vec<&str> = r.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids, vec!["s2", "s1"]); // s3 archivada y s4 inactiva fuera; casillas por orden de más reciente
    let (s2, s1) = (&r[0], &r[1]);
    assert_eq!((s1.tokens, s1.nivel, s1.tiene_chat), (300_000, NivelContexto::Bien, true));
    assert!(cerca(s1.fraccion, 0.3));
    assert_eq!((s1.proyecto.as_str(), s1.nombre.as_str()), ("Uno", "Mi chat"));
    assert_eq!((s2.tokens, s2.nivel, s2.tiene_chat), (900_000, NivelContexto::Alto, false));
    assert_eq!((s2.proyecto.as_str(), s2.nombre.as_str()), ("Dos", "Dos"));
    assert_eq!(ctx.id_chat("s1").as_deref(), Some("local_c1"));
    assert_eq!(ctx.id_chat("s2"), None);
    // con otra ventana máxima cambia la fracción
    let r = ctx.calcular(t, 500_000);
    assert_eq!((r[1].nivel, r[0].fraccion), (NivelContexto::Medio, 1.0));

    // cachea por (tamaño, fecha): mismo tamaño y misma fecha -> no se relee (el valor viejo prueba que no se leyó)
    let mt = fs::metadata(ses("s1")).unwrap().modified().unwrap();
    escribir(&ses("s1"), &linea_ctx(400_000, "claude-opus-5-5", "C:\\w\\Uno"));
    fijar_mtime(&ses("s1"), mt);
    let r = ctx.calcular(t, CTX_MAX);
    assert_eq!(r[1].tokens, 300_000);
    // al cambiar el tamaño sí se relee
    anadir(&ses("s1"), &linea_ctx(450_000, "claude-opus-5-5", "C:\\w\\Uno"));
    fijar_mtime(&ses("s1"), ahora_t - Duration::from_secs(5));
    let r = ctx.calcular(t, CTX_MAX);
    assert_eq!(r[1].tokens, 450_000);

    // casillas estables: s1 muere, entra s5; s2 sigue en su casilla
    fijar_mtime(&ses("s1"), ahora_t - Duration::from_secs(7200));
    escribir(&ses("s5"), &linea_ctx(10, "claude-opus-5-5", "C:\\w\\Cinco"));
    let r = ctx.calcular(t, CTX_MAX);
    assert_eq!(r.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["s2", "s5"]);
    let _ = fs::remove_dir_all(&td);
}

/// Prueba de humo de SOLO LECTURA sobre las transcripciones reales (`~/.claude/projects`).
#[test]
#[ignore]
fn humo_transcripciones_reales() {
    let t0 = std::time::Instant::now();
    let mut lec = Lector::new(raiz_proyectos());
    let ficheros = lec.contar_ficheros();
    lec.actualizar(None);
    let t_lector = t0.elapsed();
    let cubos = lec.cubos();
    let t_cubos = t0.elapsed();
    println!("ficheros: {ficheros}");
    println!("mensajes retenidos: {}", lec.num_mensajes());
    println!("cubos: {}", cubos.len());
    println!("lectura completa: {t_lector:?} (con cubos: {t_cubos:?})");
    let t1 = std::time::Instant::now();
    lec.actualizar(None);
    println!("segunda pasada (nada nuevo): {:?}", t1.elapsed());
    let t2 = std::time::Instant::now();
    let mut ctx = Contexto::real();
    let ahora_s = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
    let sesiones = ctx.calcular(ahora_s, CTX_MAX);
    println!("contexto: {:?}", t2.elapsed());
    for s in &sesiones {
        println!(
            "  sesion {} proyecto={} tokens={} fraccion={:.3} nivel={:?} chat={} nombre={}",
            s.id, s.proyecto, s.tokens, s.fraccion, s.nivel, s.tiene_chat, s.nombre
        );
    }
    let t3 = std::time::Instant::now();
    ctx.calcular(ahora_s, CTX_MAX);
    println!("contexto, segunda vez (con cachés): {:?}", t3.elapsed());
}
