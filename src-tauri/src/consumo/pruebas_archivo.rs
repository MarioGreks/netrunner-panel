//! Pruebas del archivo semanal, la agregación y el histórico, portadas de la función `prueba` de
//! `informe.py` y de `archivar` de `consumo.pyw`. Todo en carpetas temporales propias: nunca se
//! toca `~/.claude`, Documentos ni OneDrive.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate, TimeZone, Timelike};
use serde_json::{json, Value};

use super::agregado::*;
use super::archivar::*;
use super::archivo::*;
use super::historico::*;
use super::historico_vista::*;
use super::limites::{Muestra, SEMANA};
use super::reparto::{CuboEq, Fila, OTROS};
use super::transcripciones::{Clave, Cubo};

fn local(a: i32, m: u32, d: u32, h: u32, mi: u32) -> f64 {
    Local.with_ymd_and_hms(a, m, d, h, mi, 0).unwrap().timestamp() as f64
}

fn fecha(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("netrunner_archivo_{nombre}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn cerca(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn ok(a: &[Option<f64>], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.is_some_and(|x| cerca(x, *y)))
}

fn suma_pct(a: &Archivo) -> f64 {
    a.dias.values().flat_map(|d| d.f.iter()).map(|f| f.pct).sum()
}

fn suma_o(a: &Archivo) -> f64 {
    a.dias.values().flat_map(|d| d.o.iter()).sum()
}

fn cubo(inicio: f64, proy: &str, ses: &str, modelo: &'static str, peso: i64, tok: u64, tarea: &'static str) -> Cubo {
    Cubo { inicio: inicio as i64, proyecto: proy.into(), sesion: ses.into(), modelo, peso, tokens: tok, tarea }
}

fn clave(c: &Cubo) -> Clave {
    (c.inicio, c.proyecto.clone(), c.sesion.clone(), c.modelo, c.tarea)
}

fn fila<'a>(t: f64, proy: &'a str, ses: &'a str, modelo: &'a str, pct: f64, equipo: &'a str, tarea: &'a str) -> Fila<'a> {
    Fila { t, proyecto: proy, sesion: ses, modelo, pct, equipo, tarea }
}

/// Una fila de archivo (sin `seg`: fila antigua de 9 campos = 0 s).
fn fl(proy: &str, eq: &str, p: f64, tok: f64, usd: f64, seg: Option<f64>) -> FilaArch {
    FilaArch {
        proyecto: proy.into(),
        sesion: "s".into(),
        modelo: "opus".into(),
        tarea: "codigo".into(),
        equipo: eq.into(),
        pct: p,
        peso: 1.0,
        tokens: tok,
        coste: usd,
        seg: seg.unwrap_or(0.0),
    }
}

type DiaDef = (&'static str, Vec<FilaArch>, Vec<(usize, f64)>);

fn dia_vacio(f: Vec<FilaArch>) -> DiaArch {
    DiaArch { h: vec![0.0; 24], t: vec![0.0; 24], o: vec![0.0; 24], hp: BTreeMap::new(), k: None, f }
}

fn mk_h(maq: &str, ini: f64, dias: Vec<DiaDef>, h0: bool) -> Archivo {
    let mut m = BTreeMap::new();
    for (iso_, filas, o) in dias {
        let mut d = dia_vacio(filas);
        for (h, v) in o {
            d.o[h] = v;
        }
        m.insert(iso_.to_string(), d);
    }
    if h0 {
        if let Some(d) = m.values_mut().next() {
            d.h[0] = 0.1; // algo atribuido antes de lo sin atribuir: así el primer 'o' de la historia no es «el primer salto»
        }
    }
    Archivo { maquina: maq.into(), inicio: ini, reset: ini + SEMANA, pct: Some(10.0), generado: 0, rep: 1, dias: m }
}

fn con_rep(mut a: Archivo, r: i64) -> Archivo {
    a.rep = r;
    a
}

fn archivos_h() -> (Archivo, Archivo, Archivo, f64, f64) {
    let w1 = local(2026, 10, 24, 16, 0);
    let w2 = w1 + SEMANA;
    let ha1 = mk_h(
        "PCA",
        w1,
        vec![("2026-10-30", vec![fl("web", "PCA", 6.0, 1000.0, 1.0, None)], vec![(3, 2.0)]), ("2026-10-31", vec![fl("web", "PCA", 2.0, 500.0, 0.5, None)], vec![])],
        true,
    );
    let ha2 = mk_h(
        "PCA",
        w2 + 60.0,
        vec![
            ("2026-10-31", vec![fl("web", "PCA", 3.0, 300.0, 0.3, None), fl("app", "PCA", 1.0, 100.0, 0.1, None)], vec![(5, 0.5)]),
            ("2026-11-02", vec![fl("app", "PCA", 5.0, 2000.0, 2.0, None)], vec![]),
        ],
        true,
    );
    let hb1 = mk_h("PCB", w1, vec![("2026-10-30", vec![], vec![(3, 1.0)]), ("2026-10-31", vec![fl("app", "PCB", 4.0, 700.0, 0.7, None)], vec![(5, 1.5)])], true);
    (ha1, ha2, hb1, w1, w2)
}

// ---------- periodos y formatos ----------

#[test]
fn periodos() {
    let r = local(2026, 11, 7, 16, 0);
    let rf = local(2026, 11, 6, 12, 0);
    let p = |t: &str, a: f64, reset: Option<f64>, ant: bool| periodo(t, a, reset, ant).unwrap();
    assert_eq!(p("semana", rf, Some(r), false).0, fecha(2026, 10, 31));
    assert_eq!(p("semana", rf, Some(r), false).1, fecha(2026, 11, 8));
    assert_eq!((p("semana", rf, Some(r), true).0, p("semana", rf, Some(r), true).1), (fecha(2026, 10, 24), fecha(2026, 11, 1)));
    assert_eq!((p("semana", rf, None, false).0, p("semana", rf, None, false).1), (fecha(2026, 10, 31), fecha(2026, 11, 7)));
    assert_eq!(p("semana", rf, Some(r), false).2, "Semana del 31 de octubre al 7 de noviembre de 2026");
    assert_eq!(p("mes", rf, None, false), (fecha(2026, 11, 1), fecha(2026, 12, 1), "Noviembre 2026".into()));
    assert_eq!(p("mes", rf, None, true).0, fecha(2026, 10, 1));
    let (dic, ene) = (local(2026, 12, 15, 0, 0), local(2027, 1, 20, 0, 0));
    assert_eq!(p("mes", dic, None, false).1, fecha(2027, 1, 1));
    assert_eq!((p("mes", ene, None, true).0, p("mes", ene, None, true).1), (fecha(2026, 12, 1), fecha(2027, 1, 1)));
    assert_eq!(p("trimestre", rf, None, false), (fecha(2026, 10, 1), fecha(2027, 1, 1), "4.º trimestre 2026".into()));
    assert_eq!(p("trimestre", rf, None, true).0, fecha(2026, 7, 1));
    assert_eq!(p("trimestre", ene, None, true), (fecha(2026, 10, 1), fecha(2027, 1, 1), "4.º trimestre 2026".into()));
    assert_eq!(p("trimestre", ene, None, false).1, fecha(2027, 4, 1));
    assert_eq!(p("semestre", rf, None, false), (fecha(2026, 7, 1), fecha(2027, 1, 1), "2.º semestre 2026".into()));
    assert_eq!(p("semestre", rf, None, true).1, fecha(2026, 7, 1));
    assert_eq!(p("semestre", ene, None, true).0, fecha(2026, 7, 1));
    assert_eq!(p("semestre", ene, None, false).2, "1.er semestre 2027");
    assert_eq!(p("anio", rf, None, false), (fecha(2026, 1, 1), fecha(2027, 1, 1), "Año 2026".into()));
    assert_eq!(p("anio", rf, None, true).2, "Año 2025");
    assert_eq!(nombre_informe("semana", fecha(2026, 10, 31)), "Consumo - resumen semanal 2026-10-31.html");
    assert!(nombre_informe("anio", fecha(2026, 1, 1)).ends_with("anual 2026-01-01.html"));
    assert!(periodo("siglo", rf, None, false).is_err());
}

#[test]
fn textos_a_la_espanola() {
    assert_eq!(num(1234.56, 1), "1.234,6");
    assert_eq!(num(0.5, 2), "0,50");
    assert_eq!(num(-1234.5, 1), "-1.234,5");
    assert_eq!(num(1_000_000.0, 0), "1.000.000");
    assert_eq!(tokens_txt(2_500_000.0), "2,5 M");
    assert_eq!(tokens_txt(1500.0), "2 k");
    assert_eq!(tokens_txt(3e9), "3,00 G");
    assert_eq!(tokens_txt(12.0), "12");
    assert_eq!(texto_u("eur", Some(1.0385)), "1,04 €");
    assert_eq!(texto_u("horas", Some(2.04)), "2,0 h");
    assert_eq!(texto_u("eur_h", Some(1.234)), "1,23 €/h");
    assert_eq!(texto_u("pct_h", Some(2.5)), "2,50 %/h");
    assert_eq!(texto_u("tok_h", Some(3000.0)), "3 k/h");
    assert_eq!(texto_u("eur_h", None), "—");
}

// ---------- el archivo semanal ----------

struct Semana {
    r: f64,
    ini: f64,
    ahora: f64,
    t: [f64; 3],
    cubos_a: Vec<Cubo>,
    cubos_b: Vec<Cubo>,
}

fn semana() -> Semana {
    let r = local(2026, 11, 7, 16, 0);
    let ini = r - SEMANA;
    let t = [ini + 3600.0 * 20.0, ini + 86400.0 + 3600.0 * 5.0, ini + 86400.0 * 2.0 + 3600.0 * 30.0];
    Semana {
        r,
        ini,
        ahora: local(2026, 11, 6, 12, 0),
        t,
        cubos_a: vec![cubo(t[0], "web", "sesion-aaaa-1111", "opus", 100, 5000, "comandos"), cubo(t[1], "web", "sesion-aaaa-1111", "opus", 50, 2000, "codigo")],
        cubos_b: vec![cubo(t[2], "app", "sesion-bbbb-2222", "sonnet", 80, 3000, "explorar")],
    }
}

fn filas_s(s: &Semana) -> Vec<Fila<'static>> {
    vec![
        fila(s.t[0] + 300.0, "web", "sesion-aaaa-1111", "opus", 6.0, "PCA", "comandos"),
        fila(s.t[1] + 300.0, "web", "sesion-aaaa-1111", "opus", 3.0, "PCA", "codigo"),
        fila(s.t[2] + 300.0, "app", "sesion-bbbb-2222", "sonnet", 4.0, "PCB", "explorar"),
        fila(s.ini + 86400.0 * 3.0, OTROS, OTROS, OTROS, 2.0, OTROS, OTROS),
    ]
}

