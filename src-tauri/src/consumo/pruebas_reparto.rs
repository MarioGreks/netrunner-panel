//! Pruebas del reparto y de la sincronización entre equipos, portadas de la función `prueba` de
//! Consumo (detalle.py y consumo.pyw). Todo en carpetas temporales propias: nunca se toca
//! `~/.claude` ni OneDrive.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Local, TimeZone};
use serde_json::{json, Value};

use super::equipos::*;
use super::limites::{Uso, Ventana};
use super::reparto::*;
use super::transcripciones::{Lector, CUBO};
use super::{ciclo_detalle, refrescar, Consumo, Detalle};

const D: f64 = 86_400.0;

fn cerca(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn local(a: i32, m: u32, d: u32, h: u32, mi: u32) -> f64 {
    Local.with_ymd_and_hms(a, m, d, h, mi, 0).unwrap().timestamp() as f64
}

fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("netrunner_reparto_{nombre}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn cubo(inicio: f64, proy: &str, ses: &str, modelo: &str, peso: f64, equipo: &str, tarea: &str) -> CuboEq {
    CuboEq {
        inicio,
        proyecto: proy.into(),
        sesion: ses.into(),
        modelo: modelo.into(),
        peso,
        tokens: 1.0,
        tarea: tarea.into(),
        equipo: equipo.into(),
    }
}

fn refs(v: &[CuboEq]) -> Vec<&CuboEq> {
    v.iter().collect()
}

fn fila<'a>(t: f64, proy: &'a str, ses: &'a str, modelo: &'a str, pct: f64, equipo: &'a str, tarea: &'a str) -> Fila<'a> {
    Fila { t, proyecto: proy, sesion: ses, modelo, pct, equipo, tarea }
}

fn por_proy(filas: &[Fila]) -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    for f in filas {
        *m.entry(f.proyecto.to_string()).or_insert(0.0) += f.pct;
    }
    m
}

// ---------- reparto ----------

