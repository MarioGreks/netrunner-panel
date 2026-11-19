//! Pruebas portadas de la sección `--test` de Consumo (consumo.pyw, función `prueba`).

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{Local, TimeZone};

use super::limites::*;
use super::calcular_estado;

const D: f64 = 86_400.0;
const R: f64 = 604_800.0;

fn cerca(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn ven(pct: f64, reset: f64) -> Ventana {
    Ventana {
        pct,
        resets_at: Some(reset),
    }
}

/// Carpeta temporal propia (nunca ~/.claude), vacía.
fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("netrunner_consumo_{nombre}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn ritmo_basico() {
    let t = 1_000_000.0;
    assert_eq!(ritmo(None, 100.0, t), None);
    let r = ritmo(Some(&ven(40.0, t + 50.0)), 100.0, t).unwrap();
    assert_eq!((r.pct, r.esperado, r.falta), (40.0, 50.0, Some(50.0)));
    let r = ritmo(Some(&ven(40.0, t - 1.0)), 100.0, t).unwrap();
    assert_eq!((r.pct, r.esperado, r.falta), (0.0, 0.0, None));
    assert_eq!(ritmo(Some(&ven(5.0, t + 500.0)), 100.0, t).unwrap().esperado, 0.0);
    assert_eq!(proyeccion(30.0, 40.0), Some(75.0));
    assert_eq!(proyeccion(5.0, 1.0), None);
}

#[test]
fn texto_falta() {
    assert_eq!(falta(None), "esperando datos");
    assert_eq!(falta(Some(90000.0)), "1d 1h");
    assert_eq!(falta(Some(3700.0)), "1h 01m");
    assert_eq!(falta(Some(600.0)), "10m");
}

#[test]
fn pct_en_ventana() {
    let m = [(100.0, R, 5.0), (D + 100.0, R, 20.0)];
    assert_eq!(pct_en(&m, R, D), Some(5.0));
    assert_eq!(pct_en(&m, R + 50.0, D), Some(5.0));
    assert_eq!(pct_en(&m, R, 50.0), None);
    assert_eq!(pct_en(&m, R + 5000.0, 2.0 * D), None);
}

#[test]
fn estimaciones_de_hoy() {
    let e = estimar_hoy(50.0, R, 3.5 * D, Some(40.0), &[]).unwrap();
    assert!(cerca(e.gastado, 10.0));
    assert!((e.estimado - 17.142857).abs() < 1e-4);
    assert!((e.riesgo - 0.04761904444444444).abs() < 1e-4);
    assert_eq!(estimar_hoy(90.0, R, 3.5 * D, Some(60.0), &[]).unwrap().riesgo, 1.0);
    assert_eq!(estimar_hoy(90.0, R, 6.5 * D, Some(60.0), &[]).unwrap().riesgo, 0.0);
    assert!(estimar_hoy(5.0, R, 0.05 * D, None, &[]).is_none());
    assert!(cerca(
        estimar_hoy(50.0, R, 3.5 * D, None, &[]).unwrap().gastado,
        7.142857142857143
    ));
    // media móvil: pesos 1, 1/2, 1/4... para lo anterior y 2 × fracción para lo actual
    assert!(cerca(ritmo_medio(&[20.0, 10.0], 0.0, 0.0), 25.0 / 1.5));
    let e = estimar_hoy(50.0, R, 3.5 * D, Some(40.0), &[10.0, 10.0, 10.0]).unwrap();
    let rd = e.ritmo_dia.unwrap();
    assert!(cerca(rd, 37.5 / 2.75) && cerca(e.estimado, 10.0 + 0.5 * rd));
    // justo al cambiar de día: 0,5 % en la 1.ª hora
    let est = estimar_hoy(50.0, R, 3.05 * D, Some(49.5), &[10.0, 10.0, 10.0]).unwrap().estimado;
    assert!(est < 14.0);
    assert!(estimar_hoy(50.0, R, 3.05 * D, Some(49.5), &[]).unwrap().estimado > 14.0);
}

#[test]
fn proyecciones_de_la_semana() {
    let (total, resto) = proyeccion_semana(50.0, R, D, None).unwrap();
    assert!(cerca(total, 350.0) && cerca(resto, 300.0));
    assert!(cerca(proyeccion_semana(30.0, R, 3.5 * D, None).unwrap().1, 30.0));
    assert!(cerca(proyeccion_semana(50.0, R, D, Some(10.0)).unwrap().1, 60.0));
    assert!(proyeccion_semana(5.0, R, 0.1 * D, Some(10.0)).unwrap().1 > 0.0);
    assert!(proyeccion_semana(5.0, R, 0.1 * D, None).is_none());
    assert_eq!(proyeccion_semana(50.0, R, 8.0 * D, None).unwrap().1, 0.0);
}

#[test]
fn historial_previo() {
    let m = [(0.0, R, 0.0), (D, R, 10.0), (2.0 * D, R, 25.0)];
    assert_eq!(gastos_previos(&m, R, 2.5 * D), vec![15.0, 10.0]);
    let m5 = [
        (100.0, 1000.0, 10.0),
        (200.0, 1000.0, 30.0),
        (5000.0, 20000.0, 5.0),
        (9000.0, 30000.0, 0.0),
        (35000.0, 40100.0, 7.0),
    ];
    assert_eq!(previos_5h(&m5, 40000.0), vec![5.0, 30.0]);
}

#[test]
fn agotado() {
    assert_eq!(agotado_el(50.0, R, D, None), Some(2.0 * D));
    assert_eq!(agotado_el(50.0, R, D, Some(60.0)), Some(D + 50.0 / 60.0 * D));
    assert_eq!(agotado_el(30.0, R, 3.5 * D, None), None);
    assert_eq!(agotado_el(5.0, R, 0.1 * D, None), None);
    assert_eq!(agotado_el(0.0, R, D, None), None);
}

#[test]
fn dias_y_niveles() {
    let lun = Local.with_ymd_and_hms(2026, 11, 2, 12, 0, 0).unwrap().timestamp() as f64;
    let n: Vec<String> = [0.0, 1.0, 2.0, 6.0].iter().map(|k| nombre_dia(lun + k * D, lun)).collect();
    assert_eq!(n, ["hoy", "mañana", "el miércoles", "el domingo"]);
    assert_eq!(nivel_barra(10.0, 20.0, 0.0), (Nivel::Bien, 0.0));
    assert_eq!(nivel_barra(30.0, 20.0, 0.0), (Nivel::Encima, 0.0));
    assert_eq!(nivel_barra(10.0, 20.0, 1.0).0, Nivel::Bien); // por debajo del ritmo no hay riesgo
    assert_eq!(nivel_barra(30.0, 20.0, 1.0), (Nivel::Riesgo, 1.0));
}

#[test]
fn renovar() {
    let (r, h) = (1_794_668_400.0, 1_794_679_708.0);
    let u = Uso {
        semanal: Some(ven(97.0, r)),
        cinco_horas: Some(ven(50.0, r - 10.0)),
        ts: 0.0,
    };
    let c = u.clone();
    assert_eq!(renovar_ventanas(&u, h).semanal, Some(ven(0.0, r + R)));
    assert_eq!(u, c); // no toca la entrada
    assert_eq!(renovar_ventanas(&u, h).cinco_horas, c.cinco_horas);
    assert_eq!(renovar_ventanas(&u, r + 2.5 * R).semanal, Some(ven(0.0, r + 3.0 * R)));
    assert_eq!(renovar_ventanas(&u, r).semanal, Some(ven(0.0, r + R)));
    assert_eq!(renovar_ventanas(&u, r - 1.0), c);
    let sin_reset = Uso {
        semanal: Some(Ventana { pct: 5.0, resets_at: None }),
        ..Default::default()
    };
    assert_eq!(renovar_ventanas(&sin_reset, h), sin_reset);
    assert_eq!(renovar_ventanas(&Uso::default(), h), Uso::default());
}

#[test]
fn avisos() {
    let (ahora, r5) = (1000.0, 10_000.0);
    let mut uso = Uso {
        cinco_horas: Some(ven(85.0, r5)),
        semanal: Some(ven(10.0, 1000.0 + 400_000.0)),
        ts: 0.0,
    };
    let clave = |u| ("fiveHour".to_string(), r5 as i64, u);
    let av = avisos_nuevos(&uso, &HashSet::new(), ahora);
    assert_eq!(av.len(), 1);
    assert_eq!(av[0].claves, vec![clave(80)]);
    assert!(av[0].texto.contains("Sesión 5 h al 85 %") && av[0].texto.contains("te queda un 15 %"));
    let hechos: HashSet<_> = [clave(80)].into();
    assert!(avisos_nuevos(&uso, &hechos, ahora).is_empty());
    uso.cinco_horas = Some(ven(100.0, r5));
    let av = avisos_nuevos(&uso, &hechos, ahora);
    assert_eq!(av[0].claves, vec![clave(100)]);
    assert!(av[0].texto.contains("límite"));
    let av = avisos_nuevos(&uso, &HashSet::new(), ahora);
    assert_eq!(av[0].claves, vec![clave(80), clave(100)]);
    assert!(avisos_nuevos(&Uso::default(), &HashSet::new(), ahora).is_empty());
    // ya renovada: no hay dato
    let vieja = Uso { cinco_horas: Some(ven(99.0, 500.0)), ..Default::default() };
    assert!(avisos_nuevos(&vieja, &HashSet::new(), ahora).is_empty());
}

#[test]
fn apuntar_y_guardar() {
    let d = carpeta("apuntar");
    let ruta = d.join("h.json");
    let mut h = Vec::new();
    assert!(apuntar(&mut h, Some(&ven(5.0, 2e9)), 1000.0, 10.0));
    assert_eq!(h, vec![(1000.0, 2e9, 5.0)]);
    // sin cambio de reset ni de %: no se apunta; con cambio, sí
    assert!(!apuntar(&mut h, Some(&ven(5.0, 2e9)), 1005.0, 10.0));
    assert!(apuntar(&mut h, Some(&ven(6.0, 2e9)), 1005.0, 10.0));
    // una ventana vencida o sin dato no se apunta
    assert!(!apuntar(&mut h, Some(&ven(7.0, 900.0)), 1010.0, 10.0));
    assert!(!apuntar(&mut h, None, 1010.0, 10.0));
    // la retención descarta lo viejo
    assert!(apuntar(&mut h, Some(&ven(9.0, 2e9)), 1010.0, 7.0));
    assert_eq!(h, vec![(1005.0, 2e9, 6.0), (1010.0, 2e9, 9.0)]);
    let mut h = vec![(1000.0, 2e9, 5.0)];
    guardar_hist(&ruta, &h).unwrap();
    assert_eq!(cargar_hist(&ruta), h);
    // sin .tmp sobrante, y en el mismo formato de lista [t, reset, pct]
    assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "[[1000,2000000000,5]]");
    h.push((1010.5, 2e9, 6.5));
    guardar_hist(&ruta, &h).unwrap();
    assert_eq!(cargar_hist(&ruta), h);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn ficheros_rotos() {
    let d = carpeta("rotos");
    let (u, h) = (d.join("usage.json"), d.join("h.json"));
    // ausente o roto: sin nada
    assert_eq!(leer_uso(&u), Uso::default());
    assert!(cargar_hist(&h).is_empty());
    std::fs::write(&u, "{no es json").unwrap();
    std::fs::write(&h, "[1,2").unwrap();
    assert_eq!(leer_uso(&u), Uso::default());
    assert!(cargar_hist(&h).is_empty());
    // con BOM, ventanas que no son objetos descartadas
    std::fs::write(&u, "\u{feff}{\"fiveHour\":5,\"sevenDay\":{\"pct\":44,\"resetsAt\":1795212000},\"ts\":1795098899}").unwrap();
    let uso = leer_uso(&u);
    assert_eq!(uso.cinco_horas, None);
    assert_eq!(uso.semanal, Some(ven(44.0, 1_795_212_000.0)));
    assert_eq!(uso.ts, 1_795_098_899.0);
    // historial: se descartan las muestras mal formadas
    std::fs::write(&h, "[[1,2,3],[1,2],\"x\",[1,2,\"3\"],[true,2,3],[4,5,6]]").unwrap();
    assert_eq!(cargar_hist(&h), vec![(1.0, 2.0, 3.0), (4.0, 5.0, 6.0)]);
    std::fs::write(&h, "{\"a\":1}").unwrap();
    assert!(cargar_hist(&h).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn estado_completo() {
    // ejemplo de usage.json de Consumo
    let (ts, r5, r7) = (1_795_098_899.0, 1_795_104_600.0, 1_795_212_000.0);
    let uso = Uso {
        cinco_horas: Some(ven(8.0, r5)),
        semanal: Some(ven(44.0, r7)),
        ts,
    };
    let e = calcular_estado(&uso, &[], &[], ts + 60.0);
    assert!(!e.sin_datos);
    assert_eq!(e.antiguedad, Some(60));
    assert_eq!(e.semanal.pct, 44.0);
    assert_eq!(e.semanal.segundos_hasta_reset, Some((r7 - ts - 60.0) as i64));
    assert!(e.semanal.texto_ritmo.as_deref().unwrap().starts_with("hoy gastarás ~"));
    assert!(e.semanal.texto_semana.as_deref().unwrap().starts_with("acabas en ~"));
    assert!(e.cinco_horas.texto_ritmo.as_deref().unwrap().starts_with("a este ritmo acabas en "));
    // datos de más de 6 h, o sin ventanas: «sin datos»
    assert!(calcular_estado(&uso, &[], &[], ts + 6.0 * 3600.0 + 1.0).sin_datos);
    assert!(calcular_estado(&Uso::default(), &[], &[], ts).sin_datos);
    // el JSON de salida usa camelCase
    let j = serde_json::to_value(&e).unwrap();
    assert!(j.get("sinDatos").is_some() && j.get("cincoHoras").is_some());
    assert_eq!(j["semanal"]["nivel"].as_str().map(|s| s.is_empty()), Some(false));
    assert!(j["semanal"].get("segundosHastaReset").is_some());
}

#[test]
fn origenes_de_vista_se_suman_sin_apagarse_entre_si() {
    use super::Origenes;
    let mut o = Origenes::default();
    assert!(!o.alguno());
    assert!(o.fijar("pestana", true));
    assert!(o.fijar("ventana", true));
    // Una se oculta: la otra mantiene el trabajo
    assert!(o.fijar("pestana", false));
    assert!(o.alguno());
    assert!(o.fijar("ventana", false));
    assert!(!o.alguno());
    // Repetir no acumula y un origen desconocido se rechaza
    assert!(o.fijar("pestana", true) && o.fijar("pestana", true));
    assert!(o.fijar("pestana", false));
    assert!(!o.alguno());
    assert!(!o.fijar("otra", true));
    assert!(!o.alguno());
}
