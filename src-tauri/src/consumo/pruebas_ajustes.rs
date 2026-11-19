//! Pruebas de los ajustes de Consumo y de los avisos del 80 % / 100 % (carpetas temporales,
//! nunca ~/.claude).

use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::{json, Value};

use super::ajustes::*;
use super::equipos::{carpetas_sync, guardar_carpetas};
use super::limites::{Uso, Ventana};

fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("netrunner_consumo_aj_{nombre}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn leer(ruta: &PathBuf) -> Value {
    serde_json::from_str(&std::fs::read_to_string(ruta).unwrap()).unwrap()
}

#[test]
fn guardar_ajustes_conserva_claves_ajenas() {
    let d = carpeta("ajustes");
    let f = d.join("consumo_ajustes.json");
    std::fs::write(&f, r#"{"zoom":1.5,"tema":"oscuro","partes":["anillos"],"ctx_max":1000000,"avisos":true,"gb_max":0}"#).unwrap();
    assert!(guardar_ajustes(&f, 500_000, false, 12.345).unwrap());
    let v = leer(&f);
    assert_eq!(v["ctx_max"], 500_000);
    assert_eq!(v["avisos"], false);
    assert_eq!(v["gb_max"], json!(12.35));
    assert_eq!(v["zoom"], json!(1.5));
    assert_eq!(v["tema"], "oscuro");
    assert_eq!(v["partes"], json!(["anillos"]));
    assert!(!d.join("consumo_ajustes.json.tmp").exists());
    assert_eq!(leer_ajustes(&f), (500_000, false, 12.35));
    // sin cambios: no se reescribe
    assert!(!guardar_ajustes(&f, 500_000, false, 12.35).unwrap());
    // un fichero que no es un objeto no se pisa
    std::fs::write(&f, "no es json").unwrap();
    assert!(guardar_ajustes(&f, 200_000, true, 0.0).is_err());
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "no es json");
    // sin fichero: se crea con valores por defecto al leer
    let n = d.join("nuevo.json");
    assert_eq!(leer_ajustes(&n), (1_000_000, true, 0.0));
    assert!(guardar_ajustes(&n, 200_000, true, 0.0).unwrap());
    assert_eq!(leer_ajustes(&n), (200_000, true, 0.0));
}

#[test]
fn consumo_sync_sin_duplicados_y_principal_primero() {
    let d = carpeta("sync");
    let f = d.join("consumo_sync");
    let od = d.join("OneDrive");
    let lista = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(guardar_carpetas(&f, &lista(&["D:/Uno", "  D:/Dos ", "d:/uno/", "", "D:/Tres"])).unwrap());
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "D:/Uno\nD:/Dos\nD:/Tres");
    let r = carpetas_sync(&f, &od);
    assert_eq!(r, vec![PathBuf::from("D:/Uno"), PathBuf::from("D:/Dos"), PathBuf::from("D:/Tres")]);
    // sin cambios: no escribe
    assert!(!guardar_carpetas(&f, &lista(&["D:/Uno", "D:/Dos", "D:/Tres"])).unwrap());
    // otra principal
    assert!(guardar_carpetas(&f, &lista(&["D:/Dos", "D:/Uno"])).unwrap());
    assert_eq!(carpetas_sync(&f, &od)[0], PathBuf::from("D:/Dos"));
    // vacía: vuelve la de por defecto
    assert!(guardar_carpetas(&f, &[]).unwrap());
    assert!(!f.exists());
    assert_eq!(carpetas_sync(&f, &od), vec![od.join("Claude-consumo")]);
}

fn uso(pct: f64, reset: f64) -> Uso {
    Uso { cinco_horas: Some(Ventana { pct, resets_at: Some(reset) }), semanal: None, ts: 0.0 }
}