#[test]
fn reparto_basico() {
    // 10 % en el primer intervalo entre A (peso 3) y B (peso 1), 5 % sin cubos -> OTROS
    let r = local(2026, 11, 8, 12, 0); // la semana empieza a mediodía: los días naturales no dependen de la zona
    let ini = r - 7.0 * D;
    let dias: Vec<u32> = {
        use chrono::Datelike;
        dias_semana(r).iter().map(|d| d.day()).collect()
    };
    assert_eq!(dias, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(dias_semana(local(2026, 11, 8, 0, 0)).len(), 7);
    let cubos = vec![
        cubo(ini + 100.0, "A", "s1", "sonnet", 300.0, "", ""),
        cubo(ini + 700.0, "B", "s2", "opus", 100.0, "", ""),
        cubo(ini + 2.0 * D + 100.0, "A", "s1", "sonnet", 50.0, "", ""),
    ];
    let m = [
        (ini + D / 4.0, r, 10.0),
        (ini + 1.25 * D, r, 15.0),
        (ini + 3.0 * D, r + 100.0, 15.0),
        (ini + 4.0 * D, r + 5000.0, 99.0),
    ];
    let at = repartir(&m, r, &refs(&cubos));
    let por = por_proy(&at);
    assert!(cerca(por["A"], 7.5) && cerca(por["B"], 2.5) && cerca(por[OTROS], 5.0));
    assert!(cerca(por.values().sum(), 15.0));
    let g = agrupar(&at, r, |f| f.proyecto);
    let a = &g.iter().find(|x| x.0 == "A").unwrap().1;
    let o = &g.iter().find(|x| x.0 == OTROS).unwrap().1;
    let b = &g.iter().find(|x| x.0 == "B").unwrap().1;
    assert!(cerca(a[0], 7.5) && cerca(o[1], 5.0) && b[0] > 2.0 && a.len() == 8);
    assert!(repartir(&[], r, &refs(&cubos)).is_empty());
    assert!(repartir(&[(ini + D, r, 0.0)], r, &refs(&cubos)).is_empty());
    // el primer intervalo dura 4 días (> GAP_MAX): todo a OTROS
    let largo = repartir(&[(ini + 4.0 * D, r, 8.0)], r, &refs(&cubos));
    assert_eq!(largo.len(), 1);
    assert!(largo[0].proyecto == OTROS && cerca(largo[0].pct, 8.0));
    // sin equipo ni tarea en los cubos
    assert_eq!((at[0].equipo, at[0].tarea), ("", ""));
    // cubos sin ordenar: mismo resultado
    let mut al_reves = refs(&cubos);
    al_reves.reverse();
    assert_eq!(repartir(&m, r, &al_reves), at);
}

#[test]
fn reparto_por_equipo() {
    let r = local(2026, 11, 8, 12, 0);
    let ini = r - 7.0 * D;
    let cubos = vec![
        cubo(ini + 100.0, "A", "s1", "sonnet", 300.0, "PC1", ""),
        cubo(ini + 700.0, "B", "s2", "opus", 100.0, "PC1", ""),
        cubo(ini + 2.0 * D + 100.0, "A", "s1", "sonnet", 50.0, "PC2", ""),
    ];
    let m = [(ini + D / 4.0, r, 10.0), (ini + 1.25 * D, r, 15.0)];
    let fe = repartir(&m, r, &refs(&cubos));
    let pe = por_equipo(&fe);
    // el cubo de PC2 cae fuera de las muestras: no suma
    assert_eq!(pe.iter().map(|x| x.0).collect::<Vec<_>>(), vec!["PC1", OTROS]);
    assert!(cerca(pe[0].1, 10.0) && cerca(pe.iter().map(|x| x.1).sum(), 15.0) && pe[1] == (OTROS, 5.0));
    assert!(por_equipo(&[]).is_empty());
}

#[test]
fn reparto_por_solape() {
    // intervalo sin ningún centro de cubo dentro: se reparte entre los cubos que lo solapan
    let t20 = local(2026, 11, 3, 20, 20);
    let cs = vec![
        cubo(t20, "A", "s1", "sonnet", 100.0, "", ""),
        cubo(t20 + CUBO as f64, "B", "s2", "opus", 100.0, "", ""),
    ];
    let a0 = t20 + 7.0 * 60.0 + 5.0; // 20:27:05 -> 20:32:25: los centros (20:25 y 20:35) quedan fuera
    let rx = a0 + 604_800.0;
    let rs = repartir(&[(a0 + 320.0, rx, 1.0)], rx, &refs(&cs));
    let pr = por_proy(&rs);
    assert!(!pr.contains_key(OTROS) && cerca(pr.values().sum(), 1.0));
    assert!(cerca(pr["A"], 175.0 / 320.0) && cerca(pr["B"], 145.0 / 320.0));
    let mut ts: Vec<f64> = rs.iter().map(|f| f.t).collect();
    ts.dedup();
    assert_eq!(ts, vec![t20 + 300.0, t20 + 900.0]);
    // intervalo corto pero sin ningún cubo que lo solape
    let ry = a0 + 7200.0 + 604_800.0;
    let nr = repartir(&[(a0 + 7520.0, ry, 1.0)], ry, &refs(&cs));
    assert!(nr.len() == 1 && nr[0].proyecto == OTROS && cerca(nr[0].pct, 1.0));
}

#[test]
fn por_modelo_tarea_equipo() {
    let tf = vec![
        fila(0.0, "A", "s", "sonnet", 3.0, "PC1", "codigo"),
        fila(0.0, "A", "s", "opus", 1.0, "PC2", "comandos"),
        fila(0.0, "B", "s", "sonnet", 2.0, "PC1", "codigo"),
        fila(0.0, OTROS, OTROS, OTROS, 4.0, OTROS, OTROS),
    ];
    assert_eq!(por_tarea(&tf), vec![("codigo", 5.0), ("comandos", 1.0), (OTROS, 4.0)]);
    assert_eq!(por_modelo(&tf), vec![("sonnet", 5.0), ("opus", 1.0), (OTROS, 4.0)]);
    assert_eq!(por_equipo(&tf), vec![("PC1", 5.0), ("PC2", 1.0), (OTROS, 4.0)]);
}

#[test]
fn principales_y_proyecto_sesion() {
    let gg: Vec<(&str, Vec<f64>)> = vec![
        ("a", vec![3.0, 0.0]),
        ("b", vec![1.0, 1.0]),
        ("c", vec![0.0, 1.0]),
        ("d", vec![0.0, 0.5]),
        (OTROS, vec![2.0, 2.0]),
    ];
    assert_eq!(
        principales(&gg, 2),
        vec![
            ("a", vec![3.0, 0.0]),
            ("b", vec![1.0, 1.0]),
            (RESTO, vec![0.0, 1.5]),
            (OTROS, vec![2.0, 2.0])
        ]
    );
    assert_eq!(principales(&[("a", vec![1.0])], 3), vec![("a", vec![1.0])]);
    assert!(principales(&[], 3).is_empty());
    let at2 = vec![
        fila(0.0, "A", "s1", "x", 3.0, "", ""),
        fila(0.0, "A", "s2", "x", 1.0, "", ""),
        fila(0.0, "A", "s1", "x", 1.0, "", ""),
        fila(0.0, "B", "s3", "x", 2.0, "", ""),
        fila(0.0, "C", "s4", "x", 0.5, "", ""),
        fila(0.0, "D", "s5", "x", 0.25, "", ""),
        fila(0.0, OTROS, OTROS, OTROS, 4.0, OTROS, OTROS),
    ];
    assert_eq!(
        por_proyecto_sesion(&at2, 2),
        vec![
            ("A", 5.0, vec![("s1", 4.0), ("s2", 1.0)]),
            ("B", 2.0, vec![("s3", 2.0)]),
            (RESTO, 0.75, vec![("s4", 0.5), ("s5", 0.25)]),
            (OTROS, 4.0, vec![(OTROS, 4.0)]),
        ]
    );
    assert!(por_proyecto_sesion(&[], 5).is_empty());
}

#[test]
fn sesiones_pequenas() {
    // se juntan en un trozo (o se suman a la última grande si ni juntas llegan) y la suma no cambia
    let ses = [("a", 5.0), ("b", 2.0), ("c", 0.5), ("d", 0.4), ("e", 0.3)];
    let r = agrupar_pequenas(&ses, 1.0);
    assert_eq!(r.len(), 3);
    assert!(r[2].0 == PEQUENAS && cerca(r[2].1, 1.2));
    let r = agrupar_pequenas(&ses, 1.5);
    assert_eq!(r.iter().map(|s| s.0).collect::<Vec<_>>(), vec!["a", "b"]);
    assert!(cerca(r[1].1, 3.2));
    assert!(cerca(r.iter().map(|s| s.1).sum(), 7.0 + 1.2));
    assert_eq!(agrupar_pequenas(&ses, 0.1), ses.to_vec());
    assert_eq!(agrupar_pequenas(&[("x", 0.2)], 1.0), vec![(PEQUENAS, 0.2)]);
    assert!(agrupar_pequenas(&[], 1.0).is_empty());
}

#[test]
fn unir_cubos_no_duplica() {
    // el mismo cubo en dos equipos cuenta una vez (el de más peso); los distintos, todos
    let a = vec![cubo(10.0, "P", "s1", "sonnet", 100.0, "PC1", "")];
    let b = vec![cubo(10.0, "P", "s1", "sonnet", 120.0, "PC2", "")];
    let c = vec![cubo(20.0, "P", "s1", "sonnet", 1.0, "PC2", "")];
    assert_eq!(unir_cubos(&[&a, &b]), vec![&b[0]]);
    assert_eq!(unir_cubos(&[&a, &c]), vec![&a[0], &c[0]]);
    assert!(unir_cubos(&[]).is_empty());
    assert_eq!(unir_cubos(&[&a, &a]), vec![&a[0]]);
    // a igualdad gana el primero (el propio)
    let a2 = vec![cubo(10.0, "P", "s1", "sonnet", 100.0, "PC9", "")];
    assert_eq!(unir_cubos(&[&a, &a2])[0].equipo, "PC1");
}

fn f_hora(t: f64, p: &str, pct: f64) -> Fila<'_> {
    fila(t, p, "s", "x", pct, "", "")
}