fn costes_s(s: &Semana) -> HashMap<Clave, f64> {
    HashMap::from([(clave(&s.cubos_a[0]), 1.5), (clave(&s.cubos_a[1]), 0.5)])
}

fn construir(s: &Semana, maq: &str, cubos: &[Cubo]) -> Archivo {
    construir_archivo(maq, s.r, &filas_s(s), cubos, &costes_s(s), Some(15.0), Some(s.ahora), None, None)
}

#[test]
fn construir_y_nombre() {
    let s = semana();
    let a = construir(&s, "PCA", &s.cubos_a);
    let b = construir(&s, "PCB", &s.cubos_b);
    assert_eq!((a.maquina.as_str(), a.inicio, a.rep, a.pct), ("PCA", s.ini, REP, Some(15.0)));
    assert!(cerca(suma_pct(&a), 9.0) && cerca(suma_pct(&b), 4.0));
    assert!(cerca(suma_o(&a), 2.0) && cerca(suma_o(&b), 2.0));
    let da = iso(fecha_de(s.t[0]));
    let f = a.dias[&da].f.iter().find(|f| f.tarea == "comandos").unwrap();
    assert_eq!((f.proyecto.as_str(), f.sesion.as_str(), f.modelo.as_str(), f.equipo.as_str()), ("web", "sesion-a", "opus", "PCA"));
    assert!(f.pct == 6.0 && f.coste == 1.5 && f.tokens == 5000.0 && f.peso == 100.0 && f.seg == 0.0);
    let hora = Local.timestamp_opt(s.t[0] as i64, 0).unwrap().hour() as usize;
    assert_eq!(a.dias[&da].h[hora], 6.0);
    assert!(construir_archivo("PCA", s.r, &[], &[], &HashMap::new(), None, None, None, None).dias.is_empty());
    assert_eq!(nombre_archivo(&a), "PCA.semana-2026-10-31.json");
    // % por hora y proyecto, y los tiempos activos (10.º campo)
    assert_eq!(a.dias[&da].hp["web"][hora], 6.0);
    let tiempos = HashMap::from([(clave(&s.cubos_a[0]), 599.6)]);
    let ta = construir_archivo("PCA", s.r, &filas_s(&s), &s.cubos_a, &costes_s(&s), None, None, None, Some(&tiempos));
    let segs: Vec<f64> = ta.dias.values().flat_map(|d| d.f.iter()).filter(|f| f.tarea == "comandos").map(|f| f.seg).collect();
    assert_eq!(segs, vec![600.0]);
    assert_eq!(a.dias.values().flat_map(|d| d.f.iter()).map(|f| f.seg).sum::<f64>(), 0.0);
    // caché de contexto: el día lleva `k`; sin caché, no
    let ck = HashMap::from([(clave(&s.cubos_a[0]), (4000.0, 0.5)), (clave(&s.cubos_a[1]), (1000.0, 0.25))]);
    let ac = construir_archivo("PCA", s.r, &filas_s(&s), &s.cubos_a, &costes_s(&s), Some(15.0), None, Some(&ck), None);
    assert_eq!(ac.dias.values().map(|d| d.k.unwrap()[0]).sum::<f64>(), 5000.0);
    assert!(cerca(ac.dias.values().map(|d| d.k.unwrap()[1]).sum::<f64>(), 0.75));
    assert!(a.dias[&da].k.is_none());
}

#[test]
fn formato_json_identico_al_de_consumo() {
    let s = semana();
    let a = construir(&s, "PCA", &s.cubos_a);
    let v = a_json(&a);
    assert_eq!(v["v"], 1);
    assert_eq!(v["maquina"], "PCA");
    assert!(v["inicio"].is_i64() && v["reset"].is_i64() && v["rep"] == 2 && v["pct"] == 15.0);
    let da = iso(fecha_de(s.t[0]));
    let d = &v["dias"][&da];
    assert!(d["h"][0].is_f64() && d["t"][0].is_i64() && d["o"][0].is_f64() && d["hp"]["web"].is_array() && d.get("k").is_none());
    let f = d["f"][0].as_array().unwrap();
    assert_eq!(f.len(), 10);
    assert!(f[5].is_f64() && f[6].is_i64() && f[7].is_i64() && f[8].is_f64() && f[9].is_i64());
    // ida y vuelta
    let mut b = limpiar_archivo(&v).unwrap();
    assert_eq!(b.dias, a.dias);
    b.generado = a.generado;
    assert_eq!(b, a);
}