#[test]
fn hechos_con_ventanas_renovadas() {
    let d = carpeta("hechos");
    let f = d.join("consumo_avisos.json");
    let (ahora, r5) = (1000.0, 10_000.0);
    std::fs::write(&f, r#"[["fiveHour",10000,80],["fiveHour",500,100],["sevenDay",700,80]]"#).unwrap();
    let h = cargar_hechos(&f, ahora);
    assert_eq!(h, HashSet::from([("fiveHour".to_string(), 10_000, 80)]));
    assert!(cargar_hechos(&d.join("no.json"), ahora).is_empty());

    let mut est = Hechos::default();
    // el 80 ya estaba hecho: al llegar al 100 solo sale ese
    let av = procesar_avisos(&mut est, &f, &uso(100.0, r5), ahora, true, || true);
    assert_eq!(av.len(), 1);
    assert_eq!(av[0].claves, vec![("fiveHour".to_string(), r5 as i64, 100)]);
    let en_disco = leer(&f);
    assert!(en_disco.as_array().unwrap().contains(&json!(["fiveHour", 10000, 100])));
    // ya dado: no se repite
    assert!(procesar_avisos(&mut est, &f, &uso(100.0, r5), ahora, true, || true).is_empty());
    // ventana renovada (otro reset): vuelve a avisar
    let av = procesar_avisos(&mut est, &f, &uso(85.0, r5 + 18_000.0), ahora, true, || true);
    assert_eq!(av.len(), 1);
}

#[test]
fn avisos_desactivados_se_dan_por_hechos_sin_mostrar() {
    let d = carpeta("desact");
    let f = d.join("consumo_avisos.json");
    let (ahora, r5) = (1000.0, 10_000.0);
    let mut est = Hechos::default();
    assert!(procesar_avisos(&mut est, &f, &uso(90.0, r5), ahora, false, || true).is_empty());
    assert!(f.exists()); // quedó apuntado
    // al reactivarlos no sale de golpe
    assert!(procesar_avisos(&mut est, &f, &uso(90.0, r5), ahora, true, || true).is_empty());
    // pero el 100 sí es nuevo
    assert_eq!(procesar_avisos(&mut est, &f, &uso(100.0, r5), ahora, true, || true).len(), 1);
}

#[test]
fn con_consumo_abierto_no_avisa_ni_escribe() {
    let d = carpeta("abierto");
    let f = d.join("consumo_avisos.json");
    let mut est = Hechos::default();
    assert!(procesar_avisos(&mut est, &f, &uso(90.0, 10_000.0), 1000.0, true, || false).is_empty());
    assert!(!f.exists());
    assert!(est.hechos.is_empty());
    // sin nada nuevo no se pregunta por el proceso
    let _ = procesar_avisos(&mut est, &f, &uso(10.0, 10_000.0), 1000.0, true, || panic!("no debe preguntar"));
}

#[test]
fn vista_historico_se_guarda_y_conserva_lo_ajeno() {
    let d = carpeta("vista");
    let f = d.join("consumo_ajustes.json");
    // sin fichero: la vista de la primera vez
    let v0 = leer_vista_historico(&f);
    assert_eq!((v0.grupo.as_str(), v0.unidad.as_str(), v0.periodo.as_str(), v0.paso.as_str()), ("proyecto", "eur", "todo", "auto"));
    // con un fichero de Consumo: se conservan las claves ajenas y las casillas de dentro de la vista
    std::fs::write(&f, r#"{"zoom":1.5,"suscripcion_eur":20,"historico_vista":{"grupo":"equipo","sel":{"equipo":["A"]},"geom":"1100x760+20+5"}}"#).unwrap();
    assert!(guardar_vista_historico(&f, &json!({"periodo": "mes", "unidad": "tokens", "paso": "semana", "raro": 1})).unwrap());
    let v = leer(&f);
    assert_eq!(v["zoom"], json!(1.5));
    assert_eq!(v["suscripcion_eur"], json!(20));
    assert_eq!(v["historico_vista"]["grupo"], "equipo");
    assert_eq!(v["historico_vista"]["periodo"], "mes");
    assert_eq!(v["historico_vista"]["unidad"], "tokens");
    assert_eq!(v["historico_vista"]["paso"], "semana");
    assert_eq!(v["historico_vista"]["sel"], json!({"equipo": ["A"]}));
    assert_eq!(v["historico_vista"]["geom"], "1100x760+20+5");
    assert!(v["historico_vista"].get("raro").is_none());
    let l = leer_vista_historico(&f);
    assert_eq!((l.periodo.as_str(), l.unidad.as_str(), l.modo.as_str()), ("mes", "tokens", "apilado"));
    // sin cambios no se reescribe (ni deja temporales)
    assert!(!guardar_vista_historico(&f, &json!({"periodo": "mes", "unidad": "tokens"})).unwrap());
    assert!(!d.join("consumo_ajustes.json.tmp").exists());
    // un valor malo no pisa lo bueno: vale lo de la primera vez para esa clave
    assert!(guardar_vista_historico(&f, &json!({"modo": "lineas"})).unwrap());
    assert_eq!(leer_vista_historico(&f).modo, "lineas");
    guardar_vista_historico(&f, &json!({"unidad": "nada"})).unwrap();
    assert_eq!(leer_vista_historico(&f).unidad, "eur");
    // fichero que no es un objeto: no se pisa
    std::fs::write(&f, "no es json").unwrap();
    assert!(guardar_vista_historico(&f, &json!({"modo": "lineas"})).is_err());
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "no es json");
    // sin fichero se crea
    let n = d.join("nuevo.json");
    assert!(guardar_vista_historico(&n, &json!({"grupo": "modelo"})).unwrap());
    assert_eq!(leer_vista_historico(&n).grupo, "modelo");
}