#[test]
fn consumo_por_hora() {
    let ya = local(2026, 11, 5, 12, 0); // jueves a mediodía
    let ini7 = local(2026, 10, 30, 0, 0); // el viernes a medianoche: 6 días antes
    let a3 = vec![
        f_hora(ini7 + 5.0, "A", 2.0),
        f_hora(ini7 + 3600.0 * 25.0 + 10.0, "A", 1.0),
        f_hora(ini7 + 3600.0 * 25.0 + 20.0, "B", 3.0),
        f_hora(ya, "C", 0.5),
        f_hora(ya, OTROS, 4.0),
        f_hora(ini7 - 10.0, "A", 9.0),
        f_hora(ini7 + 7.0 * 86400.0 + 1.0, "A", 9.0),
    ];
    let (nom, ser, i0) = por_hora(&a3, ya, 2, false);
    assert_eq!(i0, ini7);
    assert_eq!(nom, vec!["A", "B", RESTO, OTROS]);
    assert!(ser.iter().all(|x| x.len() == 168));
    assert!(ser[0][0] == 2.0 && ser[0][25] == 1.0 && ser[1][25] == 3.0);
    assert_eq!(ser[2][((ya - ini7) / 3600.0) as usize], 0.5);
    assert!(cerca(ser.iter().flatten().sum(), 10.5)); // lo de fuera de los 7 días no cuenta
    let vacio = por_hora(&[], ya, 5, false);
    assert!(vacio.0.is_empty() && vacio.1.is_empty());
    // arrastrar: lo anterior a los 7 días se suma a la primera hora; lo posterior sigue fuera
    let (_, ser2, _) = por_hora(&a3, ya, 2, true);
    assert!(cerca(ser2.iter().flatten().sum(), 19.5) && ser2[0][0] == 11.0 && ser2[0][25] == 1.0);
}

#[test]
fn estado_en_un_instante() {
    // solo lo anterior a T, con el % de la semana y la ventana de 5 h si sigue abierta
    let r2 = local(2026, 11, 7, 16, 0);
    let t0 = r2 - 604_800.0;
    let hist = [(t0 + 100.0, r2, 5.0), (t0 + 5000.0, r2, 9.0), (t0 + 90_000.0, r2, 20.0), (t0 + 200.0, r2 - 604_800.0, 99.0)];
    let hist5 = [(t0 + 100.0, t0 + 18_000.0, 4.0), (t0 + 5000.0, t0 + 18_000.0, 7.0)];
    let fil = vec![
        fila(t0 + 400.0, "A", "s1", "opus", 5.0, "PC1", "comandos"),
        fila(t0 + 6000.0, "B", "s2", "sonnet", 4.0, "PC2", "codigo"),
        fila(t0 + 95_000.0, "A", "s1", "opus", 11.0, "PC1", "comandos"),
    ];
    let cub = vec![
        cubo(t0, "A", "s1", "opus", 1.0, "PC1", "comandos"),
        cubo(t0 + 5400.0, "B", "s2", "sonnet", 1.0, "PC2", "codigo"),
    ];
    let e0 = estado_en(t0 + 5999.0, r2, &hist, &hist5, &fil, &refs(&cub));
    assert_eq!(e0.semanal, (9.0, r2)); // la semana vieja no cuenta
    assert_eq!(e0.cinco_horas, Some((7.0, t0 + 18_000.0)));
    assert_eq!((e0.todas.len(), e0.atrib.1.len()), (3, 1));
    assert_eq!(e0.equipos, vec![("PC1", 5.0)]);
    assert_eq!(e0.tareas, vec![("comandos", 5.0)]);
    assert_eq!(e0.ultima, BTreeMap::from([("PC1", t0 + 600.0)]));
    let e1 = estado_en(t0 + 100_000.0, r2, &hist, &hist5, &fil, &refs(&cub));
    assert!(e1.semanal.0 == 20.0 && e1.cinco_horas.is_none() && e1.equipos[0] == ("PC1", 16.0));
    assert_eq!(e1.ultima.keys().copied().collect::<Vec<_>>(), vec!["PC1", "PC2"]);
    let e2 = estado_en(t0 - 1.0, r2, &hist, &hist5, &fil, &refs(&cub));
    assert!(e2.semanal.0 == 0.0 && e2.equipos.is_empty() && e2.ultima.is_empty());
    assert_eq!(e2.atrib, (r2, vec![]));
}