#[test]
fn guardar_sin_cambios_no_reescribe_y_proteger() {
    let s = semana();
    let td = carpeta("guardar");
    let a = construir(&s, "PCA", &s.cubos_a);
    let b = construir(&s, "PCB", &s.cubos_b);
    assert!(guardar_archivo(&td, &a, false).unwrap());
    let mut a_gen = a.clone();
    a_gen.generado = 1;
    assert!(!guardar_archivo(&td, &a_gen, false).unwrap()); // sin cambios (la hora de generación no cuenta) no se reescribe
    assert!(guardar_archivo(&td, &b, false).unwrap());
    let da = iso(fecha_de(s.t[0]));
    let mut a2 = a.clone();
    a2.dias.get_mut(&da).unwrap().f[0].pct = 7.0;
    assert!(guardar_archivo(&td, &a2, false).unwrap());
    let nombres: Vec<String> = fs::read_dir(&td).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(nombres.iter().filter(|n| *n == "PCA.semana-2026-10-31.json").count(), 1);
    assert!(!nombres.iter().any(|n| n.ends_with(".tmp")));
    let mut corto = a2.clone();
    corto.dias.remove(&da); // con menos días que el guardado: con proteger no lo pisa, sin proteger sí
    let mut cache = CacheArchivos::default();
    assert!(!guardar_archivo(&td, &corto, true).unwrap());
    assert!(cache.leer_archivos(&td).iter().find(|x| x.maquina == "PCA").unwrap().dias.contains_key(&da));
    assert!(guardar_archivo(&td, &corto, false).unwrap());
    assert!(guardar_archivo(&td, &a2, true).unwrap()); // uno que lleva más sí sustituye
    assert!(cache.leer_archivos(&td).iter().find(|x| x.maquina == "PCA").unwrap().dias.contains_key(&da));
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn ficheros_rotos_y_formas_raras() {
    let td = carpeta("rotos");
    for (nombre, cont) in [
        ("X.semana-2026-01-01.json", "{roto".to_string()),
        ("Y.semana-2026-01-01.json", "[1]".to_string()),
        ("Z.semana-2026-01-01.json", r#"{"dias": 3, "inicio": 1}"#.to_string()),
        (
            "W.semana-2026-01-01.json",
            json!({"inicio": 5, "dias": {"no-fecha": {}, "2026-13-45": {}, "2026-01-02": {"f": [[1], ["a","a","a","a","a","a","a","a","a"], ["a","a","a","a","a",1.0,2.0,3,4]], "h": "x"}}}).to_string(),
        ),
        ("bom.semana-2026-01-01.json", format!("\u{feff}{}", json!({"inicio": 5, "maquina": "BOM", "dias": {}}))),
        ("otro.json", "{}".into()),
    ] {
        fs::write(td.join(nombre), cont).unwrap();
    }
    let mut cache = CacheArchivos::default();
    let archivos = cache.leer_archivos(&td);
    let mut maq: Vec<&str> = archivos.iter().map(|a| a.maquina.as_str()).collect();
    maq.sort();
    assert_eq!(maq, vec!["?", "BOM"]);
    let w = archivos.iter().find(|a| a.maquina == "?").unwrap();
    assert_eq!(w.dias.keys().collect::<Vec<_>>(), vec!["2026-01-02"]);
    let d = &w.dias["2026-01-02"];
    assert_eq!(d.f.len(), 1);
    assert_eq!(d.h, vec![0.0; 24]);
    assert_eq!(d.f[0].seg, 0.0); // fila de 9 campos: sin tiempo
    assert!(cache.leer_archivos(&td.join("no existe")).is_empty());
    // fila de 10 campos
    let a = limpiar_archivo(&json!({"inicio": 5, "dias": {"2026-01-02": {"f": [["a","a","a","a","a",1.0,2.0,3,4,90], ["a","a","a","a","a",1.0,2.0,3,4], ["a","a","a","a","a",1.0,1.0,1.0,1.0,1.0,1.0]]}}})).unwrap();
    assert_eq!(a.dias["2026-01-02"].f.iter().map(|f| f.seg).collect::<Vec<_>>(), vec![90.0, 0.0]);
    // rep: sin él, 0 o texto = 1; 3 = 3
    assert_eq!(limpiar_archivo(&json!({"inicio": 1, "dias": {}, "rep": 3})).unwrap().rep, 3);
    for r in [json!({}), json!({"rep": 0}), json!({"rep": "x"})] {
        let mut v = json!({"inicio": 1, "dias": {}});
        v.as_object_mut().unwrap().extend(r.as_object().unwrap().clone());
        assert_eq!(limpiar_archivo(&v).unwrap().rep, 1);
    }
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn resets_y_firma() {
    let m: Vec<Muestra> = vec![
        (1.0, 100.0, 1.0),
        (1.0, 100.0 - SEMANA, 1.0),
        (1.0, 100.0 - 3.0 * SEMANA + 5.0, 2.0),
        (1.0, 100.0 - 3.0 * SEMANA, 2.0),
        (1.0, 100.0 - 9.0 * SEMANA, 3.0),
    ];
    // sin la actual ni la anterior, un reset agrupado y solo lo que empieza dentro de lo que se retiene
    assert_eq!(resets_archivables(&m, 100.0, 100.0, 5.0 * SEMANA), vec![100.0 - 3.0 * SEMANA + 5.0]);
    let c = CuboEq { inicio: 50.0, proyecto: "p".into(), sesion: "s".into(), modelo: "m".into(), peso: 3.0, tokens: 1.0, tarea: String::new(), equipo: "x".into() };
    let cs = [&c];
    assert_ne!(firma_semana(100.0, &cs, &[(60.0, 100.0, 1.0)]), firma_semana(100.0, &cs, &[(60.0, 100.0, 2.0)]));
    assert_eq!(firma_semana(100.0, &cs, &[(60.0, 100.0, 1.0)]), firma_semana(100.0, &cs, &[(60.0, 100.0, 1.0)]));
}

// ---------- agregación ----------

#[test]
fn agregar_suma_entre_equipos() {
    let s = semana();
    let a = construir(&s, "PCA", &s.cubos_a);
    let b = construir(&s, "PCB", &s.cubos_b);
    let agr = agregar(&[a.clone(), b.clone()], fecha(2026, 10, 31), fecha(2026, 11, 8), None);
    assert!(cerca(agr.total[0], 15.0) && agr.total[1] == 10000.0 && cerca(agr.total[2], 2.0));
    assert_eq!(agr.maquinas, vec!["PCA", "PCB"]);
    assert!(cerca(agr.proyectos["web"][0], 9.0) && cerca(agr.equipos["PCB"][0], 4.0) && cerca(agr.sin_atribuir.values().sum(), 2.0));
    assert_eq!(agr.sesiones.len(), 2);
    assert!(agr.semanas[0].0 == "2026-10-31" && cerca(agr.semanas[0].1, 15.0));
    assert_eq!(agregar(&[a.clone(), b.clone()], fecha(2027, 1, 1), fecha(2027, 2, 1), None).total[0], 0.0);
    assert_eq!(agregar(&[a.clone()], fecha(2026, 10, 31), fecha(2026, 11, 1), None).total[0], 0.0);
    assert!(cerca(agr.horas_proy.values().flat_map(|d| d.values()).flat_map(|v| v.iter()).sum(), 13.0));
    // caché: los archivos sin ese dato no cuentan en el porcentaje
    let ck = HashMap::from([(clave(&s.cubos_a[0]), (4000.0, 0.5)), (clave(&s.cubos_a[1]), (1000.0, 0.25))]);
    let ac = construir_archivo("PCA", s.r, &filas_s(&s), &s.cubos_a, &costes_s(&s), Some(15.0), None, Some(&ck), None);
    let agc = agregar(&[ac, b.clone()], fecha(2026, 10, 31), fecha(2026, 11, 8), None);
    assert!(agc.cache[0] == 5000.0 && agc.cache[2] == 7000.0 && cerca(agc.cache[1], 0.75));
    assert_eq!(agr.cache[2], 0.0);
    // el día del reset pertenece a dos semanas: el semanal solo cuenta el archivo de su semana
    let mut ant = a.clone();
    ant.inicio = s.ini - SEMANA;
    let da = iso(fecha_de(s.t[0]));
    ant.dias = BTreeMap::from([(da, DiaArch { h: vec![1.0; 24], t: vec![0.0; 24], o: vec![5.0; 24], hp: BTreeMap::new(), k: None, f: vec![fl("vieja", "PCA", 7.0, 100.0, 0.1, Some(0.0))] })]);
    let todos = [a, b, ant];
    let sem = agregar(&todos, fecha(2026, 10, 31), fecha(2026, 11, 8), Some(s.ini));
    assert!(cerca(sem.total[0], 15.0) && !sem.proyectos.contains_key("vieja"));
    let mes = agregar(&todos, fecha(2026, 10, 31), fecha(2026, 11, 8), None);
    assert!(mes.total[0] > 22.0 && mes.proyectos.contains_key("vieja"));
}

// ---------- varias carpetas y caché ----------

fn pon(c: &Path, a: &Archivo) {
    fs::create_dir_all(c).unwrap();
    fs::write(c.join(nombre_archivo(a)), a_json(a).to_string()).unwrap();
}

fn mk_dias(maq: &str, n: usize) -> Archivo {
    let mut a = mk_h(maq, 1000.0, vec![], false);
    for d in 0..n {
        a.dias.insert(format!("2026-11-0{}", d + 1), dia_vacio(vec![]));
    }
    a
}

fn lista(c: &Path) -> Vec<(String, usize)> {
    let mut m: Vec<(String, usize)> = CacheArchivos::default().leer_archivos(c).iter().map(|a| (a.maquina.clone(), a.dias.len())).collect();
    m.sort();
    m
}

#[test]
fn varias_carpetas_y_puente() {
    let td = carpeta("varias");
    let (c1, c2) = (td.join("1"), td.join("2"));
    for (c, a) in [(&c1, mk_dias("X", 1)), (&c2, mk_dias("X", 3)), (&c1, mk_dias("Y", 2)), (&c1, mk_dias("YO", 4))] {
        pon(c, &a);
    }
    let mut cache = CacheArchivos::default();
    let carpetas = vec![c1.clone(), c2.clone()];
    let v: Vec<(String, usize)> = cache.leer_archivos_en(&carpetas).iter().map(|a| (a.maquina.clone(), a.dias.len())).collect();
    assert_eq!(v, vec![("X".to_string(), 3), ("Y".to_string(), 2), ("YO".to_string(), 4)]);
    let mut si = || true;
    assert_eq!(puentear_archivos(&mut cache, &carpetas, "YO", &mut si), (2, false)); // X (3 días) a la 1 e Y a la 2; el de YO no se copia
    assert_eq!(puentear_archivos(&mut cache, &carpetas, "YO", &mut si), (0, false));
    assert_eq!(puentear_archivos(&mut cache, &[c1.clone()], "YO", &mut si), (0, false));
    assert_eq!(lista(&c1), vec![("X".to_string(), 3), ("Y".to_string(), 2), ("YO".to_string(), 4)]);
    assert_eq!(lista(&c2).iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), vec!["X", "Y"]);
    // sin permiso (Consumo.exe abierto) no copia nada
    pon(&c1, &mk_dias("Z", 1));
    let mut no = || false;
    assert_eq!(puentear_archivos(&mut cache, &carpetas, "YO", &mut no), (0, true));
    assert_eq!(lista(&c2).len(), 2);
    let _ = fs::remove_dir_all(&td);
}

#[test]
fn la_cache_no_vuelve_a_leer_lo_que_no_cambia() {
    let td = carpeta("cache");
    let (ha1, ha2, hb1, _, _) = archivos_h();
    for a in [&ha1, &ha2, &hb1] {
        pon(&td, a);
    }
    let carpetas = vec![td.clone()];
    let mut cache = CacheArchivos::default();
    assert_eq!(cache.leer_archivos_en(&carpetas).len(), 3);
    assert_eq!(cache.lecturas, 3);
    let firma = cache.firma(&carpetas);
    assert_eq!(cache.leer_archivos_en(&carpetas).len(), 3);
    assert_eq!(cache.lecturas, 3); // nada cambió: ni un fichero parseado de nuevo
    assert_eq!(cache.firma(&carpetas), firma);
    // cambia uno (otro tamaño): solo ese se vuelve a leer
    let mut ha1b = ha1.clone();
    ha1b.dias.get_mut("2026-10-30").unwrap().f.push(fl("nuevo", "PCA", 1.0, 1.0, 0.1, None));
    pon(&td, &ha1b);
    assert_ne!(cache.firma(&carpetas), firma);
    assert_eq!(cache.leer_archivos_en(&carpetas).len(), 3);
    assert_eq!(cache.lecturas, 4);
    // un fichero que desaparece sale de la lista y de la caché
    fs::remove_file(td.join(nombre_archivo(&hb1))).unwrap();
    assert_eq!(cache.leer_archivos_en(&carpetas).len(), 2);
    // el histórico completo sale de la memoria si no cambió nada ni la vista
    let mut l = LecturaHistorico::default();
    let vista = json!({"periodo": "todo", "grupo": "proyecto", "unidad": "eur"});
    let ahora = local(2026, 11, 14, 9, 30);
    let h1 = l.historico(&carpetas, &vista, ahora, None, 18.0);
    let lecturas = l.cache.lecturas;
    let h2 = l.historico(&carpetas, &vista, ahora, None, 18.0);
    assert!(std::sync::Arc::ptr_eq(&h1, &h2) && l.cache.lecturas == lecturas);
    let h3 = l.historico(&carpetas, &json!({"unidad": "usd"}), ahora, None, 18.0);
    assert_eq!(h3.unidad, "usd");
    assert_eq!(l.cache.lecturas, lecturas); // otra vista: se recalcula pero los ficheros no se releen
    let _ = fs::remove_dir_all(&td);
}

// ---------- histórico ----------

#[test]
fn agregar_historico_suma() {
    let (ha1, ha2, hb1, w1, w2) = archivos_h();
    let hh = agregar_historico(&[hb1.clone(), ha2.clone(), ha1.clone()], None, None, None); // el orden no importa
    let (pw, pa) = (&hh.grupos[0]["web"], &hh.grupos[0]["app"]);
    assert!(cerca(pw.pct, 11.0) && pw.tok == 1800.0 && cerca(pw.usd, 1.8) && pw.primero == "2026-10-30" && pw.ultimo == "2026-10-31" && pw.semanas == 2);
    assert!(cerca(pa.pct, 10.0) && pa.tok == 2800.0 && cerca(pa.usd, 2.8) && pa.primero == "2026-10-31" && pa.ultimo == "2026-11-02" && pa.semanas == 2);
    let (ea, eb) = (&hh.grupos[1]["PCA"], &hh.grupos[1]["PCB"]);
    assert!(cerca(ea.pct, 17.0) && ea.tok == 3900.0 && cerca(ea.usd, 3.9) && ea.semanas == 2 && cerca(eb.pct, 4.0) && eb.tok == 700.0 && eb.semanas == 1);
    assert!(cerca(hh.sin_atribuir, 4.0)); // semana 1: máx(2, 1) + máx(0, 1,5) = 3,5; semana 2: 0,5
    assert!(cerca(hh.total[0], 25.0) && hh.total[1] == 4600.0 && cerca(hh.total[2], 4.6) && hh.total[3] == 0.0);
    assert_eq!((hh.desde.as_deref(), hh.hasta.as_deref()), (Some("2026-10-30"), Some("2026-11-02")));
    assert_eq!(hh.maquinas, vec!["PCA", "PCB"]);
    assert_eq!(hh.semanas.len(), 2);
    assert!(hh.semanas[0].0 == "2026-10-24" && hh.semanas[1].0 == "2026-10-31" && cerca(hh.semanas[0].2, 3.5) && cerca(hh.semanas[1].2, 0.5));
    assert!(cerca(hh.semanas[0].1[0]["web"][0], 8.0) && cerca(hh.semanas[1].1[0]["web"][0], 3.0) && cerca(hh.semanas[1].1[1]["PCA"][0], 9.0));
    let (sp, se): (f64, f64) = (hh.grupos[1].values().map(|v| v.pct).sum(), hh.grupos[0].values().map(|v| v.pct).sum());
    assert!(cerca(sp, se));
    // filtro de periodo: solo el día 31
    let todos = [hb1.clone(), ha2.clone(), ha1.clone()];
    let h31 = agregar_historico(&todos, Some(fecha(2026, 10, 31)), Some(fecha(2026, 11, 1)), None);
    assert!(cerca(h31.grupos[0]["web"].pct, 5.0) && cerca(h31.grupos[0]["app"].pct, 5.0) && cerca(h31.sin_atribuir, 2.0) && cerca(h31.total[0], 12.0));
    assert!(h31.desde == h31.hasta && h31.desde.as_deref() == Some("2026-10-31"));
    assert!(agregar_historico(&todos, Some(fecha(2027, 1, 1)), None, None).desde.is_none());
    assert!(cerca(agregar_historico(&todos, None, Some(fecha(2026, 10, 31)), None).total[0], 8.0));
    assert!(cerca(agregar_historico(&todos, Some(fecha(2026, 11, 1)), None, None).total[0], 5.0));
    let hs2 = agregar_historico(&todos, Some(fecha(2000, 1, 1)), Some(fecha(2000, 1, 2)), Some(w2)); // con `semana` manda el archivo, no las fechas
    assert!(cerca(hs2.total[0], 9.5) && hs2.maquinas == vec!["PCA"]);
    assert!(cerca(agregar_historico(&todos, None, None, Some(w1)).total[0], 15.5));
    // tiempo activo: filas de 9 campos = 0 s; las de 10 suman
    let ht = mk_h(
        "PCT",
        w1,
        vec![
            ("2026-10-30", vec![fl("web", "PCT", 1.0, 10.0, 0.1, Some(5400.0)), fl("app", "PCT", 1.0, 10.0, 0.1, Some(1800.0))], vec![]),
            ("2026-10-31", vec![fl("web", "PCT", 1.0, 10.0, 0.1, None)], vec![]),
        ],
        true,
    );
    let h = agregar_historico(&[ht.clone(), ha1.clone()], None, None, None);
    assert!(h.total[3] == 7200.0 && h.grupos[0]["web"].seg == 5400.0 && h.grupos[1]["PCT"].seg == 7200.0 && h.grupos[1]["PCA"].seg == 0.0);
    assert_eq!(h.semanas[0].1[0]["web"][3], 5400.0);
    let sh = series_historico(&[ht, ha1], "proyecto", "horas", None, None, None, PRECIO_EUR, None);
    assert!(cerca(sh.total["horas"].unwrap(), 2.0) && sh.sin_atribuir.is_none());
    assert!(cerca(sh.filas[0].valor, 1.5) && sh.filas[0].nombre == "web" && cerca(sh.series["web"].iter().flatten().sum(), 1.5));
}

#[test]
fn primer_salto_y_metodo_de_reparto() {
    let (_, _, _, w1, w2) = archivos_h();
    let s1 = mk_h("PCA", w1, vec![("2026-10-27", vec![], vec![(17, 78.0)]), ("2026-10-28", vec![], vec![(9, 5.0)])], false);
    let s1b = mk_h("PCB", w1 + 30.0, vec![("2026-10-27", vec![], vec![(17, 78.0)])], false);
    assert_eq!(primer_salto(&[s1.clone()]), Some((w1, "2026-10-27".into(), 17)));
    assert_eq!(primer_salto(&[s1b.clone(), s1.clone()]).map(|x| (x.1, x.2)), Some(("2026-10-27".into(), 17)));
    assert_eq!(primer_salto::<Archivo>(&[]), None);
    let hs = agregar_historico(&[s1.clone(), s1b.clone()], None, None, None);
    assert!(cerca(hs.sin_atribuir, 5.0)); // el 78 % de las dos máquinas se ignora; el del 28, no
    let sd = series_historico(&[s1.clone(), s1b.clone()], "proyecto", "dias", None, None, None, PRECIO_EUR, None);
    assert!(cerca(sd.total["dias"].unwrap(), 0.35));
    let ag = agregar(&[s1.clone(), s1b.clone()], fecha(2026, 10, 24), fecha(2026, 11, 3), None);
    assert!(cerca(ag.sin_atribuir.values().sum(), 5.0));
    assert!(cerca(agregar_historico(&[s1.clone()], None, None, None).sin_atribuir, 5.0));
    let s2 = mk_h("PCA", w1, vec![("2026-10-27", vec![], vec![(17, 78.0)])], true); // algo atribuido antes: no se ignora nada
    assert_eq!(primer_salto(&[s2.clone()]), None);
    assert!(cerca(agregar_historico(&[s2], None, None, None).sin_atribuir, 78.0));
    let s3 = mk_h("PCA", w1, vec![("2026-10-26", vec![], vec![(4, 1.0)]), ("2026-10-27", vec![], vec![(17, 78.0)])], false);
    assert_eq!(primer_salto(&[s3.clone()]), Some((w1, "2026-10-26".into(), 4)));
    assert!(cerca(agregar_historico(&[s3], None, None, None).sin_atribuir, 78.0));
    let s4 = mk_h("PCA", w2, vec![("2026-11-02", vec![], vec![(1, 9.0)])], false); // una semana posterior no es el primer salto
    assert_eq!(primer_salto(&[s1.clone(), s4.clone()]).map(|x| x.1), Some("2026-10-27".into()));
    assert!(cerca(agregar_historico(&[s1.clone(), s4], None, None, None).sin_atribuir, 14.0));
    // método de reparto ('rep'): por semana solo cuenta el 'o' de los archivos del método más nuevo
    let v1 = mk_h("PCA", w1, vec![("2026-10-28", vec![], vec![(9, 8.0)])], true);
    let v2 = con_rep(mk_h("PCB", w1 + 30.0, vec![("2026-10-28", vec![], vec![(9, 0.0)]), ("2026-10-29", vec![fl("web", "PCB", 1.0, 10.0, 0.1, Some(0.0))], vec![])], true), 2);
    assert_eq!(o_validos(&[v1.clone(), v2.clone()]), vec![false, true]);
    assert!(agregar(&[v1.clone(), v2.clone()], fecha(2026, 10, 24), fecha(2026, 11, 3), None).sin_atribuir.values().sum::<f64>().abs() < 1e-9);
    assert!(agregar_historico(&[v1.clone(), v2.clone()], None, None, None).sin_atribuir.abs() < 1e-9);
    assert!(cerca(agregar_historico(&[v1.clone(), v2.clone()], None, None, None).grupos[0]["web"].pct, 1.0)); // lo demás del archivo viejo y del nuevo sigue contando
    let v3 = con_rep(mk_h("PCC", w1, vec![("2026-10-28", vec![], vec![(9, 3.0)])], true), 2);
    assert!(cerca(agregar_historico(&[v1.clone(), v2.clone(), v3.clone()], None, None, None).sin_atribuir, 3.0));
    assert!(cerca(agregar(&[v1.clone(), v2.clone(), v3], fecha(2026, 10, 24), fecha(2026, 11, 3), None).sin_atribuir.values().sum(), 3.0));
    let v4 = mk_h("PCB", w1, vec![("2026-10-28", vec![], vec![(9, 5.0)])], true);
    assert!(cerca(agregar_historico(&[v1.clone(), v4.clone()], None, None, None).sin_atribuir, 8.0));
    assert_eq!(o_validos(&[v1.clone(), v4]).iter().filter(|x| **x).count(), 2); // los dos viejos: el máximo, como antes
    let v5 = con_rep(mk_h("PCC", w2, vec![("2026-11-02", vec![], vec![(1, 4.0)])], true), 2); // otra semana: no depende de la rep de la primera
    assert!(cerca(agregar_historico(&[v1, v2, v5], None, None, None).sin_atribuir, 4.0));
    let p1 = mk_h("PCA", w1, vec![("2026-10-27", vec![], vec![(17, 78.0)])], false); // el salto de un archivo viejo no cuenta si hay uno nuevo
    let p2 = con_rep(mk_h("PCB", w1, vec![("2026-10-28", vec![], vec![(2, 1.0)])], false), 2);
    assert_eq!(primer_salto(&[p1.clone(), p2]).map(|x| (x.1, x.2)), Some(("2026-10-28".into(), 2)));
    assert_eq!(primer_salto(&[p1]).map(|x| (x.1, x.2)), Some(("2026-10-27".into(), 17)));
}

#[test]
fn conversion_de_unidades() {
    assert!((valor_u("eur", 25.0, 0.0, 0.0, 18.0, 0.0).unwrap() - 25.0 / 100.0 * 18.0 * 12.0 / 52.0).abs() < 1e-12);
    assert!((valor_u("eur", 25.0, 0.0, 0.0, 18.0, 0.0).unwrap() - 1.0385).abs() < 1e-4 && (eur_semana(18.0) - 4.1538).abs() < 1e-4);
    assert!((valor_u("eur", 100.0, 0.0, 0.0, PRECIO_EUR, 0.0).unwrap() - eur_semana(PRECIO_EUR)).abs() < 1e-12);
    assert_eq!(valor_u("dias", 25.0, 9.0, 9.0, 18.0, 0.0), Some(1.75));
    assert_eq!(valor_u("usd", 25.0, 9.0, 7.5, 18.0, 0.0), Some(7.5));
    assert_eq!(valor_u("tokens", 25.0, 9.0, 7.5, 18.0, 0.0), Some(9.0));
    assert_eq!(valor_u("horas", 25.0, 9.0, 7.5, 18.0, 7200.0), Some(2.0));
    assert_eq!(valor_u("horas", 25.0, 9.0, 7.5, 18.0, 0.0), Some(0.0));
    for (u, s) in [("eur_h", 0.0), ("pct_h", 0.0), ("tok_h", -3.0)] {
        assert_eq!(valor_u(u, 5.0, 9.0, 1.0, 18.0, s), None); // sin horas no hay cociente
    }
    assert_eq!(valor_u("pct_h", 5.0, 9.0, 1.0, 18.0, 7200.0), Some(2.5));
    assert_eq!(valor_u("tok_h", 5.0, 9.0, 1.0, 18.0, 1800.0), Some(18.0));
    assert!((valor_u("eur_h", 25.0, 0.0, 0.0, 18.0, 3600.0).unwrap() - 25.0 / 100.0 * eur_semana(18.0)).abs() < 1e-12);
    assert_eq!(UNIDADES.iter().rev().take(3).map(|u| u.0).collect::<Vec<_>>(), vec!["tok_h", "pct_h", "eur_h"]);
    assert!(escala_u("tok_h") == 1e-6 && escala_u("tokens") == 1e-6 && escala_u("eur_h") == 1.0);
    assert_eq!((unidad_parte("eur_h"), unidad_parte("pct_h"), unidad_parte("tok_h"), unidad_parte("usd")), ("eur", "dias", "tokens", "usd"));
    assert!(precio_valido(&json!(18)) && precio_valido(&json!(0.5)));
    for x in [json!(0), json!(-3), json!("x"), json!(true), json!(null), json!(9999)] {
        assert!(!precio_valido(&x), "{x}");
    }
}

#[test]
fn periodos_del_historico() {
    let rf = local(2026, 11, 6, 12, 0);
    let r = local(2026, 11, 7, 16, 0);
    let (dic, ene) = (local(2026, 12, 15, 0, 0), local(2027, 1, 20, 0, 0));
    let ph = |c: &str, reset: Option<f64>, d: Option<NaiveDate>, h: Option<NaiveDate>| periodo_historico(c, rf, reset, d, h).unwrap();
    let t = ph("semana", Some(r), None, None);
    assert_eq!((t.0, t.1, t.2), (Some(fecha(2026, 10, 31)), Some(fecha(2026, 11, 8)), Some(r - SEMANA)));
    assert_eq!(t.3, "Semana del 31 de octubre al 7 de noviembre de 2026");
    let t = ph("semana_pasada", Some(r), None, None);
    assert_eq!((t.0, t.1, t.2), (Some(fecha(2026, 10, 24)), Some(fecha(2026, 11, 1)), Some(r - 2.0 * SEMANA)));
    let t = ph("semana", None, None, None);
    assert_eq!((t.0, t.1, t.2), (Some(fecha(2026, 10, 31)), Some(fecha(2026, 11, 7)), None)); // sin reset: 7 días que acaban hoy
    let t = ph("semana_pasada", None, None, None);
    assert_eq!((t.0, t.1), (Some(fecha(2026, 10, 24)), Some(fecha(2026, 10, 31))));
    assert_eq!(ph("mes", None, None, None), (Some(fecha(2026, 11, 1)), Some(fecha(2026, 12, 1)), None, "Noviembre 2026".into()));
    assert_eq!(ph("mes_pasado", None, None, None).0, Some(fecha(2026, 10, 1)));
    assert_eq!(periodo_historico("mes_pasado", ene, None, None, None).unwrap().0, Some(fecha(2026, 12, 1)));
    assert_eq!(periodo_historico("mes", dic, None, None, None).unwrap().1, Some(fecha(2027, 1, 1)));
    let t = ph("3meses", None, None, None);
    assert_eq!((t.0, t.1), (Some(fecha(2026, 8, 9)), Some(fecha(2026, 11, 7))));
    assert!(t.3.contains("Últimos 3 meses") && (fecha(2026, 11, 7) - fecha(2026, 8, 9)).num_days() == 90);
    assert_eq!(ph("anio", None, None, None), (Some(fecha(2026, 1, 1)), Some(fecha(2027, 1, 1)), None, "Año 2026".into()));
    assert_eq!(ph("todo", None, None, None), (None, None, None, "Todo el histórico".into()));
    let t = ph("personalizado", None, Some(fecha(2026, 10, 30)), Some(fecha(2026, 11, 2)));
    assert_eq!((t.0, t.1), (Some(fecha(2026, 10, 30)), Some(fecha(2026, 11, 3))));
    assert_eq!(t.3, "Del 30 de octubre de 2026 al 2 de noviembre de 2026");
    assert_eq!(ph("personalizado", None, Some(fecha(2026, 10, 30)), None).1, None);
    assert_eq!(ph("personalizado", None, None, Some(fecha(2026, 11, 2))).0, None);
    assert_eq!(ph("personalizado", None, None, None).3, "Todo el histórico");
    assert!(periodo_historico("siglo", rf, None, None, None).is_err());
    let d0 = fecha(2026, 10, 1);
    let pasos: Vec<&str> = [1, 31, 32, 182, 183, 800].iter().map(|n| paso_auto(d0, d0 + chrono::Duration::days(n - 1))).collect();
    assert_eq!(pasos, vec!["dia", "dia", "semana", "semana", "mes", "mes"]);
    assert_eq!(acumular(&[1.0, 2.0, 0.5]), vec![1.0, 3.0, 3.5]);
    assert!(acumular(&[]).is_empty());
}

fn ser(a: &[&Archivo], g: &str, u: &str, paso: Option<&str>) -> SeriesH {
    series_historico(a, g, u, None, None, None, PRECIO_EUR, paso)
}

#[test]
fn series_por_tramo() {
    let (ha1, ha2, hb1, _, _) = archivos_h();
    let a = [&hb1, &ha2, &ha1];
    let sd = ser(&a, "proyecto", "dias", None);
    assert_eq!(sd.paso, "dia");
    assert_eq!(sd.tramos.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(), vec!["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"]);
    assert_eq!(sd.tramos[0].1, "vie 30/10");
    assert!(ok(&sd.series["web"], &[0.42, 0.35, 0.0, 0.0]) && ok(&sd.series["app"], &[0.0, 0.35, 0.0, 0.35]));
    assert!(sd.sin_atribuir.as_ref().unwrap().iter().zip([0.14, 0.14, 0.0, 0.0]).all(|(x, y)| cerca(*x, y)));
    let suma: f64 = sd.series.values().flat_map(|v| v.iter().flatten()).sum::<f64>() + sd.sin_atribuir.as_ref().unwrap().iter().sum::<f64>();
    assert!(cerca(suma, sd.total["dias"].unwrap()) && cerca(sd.total["dias"].unwrap(), 1.75));
    assert_eq!(sd.filas.iter().map(|f| f.clave.as_str()).collect::<Vec<_>>(), vec!["web", "app"]);
    let f0 = &sd.filas[0];
    assert!(cerca(f0.valor, 0.77) && f0.semanas == 2 && f0.primero == "2026-10-30");
    assert!(cerca(f0.participacion, 0.77 / 1.75 * 100.0));
    assert!(cerca(f0.valores["eur"].unwrap(), 11.0 / 100.0 * eur_semana(PRECIO_EUR)) && cerca(f0.valores["usd"].unwrap(), 1.8));
    assert!(cerca(sd.total["eur"].unwrap(), 25.0 / 100.0 * eur_semana(PRECIO_EUR)) && cerca(sd.sin["dias"], 0.28));
    assert_eq!((sd.maquinas.clone(), sd.desde.as_deref()), (vec!["PCA".to_string(), "PCB".to_string()], Some("2026-10-30")));
    // otro precio: otra conversión
    let se = series_historico(&a, "proyecto", "eur", None, None, None, 30.0, None);
    let esp: Vec<f64> = [0.42, 0.35, 0.0, 0.0].iter().map(|x| x / 0.07 / 100.0 * 30.0 * 12.0 / 52.0).collect();
    assert!(ok(&se.series["web"], &esp) && cerca(se.total["eur"].unwrap(), 0.25 * 30.0 * 12.0 / 52.0));
    // $ y tokens: sin atribuir no hay
    let (su, st) = (ser(&a, "proyecto", "usd", None), ser(&a, "proyecto", "tokens", None));
    assert!(su.sin_atribuir.is_none() && st.sin_atribuir.is_none());
    assert!(ok(&su.series["app"], &[0.0, 0.8, 0.0, 2.0]) && st.series["app"][3] == Some(2000.0));
    assert!(ser(&a, "proyecto", "eur", None).sin_atribuir.is_some() && su.sin.contains_key("eur"));
    // por semana de Claude y por mes
    let sw = ser(&a, "proyecto", "dias", Some("semana"));
    assert_eq!(sw.paso, "semana");
    assert_eq!(sw.tramos.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(), vec!["2026-10-24", "2026-10-31"]);
    assert!(ok(&sw.series["web"], &[0.56, 0.21]) && ok(&sw.series["app"], &[0.28, 0.42]));
    assert!(sw.sin_atribuir.as_ref().unwrap().iter().zip([0.245, 0.035]).all(|(x, y)| cerca(*x, y)));
    let sm = ser(&a, "proyecto", "dias", Some("mes"));
    assert_eq!(sm.tramos, vec![("2026-10".to_string(), "oct 2026".to_string()), ("2026-11".to_string(), "nov 2026".to_string())]);
    assert!(ok(&sm.series["web"], &[0.77, 0.0]) && ok(&sm.series["app"], &[0.35, 0.35]));
    // otras agrupaciones, con sus nombres legibles
    assert_eq!(ser(&a, "equipo", "dias", None).series.keys().collect::<Vec<_>>(), vec!["PCA", "PCB"]);
    assert_eq!(ser(&a, "modelo", "dias", None).filas.iter().map(|f| f.nombre.as_str()).collect::<Vec<_>>(), vec!["Opus"]);
    assert_eq!(ser(&a, "tarea", "dias", None).filas.iter().map(|f| f.nombre.as_str()).collect::<Vec<_>>(), vec!["Escribir código"]);
    assert!(acumular(&sd.series["web"].iter().flatten().copied().collect::<Vec<_>>()).iter().zip([0.42, 0.77, 0.77, 0.77]).all(|(x, y)| cerca(*x, y)));
    // filtro por fechas y vacíos
    let s31 = series_historico(&a, "proyecto", "dias", Some(fecha(2026, 10, 31)), Some(fecha(2026, 11, 1)), None, PRECIO_EUR, None);
    assert_eq!(s31.tramos, vec![("2026-10-31".to_string(), "sáb 31/10".to_string())]);
    let sv = ser(&[], "proyecto", "eur", None);
    assert!(sv.tramos.is_empty() && sv.series.is_empty() && sv.filas.is_empty() && sv.sin_atribuir == Some(vec![]) && sv.total["eur"] == Some(0.0));
    assert!(ser(&[], "tarea", "usd", None).sin_atribuir.is_none());
    assert!(series_historico(&[&ha1], "proyecto", "eur", Some(fecha(2030, 1, 1)), None, None, PRECIO_EUR, None).tramos.is_empty());
}

#[test]
fn unidades_por_hora_y_horas_activas() {
    let w1 = local(2026, 10, 24, 16, 0);
    let hr = mk_h(
        "PCR",
        w1,
        vec![
            ("2026-10-30", vec![fl("web", "PCR", 6.0, 1000.0, 1.0, Some(3600.0)), fl("app", "PCR", 2.0, 400.0, 0.4, Some(0.0))], vec![]),
            ("2026-10-31", vec![fl("web", "PCR", 4.0, 500.0, 0.5, Some(3600.0))], vec![]),
            ("2026-11-01", vec![fl("app", "PCR", 1.0, 100.0, 0.1, Some(0.0))], vec![]),
        ],
        true,
    );
    let a = [&hr];
    let sr = ser(&a, "proyecto", "pct_h", None);
    assert_eq!(sr.series["web"], vec![Some(6.0), Some(4.0), None]);
    assert_eq!(sr.series["app"], vec![None, None, None]);
    assert_eq!(sr.series_acum["web"], vec![Some(6.0), Some(5.0), Some(5.0)]); // el acumulado es suma / suma, no 6 + 4
    assert_eq!(sr.series_acum["app"], vec![None, None, None]);
    assert_eq!(sr.filas.iter().map(|f| f.clave.as_str()).collect::<Vec<_>>(), vec!["web"]);
    assert!(cerca(sr.filas[0].valor, 5.0) && sr.total["pct_h"] == Some(6.5) && cerca(sr.filas[0].participacion, 10.0 / 13.0 * 100.0));
    assert!(sr.sin_atribuir.is_none() && sr.sin_acum.is_none());
    assert!(sr.filas[0].valores["horas"] == Some(2.0) && sr.filas[0].valores["pct_h"] == Some(5.0));
    let st = ser(&a, "proyecto", "tok_h", None);
    assert_eq!(st.series["web"], vec![Some(1000.0), Some(500.0), None]);
    assert_eq!(st.series_acum["web"], vec![Some(1000.0), Some(750.0), Some(750.0)]);
    assert!(cerca(st.total["tok_h"].unwrap(), 1000.0));
    let se = ser(&a, "proyecto", "eur_h", None);
    assert!(cerca(se.series["web"][0].unwrap(), 6.0 / 100.0 * eur_semana(PRECIO_EUR)));
    assert!(cerca(se.series_acum["web"][1].unwrap(), 10.0 / 100.0 * eur_semana(PRECIO_EUR) / 2.0));
    // la línea de horas
    let sh = ser(&a, "proyecto", "dias", None);
    assert_eq!(sh.horas["web"], vec![1.0, 1.0, 0.0]);
    assert_eq!(horas_tramo(&sh, &["web", "app"], false), vec![1.0, 1.0, 0.0]);
    assert_eq!(horas_tramo(&sh, &["web", "app"], true), vec![1.0, 2.0, 2.0]);
    assert_eq!(horas_tramo(&sh, &["app"], false), vec![0.0; 3]);
    assert_eq!(horas_tramo(&sh, &[], false), vec![0.0; 3]);
    assert_eq!(horas_tramo(&sh, &["no existe"], false), vec![0.0; 3]);
    assert!(horas_tramo(&ser(&[], "proyecto", "eur", None), &["x"], false).is_empty());
    assert_eq!(horas_tramo(&sr, &["web"], false), vec![1.0, 1.0, 0.0]); // también con la unidad en cocientes
    // archivo sin tiempos (9 campos): ni series, ni ranking, ni total
    let (ha1, _, _, _, _) = archivos_h();
    let sx = ser(&[&ha1], "proyecto", "eur_h", None);
    assert!(sx.filas.is_empty() && sx.total["eur_h"].is_none() && sx.series["web"].iter().all(Option::is_none) && !sx.tramos.is_empty());
    assert!(ser(&[], "proyecto", "pct_h", None).filas.is_empty());
    // la serie en unidades de suma no cambia por el reparto en bruto
    let sd = ser(&a, "proyecto", "dias", None);
    let web: Vec<f64> = sd.series["web"].iter().flatten().copied().collect();
    assert!(sd.series_acum["web"].iter().flatten().zip(acumular(&web)).all(|(x, y)| cerca(*x, y)));
}

#[test]
fn vista_saneada() {
    let vd = limpiar_vista_historico(&Value::Null);
    assert_eq!((vd.grupo.as_str(), vd.unidad.as_str(), vd.periodo.as_str(), vd.modo.as_str(), vd.paso.as_str()), ("proyecto", "eur", "todo", "apilado", "auto"));
    assert!(vd.desde.is_empty() && vd.hasta.is_empty() && !vd.acum && vd.sin && vd.horas && vd.sel.is_empty() && vd.geom.is_empty());
    let raro = json!({"grupo": 5, "unidad": "x", "periodo": "siglo", "desde": "2026-13-01", "modo": 3, "acum": "si", "sin": 0, "horas": "si", "sel": [1], "geom": "enorme"});
    assert_eq!(limpiar_vista_historico(&raro), vd);
    let vb = json!({"grupo": "tarea", "unidad": "tokens", "periodo": "personalizado", "desde": "2026-10-01", "hasta": "", "modo": "lineas", "acum": true, "sin": false, "horas": false,
        "sel": {"tarea": ["codigo", "web"], "proyecto": ["a"]}, "geom": "1100x760+20-5", "paso": "mes"});
    let b = limpiar_vista_historico(&vb);
    assert!(b.grupo == "tarea" && b.unidad == "tokens" && b.periodo == "personalizado" && b.desde == "2026-10-01" && b.hasta.is_empty());
    assert!(b.modo == "lineas" && b.acum && !b.sin && !b.horas && b.geom == "1100x760+20-5" && b.paso == "mes");
    assert_eq!(b.sel["tarea"], vec!["codigo", "web"]);
    let mut con_raro = vb.clone();
    con_raro["raro"] = json!(1);
    con_raro["sel"]["otro"] = json!(["x"]);
    con_raro["sel"]["equipo"] = json!([1, "b", null]);
    let c = limpiar_vista_historico(&con_raro);
    assert_eq!(c.sel.keys().collect::<Vec<_>>(), vec!["equipo", "proyecto", "tarea"]);
    assert_eq!(c.sel["equipo"], vec!["b"]);
    assert_eq!(limpiar_vista_historico(&json!({"geom": "1000x700+-8+0"})).geom, "1000x700+-8+0");
    assert_eq!(limpiar_vista_historico(&json!({"geom": "1000x700+--8+0"})).geom, ""); // '+-8': a la izquierda de 0
    let mut sin_desde = vb.clone();
    sin_desde["desde"] = json!("");
    assert_eq!(limpiar_vista_historico(&sin_desde).periodo, "todo");
    assert_eq!(limpiar_vista_historico(&json!({"periodo": "mes_pasado"})).periodo, "mes_pasado");
    assert!(!limpiar_vista_historico(&json!({"horas": false})).horas && limpiar_vista_historico(&json!({"horas": 1})).horas && limpiar_vista_historico(&json!({"horas": null})).horas);
    assert_eq!(limpiar_vista_historico(&json!({"paso": "siglo"})).paso, "auto");
}

#[test]
fn comando_historico_y_semanas() {
    let (ha1, ha2, hb1, _, _) = archivos_h();
    let a = [ha1, ha2, hb1];
    let ahora = local(2026, 11, 14, 9, 30);
    let h = calcular_historico(&a, &json!({"periodo": "todo", "grupo": "proyecto", "unidad": "dias", "modo": "lineas", "paso": "dia"}), ahora, None, 18.0);
    assert!(h.hay_datos && h.paso == "dia" && h.unidad == "dias" && h.escala == 1.0 && h.titulo == "Todo el histórico");
    assert_eq!(h.etiquetas.len(), 4);
    assert_eq!(h.series.iter().map(|s| s.clave.as_str()).collect::<Vec<_>>(), vec!["web", "app"]);
    assert!(h.series[0].texto == "0,8 días" && h.total.iter().find(|t| t.unidad == "dias").unwrap().texto == "1,8 días");
    assert!(h.sin_atribuir.is_some() && h.archivos == 3 && h.semanas == 2 && h.maquinas == vec!["PCA", "PCB"]);
    assert_eq!(h.filas[0].texto_participacion, "44,0 %");
    assert!(h.filas[0].valores.iter().any(|v| v.unidad == "eur" && v.texto == "0,46 €"));
    assert!(cerca(h.eur_semana, 18.0 * 12.0 / 52.0));
    // en JSON, camelCase
    let v = serde_json::to_value(&h).unwrap();
    assert!(v["hayDatos"] == true && v["etiquetas"].is_array() && v["series"][0]["horasAcumuladas"].is_array() && v["vista"]["grupo"] == "proyecto" && v["precioEurMes"] == 18.0);
    // tokens: escala en millones
    let t = calcular_historico(&a, &json!({"unidad": "tokens"}), ahora, None, 18.0);
    assert!(t.escala == 1e-6 && t.sin_atribuir.is_none() && t.total.iter().find(|x| x.unidad == "tokens").unwrap().texto == "5 k");
    // sin datos
    let vacio = calcular_historico::<Archivo>(&[], &json!({}), ahora, None, 18.0);
    assert!(!vacio.hay_datos && vacio.series.is_empty() && vacio.etiquetas.is_empty());
    // un periodo sin nada (los 7 días que acaban hoy)
    assert!(!calcular_historico(&a, &json!({"periodo": "semana"}), ahora, None, 18.0).hay_datos);
    // semanas archivadas: de la más nueva a la más vieja
    let sem = calcular_semanas(&a);
    assert_eq!(sem.len(), 2);
    assert_eq!((sem[0].inicio.as_str(), sem[1].inicio.as_str()), ("2026-10-31", "2026-10-24"));
    assert!(cerca(sem[1].pct_atribuido, 12.0) && cerca(sem[1].pct_sin_atribuir, 3.5) && cerca(sem[1].pct_repartido, 15.5));
    assert_eq!(sem[1].equipos.iter().map(|e| (e.nombre.as_str(), e.pct)).collect::<Vec<_>>(), vec![("PCA", 8.0), ("PCB", 4.0)]);
    assert_eq!(sem[1].pct_final, Some(10.0));
    assert_eq!(sem[0].equipos.len(), 1);
}

// ---------- archivar ----------

struct Mundo {
    s: Semana,
    cubos: Vec<Cubo>,
    todos: Vec<CuboEq>,
    todas: Vec<Muestra>,
}

fn eq(c: &Cubo, e: &str) -> CuboEq {
    CuboEq {
        inicio: c.inicio as f64,
        proyecto: c.proyecto.clone(),
        sesion: c.sesion.clone(),
        modelo: c.modelo.into(),
        peso: c.peso as f64,
        tokens: c.tokens as f64,
        tarea: c.tarea.into(),
        equipo: e.into(),
    }
}

fn mundo() -> Mundo {
    let s = semana();
    let cubos = s.cubos_a.clone();
    let mut todos: Vec<CuboEq> = cubos.iter().map(|c| eq(c, "PCA")).chain(s.cubos_b.iter().map(|c| eq(c, "PCB"))).collect();
    todos.sort_by(|a, b| a.inicio.total_cmp(&b.inicio));
    let todas = vec![(s.t[0] + 600.0, s.r, 6.0), (s.t[1] + 600.0, s.r, 9.0), (s.t[2] + 600.0, s.r, 13.0)];
    Mundo { s, cubos, todos, todas }
}

impl Mundo {
    /// Añade un cubo propio.
    fn anadir(&mut self, c: Cubo) {
        let e = eq(&c, "PCA");
        self.cubos.push(c);
        self.todos.push(e);
        self.todos.sort_by(|a, b| a.inicio.total_cmp(&b.inicio));
    }
}

fn entrada<'a>(m: &'a Mundo, refs: &'a [&'a CuboEq]) -> Entrada<'a> {
    Entrada { yo: "PCA", reset: m.s.r, pct: Some(13.0), propios: &m.cubos, todos: refs, todas: &m.todas }
}