// ---------- equipos ----------

#[test]
fn limpieza_de_lo_ajeno() {
    // detalle: cubos [inicio, proyecto, sesión, modelo, peso, tokens(, tarea)] bien formados
    assert!(limpiar_detalle(&json!([1, 2])).is_none());
    assert!(limpiar_detalle(&json!({"machine": "X"})).is_none());
    let d = limpiar_detalle(&json!({"machine": "PC2", "ts": 5, "cubos": [
        [1, "P", "s", "sonnet", 10, 20],
        [2, "P", "s", "sonnet", 10, 20, "codigo"],
        [3, "P", "s", "sonnet", 10],
        [4, "P", 7, "sonnet", 10, 20],
        [5, "P", "s", "sonnet", "x", 20],
        [6, "P", "s", "sonnet", 1, 2, 3],
        "roto", null]}))
    .unwrap();
    assert_eq!((d.machine.as_str(), d.ts, d.cubos.len()), ("PC2", 5.0, 2));
    assert_eq!((d.cubos[0].tarea.as_str(), d.cubos[1].tarea.as_str()), ("", "codigo")); // antiguo: sin tarea
    assert!(d.cubos.iter().all(|c| c.equipo == "PC2"));
    assert_eq!(limpiar_detalle(&json!({"cubos": []})).unwrap().machine, "?");
    // consumo: solo ventanas con pct numérico y resetsAt numérico o ausente
    let c = limpiar_consumo(&json!({"machine": "PC3", "ts": 9,
        "uso": {"fiveHour": {"pct": 4}, "sevenDay": {"pct": "x"}, "otra": {"pct": 1}},
        "hist": [[1, 2, 3], [1, 2], ["a", 2, 3], [4, 5, 6]], "hist5": "no"}))
    .unwrap();
    assert_eq!(c.uso.cinco_horas, Some(Ventana { pct: 4.0, resets_at: None }));
    assert!(c.uso.semanal.is_none());
    assert_eq!((c.hist, c.hist5), (vec![(1.0, 2.0, 3.0), (4.0, 5.0, 6.0)], vec![]));
    assert!(limpiar_consumo(&json!("x")).is_none());
}

fn ven(pct: f64, reset: Option<f64>) -> Option<Ventana> {
    Some(Ventana { pct, resets_at: reset })
}

fn otro(ts: f64, sem: Option<Ventana>, cinco: Option<Ventana>, hist: Vec<(f64, f64, f64)>) -> Arc<OtroConsumo> {
    Arc::new(OtroConsumo {
        machine: "O".into(),
        ts,
        uso: Uso { cinco_horas: cinco, semanal: sem, ts: 0.0 },
        hist,
        hist5: vec![],
    })
}

#[test]
fn fusion_de_uso_e_historial() {
    let r = 1_000_000.0;
    let l = Uso { cinco_horas: None, semanal: ven(10.0, Some(r)), ts: 100.0 };
    // misma ventana (reset ±10 min): el % más alto; el de otro equipo gana si sube
    let f = fusionar_uso(&l, &[otro(50.0, ven(30.0, Some(r + 300.0)), None, vec![])]);
    assert_eq!(f.semanal, ven(30.0, Some(r + 300.0)));
    let f = fusionar_uso(&l, &[otro(500.0, ven(5.0, Some(r)), None, vec![])]);
    assert_eq!(f.semanal, ven(10.0, Some(r))); // el % solo sube, aunque el otro sea más reciente
    // manda la ventana más nueva (reset más lejano) y no la más vieja
    let f = fusionar_uso(&l, &[otro(1.0, ven(2.0, Some(r + 604_800.0)), None, vec![])]);
    assert_eq!(f.semanal, ven(2.0, Some(r + 604_800.0)));
    let f = fusionar_uso(&l, &[otro(999.0, ven(90.0, Some(r - 604_800.0)), None, vec![])]);
    assert_eq!(f.semanal, ven(10.0, Some(r)));
    // lo que falta en local lo pone otro; sin reset, la leída más tarde; conserva el ts local
    let f = fusionar_uso(&l, &[otro(7.0, None, ven(40.0, Some(5.0)), vec![])]);
    assert_eq!((f.cinco_horas, f.ts), (ven(40.0, Some(5.0)), 100.0));
    let sin = Uso { cinco_horas: None, semanal: ven(10.0, None), ts: 100.0 };
    assert_eq!(fusionar_uso(&sin, &[otro(200.0, ven(3.0, None), None, vec![])]).semanal, ven(3.0, None));
    assert_eq!(fusionar_uso(&sin, &[otro(50.0, ven(3.0, None), None, vec![])]).semanal, ven(10.0, None));
    assert_eq!(fusionar_uso(&Uso::default(), &[]), Uso::default());
    // historial: unión ordenada y sin repetidos
    let propio = [(5.0, 1.0, 1.0), (1.0, 1.0, 1.0)];
    let o = [(3.0, 1.0, 1.0), (5.0, 1.0, 1.0)];
    assert_eq!(
        fusionar_hist(&propio, [&o[..]].into_iter()),
        vec![(1.0, 1.0, 1.0), (3.0, 1.0, 1.0), (5.0, 1.0, 1.0)]
    );
}

#[test]
fn carpetas_de_sincronizacion() {
    let d = carpeta("sync_cfg");
    let fich = d.join("consumo_sync");
    let od = d.join("OneDrive");
    // sin fichero: la de OneDrive
    assert_eq!(carpetas_sync(&fich, &od), vec![od.join("Claude-consumo")]);
    // una ruta por línea, con BOM, vacías y repetidas (sin distinguir mayúsculas ni la barra final)
    fs::write(&fich, "\u{feff}C:\\Uno\\Sync\r\n\r\n  D:\\Dos  \r\nc:\\uno\\sync\\\r\n").unwrap();
    assert_eq!(carpetas_sync(&fich, &od), vec![PathBuf::from("C:\\Uno\\Sync"), PathBuf::from("D:\\Dos")]);
    fs::write(&fich, "\r\n  \r\n").unwrap();
    assert_eq!(carpetas_sync(&fich, &od), vec![od.join("Claude-consumo")]);
    // solo las que existen, sin repetir
    let (a, b) = (d.join("a"), d.join("b"));
    fs::create_dir_all(&a).unwrap();
    assert_eq!(carpetas_activas(&[a.clone(), b, a.clone()]), vec![a]);
    let _ = fs::remove_dir_all(&d);
}

fn escribir(ruta: &Path, v: &Value) {
    fs::write(ruta, v.to_string()).unwrap();
}