fn mtime(p: &Path) -> std::time::SystemTime {
    fs::metadata(p).unwrap().modified().unwrap()
}

#[test]
fn archivar_solo_si_cambia() {
    let d = carpeta("archivar");
    let carpetas = vec![d.join("sync1/historial"), d.join("sync2/historial")];
    let mut m = mundo();
    let ahora = m.s.ahora;
    let mut est = EstadoArchivo::default();
    let mut cache = CacheArchivos::default();
    let medidas = Cell::new(0);
    let preguntas = Cell::new(0);
    let nombre = "PCA.semana-2026-10-31.json";
    let calc = || {
        medidas.set(medidas.get() + 1);
        Medidas::default()
    };
    let mut no = || {
        preguntas.set(preguntas.get() + 1);
        false
    };
    let mut si = || {
        preguntas.set(preguntas.get() + 1);
        true
    };

    // Consumo.exe abierto: no escribe nada y lo deja pendiente
    let refs: Vec<&CuboEq> = m.todos.iter().collect();
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora, false, calc, &mut no);
    assert!(r.pendiente && r.escritos == 0 && !carpetas[0].join(nombre).exists());
    assert_eq!(preguntas.get(), 1);

    // ya se puede: el archivo de la semana, en las dos carpetas
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora + 400.0, false, calc, &mut si);
    assert_eq!((r.escritos, r.pendiente), (2, false));
    let arch = cache.leer(&carpetas[0].join(nombre)).unwrap();
    assert!(cerca(suma_pct(&arch), 9.0) && arch.maquina == "PCA" && arch.pct == Some(13.0));
    assert!(!carpetas[0].join("PCB.semana-2026-10-31.json").exists()); // solo lo propio
    let (m1, p1, ms1) = (mtime(&carpetas[0].join(nombre)), preguntas.get(), medidas.get());

    // sin cambios: ni se calcula, ni se pregunta, ni se escribe
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora + 800.0, false, calc, &mut si);
    assert_eq!(r, ResultadoArchivo::default());
    assert_eq!((preguntas.get(), medidas.get()), (p1, ms1));
    assert_eq!(mtime(&carpetas[0].join(nombre)), m1);

    // cambia lo que entra (un cubo nuevo): se reescribe
    m.anadir(cubo(m.s.t[1] + 1200.0, "web", "sesion-aaaa-1111", "opus", 10, 100, "pensar"));
    let refs: Vec<&CuboEq> = m.todos.iter().collect();
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora + 850.0, false, calc, &mut si);
    assert_eq!(r.escritos, 2);
    assert!(cache.leer(&carpetas[1].join(nombre)).unwrap().dias.values().map(|d| d.f.len()).sum::<usize>() >= 3);

    // otro cambio a los 50 s: como mucho cada 5 min, espera; a los 5 min, se escribe
    m.anadir(cubo(m.s.t[1] + 1800.0, "web", "sesion-aaaa-1111", "opus", 10, 100, "web"));
    let refs: Vec<&CuboEq> = m.todos.iter().collect();
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora + 900.0, false, calc, &mut si);
    assert_eq!(r, ResultadoArchivo::default());
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora + 1200.0, false, calc, &mut si);
    assert_eq!(r.escritos, 2);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn archivar_semanas_cerradas_y_puente() {
    let d = carpeta("viejas");
    let carpetas = vec![d.join("sync1/historial"), d.join("sync2/historial")];
    let mut m = mundo();
    // una semana cerrada hace 3 semanas, con un cubo de este equipo y su muestra del % (dentro de los 35 días que se retienen)
    let reset_viejo = m.s.r - 3.0 * SEMANA;
    let t_viejo = reset_viejo - SEMANA + 86400.0; // a un día del inicio de su semana: el reparto no deja huecos de más de 2 días
    m.anadir(cubo(t_viejo, "vieja", "sesion-vvvv-0000", "opus", 40, 900, "pensar"));
    m.todas.insert(0, (t_viejo + 600.0, reset_viejo, 5.0));
    let ahora = m.s.r - 86400.0;
    let mut est = EstadoArchivo::default();
    let mut cache = CacheArchivos::default();
    let mut si = || true;
    // el archivo de otro equipo, solo en la carpeta 1: el puente lo pasa a la 2
    let ajeno = mk_dias("PCB", 2);
    pon(&carpetas[0], &ajeno);
    let refs: Vec<&CuboEq> = m.todos.iter().collect();
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora, false, Medidas::default, &mut si);
    assert_eq!(r.escritos, 4); // semana actual y la cerrada, en las dos carpetas
    assert_eq!(r.puenteados, 1);
    assert!(carpetas[1].join(nombre_archivo(&ajeno)).exists());
    let viejo = format!("PCA.semana-{}.json", iso(fecha_de(reset_viejo - SEMANA)));
    let a = cache.leer(&carpetas[0].join(&viejo)).unwrap();
    assert_eq!(a.dias.values().flat_map(|d| d.f.iter()).map(|f| f.proyecto.as_str()).collect::<Vec<_>>(), vec!["vieja"]);
    assert!(cerca(suma_pct(&a), 5.0) && a.pct == Some(5.0), "{} {:?}", suma_pct(&a), a.pct);
    assert_eq!(est.firmas_viejas(), 1);
    // cambia algo de la semana actual: la cerrada no se vuelve a repartir ni a escribir
    let m1 = mtime(&carpetas[0].join(&viejo));
    m.anadir(cubo(m.s.t[1] + 1200.0, "web", "sesion-aaaa-1111", "opus", 10, 100, "codigo"));
    let refs: Vec<&CuboEq> = m.todos.iter().collect();
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora + 600.0, false, Medidas::default, &mut si);
    assert_eq!(r.escritos, 2);
    assert_eq!(r.puenteados, 0);
    assert_eq!(mtime(&carpetas[0].join(&viejo)), m1);
    // una semana cerrada que ya trae más (un equipo con más datos) no se empeora
    let mut mas = (*cache.leer(&carpetas[0].join(&viejo)).unwrap()).clone();
    mas.dias.insert("2026-10-14".into(), dia_vacio(vec![fl("otra", "PCA", 1.0, 1.0, 0.1, None)]));
    mas.dias.insert("2026-10-13".into(), dia_vacio(vec![]));
    assert!(guardar_archivo(&carpetas[0], &mas, false).unwrap());
    m.todas.insert(1, (t_viejo + 700.0, reset_viejo, 6.0)); // cambia la firma de esa semana: se reparte otra vez
    let refs: Vec<&CuboEq> = m.todos.iter().collect();
    let r = archivar(&mut est, &mut cache, &entrada(&m, &refs), &carpetas, ahora + 1200.0, false, Medidas::default, &mut si);
    assert_eq!(r.escritos, 1); // solo la cerrada en la carpeta 2: en la 1 el archivo guardado lleva más días y no se empeora
    assert!(cache.leer(&carpetas[0].join(&viejo)).unwrap().dias.contains_key("2026-10-14"));
    assert!(!cache.leer(&carpetas[1].join(&viejo)).unwrap().dias.contains_key("2026-10-14"));
    let _ = fs::remove_dir_all(&d);
}

// ---------- humo con datos reales: solo lectura ----------

/// `cargo test humo_archivo_real -- --ignored --nocapture`: lee las carpetas `historial` reales SIN
/// ESCRIBIR NADA y calcula el histórico «todo» por proyecto en €.
#[test]
#[ignore]
fn humo_archivo_real() {
    use super::equipos::Equipos;
    let t0 = std::time::Instant::now();
    let home = PathBuf::from(std::env::var_os("USERPROFILE").expect("USERPROFILE"));
    let barra = home.join(".claude").join("statusbar");
    let eq = Equipos::real(&barra, &home);
    let carpetas = super::disco::carpetas_historial(&eq.activas(), &barra);
    println!("carpetas historial: {carpetas:?}");
    let ficheros: usize = carpetas.iter().map(|c| listar_archivos(c).len()).sum();
    let mut cache = CacheArchivos::default();
    let archivos = cache.leer_archivos_en(&carpetas);
    let primera = t0.elapsed();
    let precio = super::ajustes::leer_suscripcion(&barra.join(super::ajustes::FICHERO_AJUSTES));
    println!("ficheros semanales: {ficheros}; archivos (por equipo y semana): {}; parseados: {}", archivos.len(), cache.lecturas);
    let semanas = calcular_semanas(&archivos);
    println!("semanas distintas: {}", semanas.len());
    for s in semanas.iter().take(3) {
        println!("  {} -> repartido {:.1} % ({} equipos, {} días)", s.inicio, s.pct_repartido, s.equipos.len(), s.dias);
    }
    let t1 = std::time::Instant::now();
    let h = calcular_historico(&archivos, &json!({"periodo": "todo", "grupo": "proyecto", "unidad": "eur"}), super::ahora_s(), None, precio);
    println!("suscripción {precio} €/mes; histórico {} tramos ({}), {} proyectos, total {}", h.etiquetas.len(), h.paso, h.filas.len(), h.total[0].texto);
    for f in h.filas.iter().take(5) {
        println!("  {:<30} {:>10}  {:>6}  ({} semanas, {} -> {})", f.nombre, f.texto, f.texto_participacion, f.semanas, f.primero, f.ultimo);
    }
    println!("sin atribuir: {}", h.sin[0].texto);
    // segunda lectura: todo sale de la caché
    let antes = cache.lecturas;
    let _ = cache.leer_archivos_en(&carpetas);
    println!("segunda lectura: {} ficheros parseados de nuevo", cache.lecturas - antes);
    println!("tiempo: lectura {primera:.1?}, cálculo {:.1?}, total {:.1?}", t1.elapsed(), t0.elapsed());
}