#[test]
fn leer_otros_con_cache() {
    let d = carpeta("leer");
    let (c1, c2) = (d.join("c1"), d.join("c2"));
    fs::create_dir_all(&c1).unwrap();
    fs::create_dir_all(&c2).unwrap();
    let cub = |t: i64| json!([t, "P", "s", "sonnet", 10, 20, "codigo"]);
    escribir(&c1.join("PC2.detalle.json"), &json!({"machine": "PC2", "ts": 100, "cubos": [cub(1)]}));
    // el mismo equipo por la otra cuenta de OneDrive: manda el ts más reciente
    escribir(&c2.join("PC2.detalle.json"), &json!({"machine": "PC2", "ts": 200, "cubos": [cub(1), cub(2)]}));
    escribir(&c1.join("YO.detalle.json"), &json!({"machine": "YO", "ts": 1, "cubos": [cub(9)]})); // el propio se ignora
    fs::write(c1.join("PC3.detalle.json"), "{ a medias").unwrap(); // roto: se descarta
    fs::write(c1.join("PC4.detalle.json"), "").unwrap();
    fs::write(c1.join("PC5.detalle.json.tmp"), "x").unwrap();
    escribir(&c1.join("PC6.consumo.json"), &json!({"machine": "PC6", "ts": 1}));
    let mut cache: Cache<OtroDetalle> = Cache::default();
    let carpetas = [c1.clone(), c2.clone()];
    let (v, f1) = leer_otros(&mut cache, &carpetas, ".detalle.json", "yo", limpiar_detalle);
    assert_eq!(v.len(), 1);
    assert_eq!((v[0].machine.as_str(), v[0].cubos.len()), ("PC2", 2));
    // sin cambios: de la caché (el mismo dato, sin releer) y la misma firma
    let (v2, f2) = leer_otros(&mut cache, &carpetas, ".detalle.json", "yo", limpiar_detalle);
    assert!(Arc::ptr_eq(&v[0], &v2[0]) && f1 == f2);
    // cambia uno: se relee y cambia la firma
    escribir(&c2.join("PC2.detalle.json"), &json!({"machine": "PC2", "ts": 300, "cubos": [cub(1), cub(2), cub(3)]}));
    let (v3, f3) = leer_otros(&mut cache, &carpetas, ".detalle.json", "yo", limpiar_detalle);
    assert!(v3[0].cubos.len() == 3 && f3 != f2);
    // un fichero que se arregla se vuelve a leer; uno que desaparece sale de la caché
    escribir(&c1.join("PC3.detalle.json"), &json!({"machine": "PC3", "ts": 5, "cubos": []}));
    assert_eq!(leer_otros(&mut cache, &carpetas, ".detalle.json", "yo", limpiar_detalle).0.len(), 2);
    fs::remove_file(c1.join("PC3.detalle.json")).unwrap();
    assert_eq!(leer_otros(&mut cache, &carpetas, ".detalle.json", "yo", limpiar_detalle).0.len(), 1);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn publicar_solo_si_cambia() {
    let d = carpeta("publicar");
    let (a, b) = (d.join("a"), d.join("b"));
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    let carpetas = [a.clone(), b.clone()];
    let mut p = Publicador::default();
    let mut texto_pedido = 0;
    // no permitido (Consumo.exe abierto): no escribe ni construye el texto
    let r = p.publicar(
        "x.json",
        1,
        || {
            texto_pedido += 1;
            "uno".to_string()
        },
        &carpetas,
        || false,
    );
    assert_eq!((r, texto_pedido), (Publicacion { escritas: 0, pendiente: true }, 0));
    assert!(!a.join("x.json").exists());
    // permitido: escribe en las dos, sin dejar el .tmp
    let r = p.publicar("x.json", 1, || "uno".to_string(), &carpetas, || true);
    assert_eq!(r, Publicacion { escritas: 2, pendiente: false });
    assert_eq!(fs::read_to_string(b.join("x.json")).unwrap(), "uno");
    assert!(!a.join("x.json.tmp").exists());
    // mismo contenido: no escribe nada y ni siquiera pregunta
    let r = p.publicar("x.json", 1, || panic!("no hace falta"), &carpetas, || panic!("no hace falta"));
    assert_eq!(r, Publicacion::default());
    // contenido nuevo: se reescribe; una carpeta que aparece después recibe el vigente
    let c = d.join("c");
    let r = p.publicar("x.json", 2, || "dos".to_string(), &carpetas, || true);
    assert_eq!(r.escritas, 2);
    fs::create_dir_all(&c).unwrap();
    let tres = [a.clone(), b.clone(), c.clone()];
    assert_eq!(p.publicar("x.json", 2, || "dos".to_string(), &tres, || true).escritas, 1);
    assert_eq!(fs::read_to_string(c.join("x.json")).unwrap(), "dos");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn equipos_publica_y_lee() {
    let d = carpeta("equipos");
    let sync = d.join("sync");
    fs::create_dir_all(&sync).unwrap();
    let fich = d.join("consumo_sync");
    // la segunda carpeta no existe (OneDrive aún no la creó): se salta
    fs::write(&fich, format!("{}\n{}\n", sync.display(), d.join("no_existe").display())).unwrap();
    let mut e = Equipos::nuevo("PC-YO".into(), fich, d.join("OneDrive"));
    assert_eq!(e.activas(), vec![sync.clone()]);
    let uso = Uso { cinco_horas: ven(4.0, Some(10.0)), semanal: ven(12.5, Some(99.0)), ts: 1234.0 };
    let h = [(1.0, 99.0, 12.5)];
    let p = e.publicar_consumo(&uso, &h, &[], || true);
    assert_eq!(p.escritas, 1);
    assert_eq!(e.publicar_consumo(&uso, &h, &[], || true), Publicacion::default()); // no cambió
    assert_eq!(e.publicar_consumo(&Uso::default(), &h, &[], || true), Publicacion::default()); // sin ts: nada
    let v: Value = serde_json::from_str(&fs::read_to_string(sync.join("PC-YO.consumo.json")).unwrap()).unwrap();
    assert_eq!(v["machine"], "PC-YO");
    assert_eq!(v["uso"]["sevenDay"]["pct"], 12.5);
    assert_eq!(v["hist"], json!([[1.0, 99.0, 12.5]]));
    // otro equipo (con Consumo en Python) publica lo suyo; el propio no cuenta como «otro»
    fs::write(
        sync.join("PC-AMIGO.consumo.json"),
        r#"{"machine": "PC-AMIGO", "ts": 5, "uso": {"sevenDay": {"pct": 20, "resetsAt": 99}}, "hist": [[2, 99, 20]], "hist5": []}"#,
    )
    .unwrap();
    let (otros, f) = e.leer_consumo();
    assert_eq!(otros.len(), 1);
    assert_eq!(otros[0].machine, "PC-AMIGO");
    assert_eq!(fusionar_uso(&uso, &otros).semanal, ven(20.0, Some(99.0)));
    assert_eq!(e.leer_consumo().1, f);
    // los cubos propios, en el formato de siempre
    let cj = cubos_json(&[]);
    assert_eq!(cj, "[]");
    assert_eq!(e.publicar_detalle(cj, || true).escritas, 1);
    let v: Value = serde_json::from_str(&fs::read_to_string(sync.join("PC-YO.detalle.json")).unwrap()).unwrap();
    assert!(v["machine"] == "PC-YO" && v["cubos"] == json!([]) && v["ts"].as_u64().unwrap() > 1_700_000_000);
    // solo lectura: no escribe nada
    e.solo_lectura = true;
    assert_eq!(e.publicar_detalle("[1]".into(), || true), Publicacion::default());
    let _ = fs::remove_dir_all(&d);
}

// ---------- el ciclo completo (usage.json + otros equipos + transcripciones) ----------

fn iso(t: f64) -> String {
    chrono::DateTime::from_timestamp(t as i64, 0).unwrap().to_rfc3339()
}

#[test]
fn ciclo_completo() {
    let d = carpeta("ciclo");
    let (barra, sync, proy) = (d.join("statusbar"), d.join("sync"), d.join("projects"));
    for c in [&barra, &sync, &proy.join("p")] {
        fs::create_dir_all(c).unwrap();
    }
    let ahora = super::ahora_s().floor();
    let reset = ahora + 3.0 * D;
    // usage.json de este equipo y el historial propio (dos muestras de la semana)
    escribir(
        &barra.join("usage.json"),
        &json!({"ts": ahora, "sevenDay": {"pct": 12, "resetsAt": reset}, "fiveHour": {"pct": 3, "resetsAt": ahora + 3600.0}}),
    );
    escribir(&barra.join("consumo_historial.json"), &json!([[ahora - 2.0 * D, reset, 5], [ahora - D, reset, 10]]));
    // un mensaje propio y un cubo de otro equipo, en el intervalo (ahora-2d, ahora-1d] con el mismo peso
    let t = ahora - 1.5 * D;
    let linea = json!({"type": "assistant", "timestamp": iso(t), "cwd": "C:\\x\\Proj", "sessionId": "s1",
        "message": {"id": "m1", "model": "claude-sonnet-5-5",
            "usage": {"input_tokens": 10, "cache_creation_input_tokens": 100, "cache_read_input_tokens": 1000, "output_tokens": 10}}});
    fs::write(proy.join("p").join("s1.jsonl"), format!("{linea}\n")).unwrap();
    let inicio = (t / 600.0).floor() * 600.0;
    escribir(
        &sync.join("PC-AMIGO.detalle.json"),
        &json!({"machine": "PC-AMIGO", "ts": 1, "cubos": [[inicio, "Otra", "s9", "sonnet", 285, 5, "codigo"]]}),
    );
    escribir(
        &sync.join("PC-AMIGO.consumo.json"),
        &json!({"machine": "PC-AMIGO", "ts": ahora - 100.0, "uso": {"sevenDay": {"pct": 12.5, "resetsAt": reset}}, "hist": [], "hist5": []}),
    );

    let fich = barra.join("consumo_sync");
    fs::write(&fich, sync.display().to_string()).unwrap();
    let c = Consumo::nuevo(Some(barra.clone()), Equipos::nuevo("PC-YO".into(), fich, d.join("OneDrive")));
    let det = Detalle::con_lector(Lector::new(proy).con_es_repo(|_| false));

    // 1. el muestreador: fusiona (el 12,5 % del otro gana), apunta y publica lo propio una vez
    assert!(refrescar(&c, || false));
    {
        let g = c.interno.lock().unwrap();
        assert_eq!(g.uso.semanal.as_ref().unwrap().pct, 12.5);
        assert_eq!(g.uso_local.semanal.as_ref().unwrap().pct, 12.0);
        assert!(g.todas.len() >= 3 && g.hist.len() == 3 && !g.sucio_hist);
    }
    let propio = sync.join("PC-YO.consumo.json");
    assert!(propio.exists() && barra.join("consumo_historial.json").exists());
    let m1 = fs::metadata(&propio).unwrap().modified().unwrap();
    assert!(!refrescar(&c, || false)); // sin cambios: ni se lee ni se publica
    assert_eq!(fs::metadata(&propio).unwrap().modified().unwrap(), m1);

    // 2. la lectura: publica el detalle propio y reparte entre los dos equipos
    let res = ciclo_detalle(&c, &det, ahora, || true).unwrap();
    assert_eq!(res.reset, reset);
    let proyecto = |n: &str| res.proyectos.iter().find(|g| g.nombre == n).map(|g| g.pct);
    assert!((proyecto("Proj").unwrap() - 2.5).abs() < 1e-6 && (proyecto("Otra").unwrap() - 2.5).abs() < 1e-6);
    assert!(proyecto(OTROS).unwrap() > 7.0); // lo que ningún cubo explica
    let total: f64 = res.proyectos.iter().map(|g| g.pct).sum();
    assert!((total - 12.5).abs() < 1e-6);
    let eq: Vec<&str> = res.equipos.iter().map(|e| e.nombre.as_str()).collect();
    assert!(eq.contains(&"PC-YO") && eq.contains(&"PC-AMIGO") && eq.last() == Some(&OTROS));
    assert_eq!(res.modelos[0].nombre, "sonnet");
    assert!(res.tareas.iter().any(|t| t.nombre == "codigo"));
    assert_eq!(res.sin_tarea, vec!["PC-YO".to_string()]); // el mensaje propio no tiene bloques: sin tarea
    assert_eq!(res.dias.fechas.len(), res.dias.total.len());
    assert_eq!(res.por_hora.series.first().map(|s| s.valores.len()), Some(168));
    assert!(res.previas.is_none());
    let pub_det = sync.join("PC-YO.detalle.json");
    let v: Value = serde_json::from_str(&fs::read_to_string(&pub_det).unwrap()).unwrap();
    assert_eq!(v["cubos"].as_array().unwrap().len(), 1);
    assert_eq!(v["cubos"][0][1], "Proj");
    // sin cambios en los cubos propios: no se vuelve a escribir (OneDrive subiría cada escritura)
    let m2 = fs::metadata(&pub_det).unwrap().modified().unwrap();
    ciclo_detalle(&c, &det, ahora, || panic!("no hay nada que escribir")).unwrap();
    assert_eq!(fs::metadata(&pub_det).unwrap().modified().unwrap(), m2);
    // JSON de la interfaz en camelCase
    let js = serde_json::to_value(&res).unwrap();
    assert!(js["porHora"]["series"].is_array() && js["sinTarea"].is_array() && js["dias"]["fechas"].is_array());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn consumo_exe_abierto_no_escribe() {
    let d = carpeta("exe");
    let (barra, sync) = (d.join("statusbar"), d.join("sync"));
    fs::create_dir_all(&barra).unwrap();
    fs::create_dir_all(&sync).unwrap();
    let ahora = super::ahora_s().floor();
    escribir(&barra.join("usage.json"), &json!({"ts": ahora, "sevenDay": {"pct": 12, "resetsAt": ahora + D}}));
    let fich = barra.join("consumo_sync");
    fs::write(&fich, sync.display().to_string()).unwrap();
    let c = Consumo::nuevo(Some(barra.clone()), Equipos::nuevo("PC-YO".into(), fich, d.join("OneDrive")));
    assert!(refrescar(&c, || true)); // Consumo.exe en marcha
    assert!(!sync.join("PC-YO.consumo.json").exists() && !barra.join("consumo_historial.json").exists());
    assert!(c.interno.lock().unwrap().pub_pendiente);
    assert!(refrescar(&c, || false)); // se cierra: se publica lo pendiente, aunque usage.json no haya cambiado
    assert!(sync.join("PC-YO.consumo.json").exists() && barra.join("consumo_historial.json").exists());
    let _ = fs::remove_dir_all(&d);
}

// ---------- humo con datos reales: solo lectura ----------

/// `cargo test humo_reparto_real -- --ignored --nocapture`: lee usage.json, el historial,
/// `~/.claude/projects` y la carpeta de sincronización reales SIN ESCRIBIR NADA y cuenta lo que ve.
#[test]
#[ignore]
fn humo_reparto_real() {
    let t0 = std::time::Instant::now();
    let home = PathBuf::from(std::env::var_os("USERPROFILE").expect("USERPROFILE"));
    let barra = home.join(".claude").join("statusbar");
    let mut eq = Equipos::real(&barra, &home);
    eq.solo_lectura = true;
    println!("equipo: {}", eq.yo);
    println!("carpetas de sync encontradas: {:?}", eq.activas());
    let mut c = Consumo::nuevo(Some(barra), eq);
    c.solo_lectura = true;
    let det = Detalle::nuevo();
    refrescar(&c, || true);
    let ahora = super::ahora_s();
    {
        let g = c.interno.lock().unwrap();
        println!(
            "uso fusionado: semanal {:?}; historial propio {} muestras, fusionado {}",
            g.uso.semanal,
            g.hist.len(),
            g.todas.len()
        );
    }
    let (otros, _) = c.equipos.lock().unwrap().leer_consumo();
    println!("equipos leídos (consumo.json): {:?}", otros.iter().map(|o| &o.machine).collect::<Vec<_>>());
    let t1 = std::time::Instant::now();
    let res = ciclo_detalle(&c, &det, ahora, || false);
    println!("lectura de transcripciones + reparto: {:.1?}", t1.elapsed());
    let otros_det = c.equipos.lock().unwrap().leer_detalle();
    println!(
        "equipos con detalle (cubos): {:?}",
        otros_det.iter().map(|o| (&o.machine, o.cubos.len())).collect::<Vec<_>>()
    );
    let r2 = |x: f64| (x * 100.0).round() / 100.0;
    match res {
        None => println!("sin semana: no hay reparto"),
        Some(r) => {
            println!("semana hasta {} (% semanal {:?})", r.reset, r.pct);
            println!("top proyectos:");
            for g in r.proyectos.iter().take(8) {
                println!("  {:<28} {:>6.2} %  ({} sesiones)", g.nombre, g.pct, g.sesiones.len());
            }
            println!("equipos: {:?}", r.equipos.iter().map(|e| (&e.nombre, r2(e.pct))).collect::<Vec<_>>());
            println!("modelos: {:?}", r.modelos.iter().map(|e| (&e.nombre, r2(e.pct))).collect::<Vec<_>>());
            println!("tareas: {:?}", r.tareas.iter().map(|e| (&e.nombre, r2(e.pct))).collect::<Vec<_>>());
            println!("días: {:?} total {:?}", r.dias.fechas, r.dias.total.iter().map(|x| r2(*x)).collect::<Vec<_>>());
            println!("previas: {:?}", r.previas.as_ref().map(|p| (p.reset, r2(p.pct), p.proyectos.len())));
        }
    }
    println!("tiempo total: {:.1?}", t0.elapsed());
}
