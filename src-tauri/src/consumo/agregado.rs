//! Periodos y agregación de los archivos semanales (port de `periodo`, `agregar`, `_sumar`,
//! `_o_validos`, `primer_salto` y `_salta` de `informe.py`). Lo usan el Histórico (`historico.rs`)
//! y los informes (encargos 8b/8c). Todo son funciones puras: no tocan disco ni reloj.
//!
//! Diferencia con Consumo: los diccionarios son ordenados por clave (en Python, por orden de
//! inserción); solo cambia el orden de los empates al ordenar.

// Parte de la API la usarán los informes (encargos 8b/8c).
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, Duration, NaiveDate};

use super::archivo::{fecha_de, fecha_iso, iso, Archivo};
use super::limites::SEMANA;

pub const DIAS: [&str; 7] = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"];
pub const MESES: [&str; 12] = [
    "enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre",
];

/// [%, tokens, $, peso].
pub type Acc = [f64; 4];

pub fn capitalizar(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn ordinal(n: i32) -> &'static str {
    match n {
        1 => "1.er",
        2 => "2.º",
        3 => "3.er",
        _ => "4.º",
    }
}

pub fn fecha_larga(d: NaiveDate, con_anio: bool) -> String {
    let base = format!("{} de {}", d.day(), MESES[d.month0() as usize]);
    if con_anio {
        format!("{base} de {}", d.year())
    } else {
        base
    }
}

fn ymd(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap_or_default()
}

/// (desde, hasta, título): las fechas [desde, hasta) del periodo que contiene `ahora` (o el
/// anterior). La semana es la de Claude (de reset a reset: hace falta `reset`; sin él, los 7 días
/// que acaban hoy); los demás, de calendario. Err con un tipo desconocido.
pub fn periodo(tipo: &str, ahora: f64, reset: Option<f64>, anterior: bool) -> Result<(NaiveDate, NaiveDate, String), String> {
    let hoy = fecha_de(ahora);
    let ant = i32::from(anterior);
    match tipo {
        "semana" => {
            let (desde, ultimo) = match reset.filter(|r| *r != 0.0) {
                Some(r) => {
                    let ini = r - SEMANA * f64::from(1 + ant);
                    (fecha_de(ini), fecha_de(ini + SEMANA - 1.0))
                }
                None => {
                    let desde = hoy - Duration::days(6 + 7 * i64::from(ant));
                    (desde, desde + Duration::days(6))
                }
            };
            let t = format!("Semana del {} al {}", fecha_larga(desde, false), fecha_larga(ultimo, true));
            Ok((desde, ultimo + Duration::days(1), t))
        }
        "mes" => {
            let (mut a, mut m) = (hoy.year(), hoy.month() as i32 - ant);
            if m < 1 {
                a -= 1;
                m = 12;
            }
            let desde = ymd(a, m as u32, 1);
            let hasta = ymd(a + i32::from(m == 12), (m % 12 + 1) as u32, 1);
            Ok((desde, hasta, format!("{} {a}", capitalizar(MESES[m as usize - 1]))))
        }
        "trimestre" => {
            let (mut a, mut q) = (hoy.year(), (hoy.month() as i32 - 1) / 3 - ant);
            if q < 0 {
                a -= 1;
                q = 3;
            }
            Ok((
                ymd(a, (3 * q + 1) as u32, 1),
                ymd(a + i32::from(q == 3), ((3 * q + 3) % 12 + 1) as u32, 1),
                format!("{} trimestre {a}", ordinal(q + 1)),
            ))
        }
        "semestre" => {
            let (mut a, mut h) = (hoy.year(), (hoy.month() as i32 - 1) / 6 - ant);
            if h < 0 {
                a -= 1;
                h = 1;
            }
            Ok((ymd(a, (6 * h + 1) as u32, 1), ymd(a + h, if h == 1 { 1 } else { 7 }, 1), format!("{} semestre {a}", ordinal(h + 1))))
        }
        "anio" => {
            let a = hoy.year() - ant;
            Ok((ymd(a, 1, 1), ymd(a + 1, 1, 1), format!("Año {a}")))
        }
        _ => Err(tipo.to_string()),
    }
}

/// Nombre del fichero de un informe: «Consumo - resumen semanal 2026-10-31.html».
pub fn nombre_informe(tipo: &str, desde: NaiveDate) -> String {
    let t = match tipo {
        "semana" => "semanal",
        "mes" => "mensual",
        "trimestre" => "trimestral",
        "semestre" => "semestral",
        _ => "anual",
    };
    format!("Consumo - resumen {t} {}.html", iso(desde))
}

// ---------- agregación ----------

pub fn sumar(dic: &mut BTreeMap<String, Acc>, k: &str, p: f64, tok: f64, coste: f64, peso: f64) {
    let a = dic.entry(k.to_string()).or_insert([0.0; 4]);
    a[0] += p;
    a[1] += tok;
    a[2] += coste;
    a[3] += peso;
}

/// Los índices (en `archivos`) de los archivos cuyo `o` (sin atribuir) cuenta: por semana (inicio
/// ±600 s) solo los del método más nuevo (`rep` máximo de esa semana; sin `rep` = 1). Un archivo
/// viejo de un equipo que casi no se abre no impone para siempre un `o` mal repartido. Lo demás de
/// esos archivos sí cuenta.
pub fn o_validos<A: AsRef<Archivo>>(archivos: &[A]) -> Vec<bool> {
    let mut orden: Vec<usize> = (0..archivos.len()).collect();
    orden.sort_by(|a, b| archivos[*a].as_ref().inicio.total_cmp(&archivos[*b].as_ref().inicio));
    let mut claves: Vec<f64> = Vec::new();
    let mut por: Vec<(usize, i64, usize)> = Vec::new(); // (clave, rep, índice)
    for i in orden {
        let a = archivos[i].as_ref();
        let c = match claves.iter().position(|c| (a.inicio - c).abs() <= 600.0) {
            Some(c) => c,
            None => {
                claves.push(a.inicio);
                claves.len() - 1
            }
        };
        por.push((c, a.rep, i));
    }
    let mut mx: HashMap<usize, i64> = HashMap::new();
    for (c, r, _) in &por {
        let e = mx.entry(*c).or_insert(1);
        *e = (*e).max(*r);
    }
    let mut ok = vec![false; archivos.len()];
    for (c, r, i) in por {
        ok[i] = r == mx[&c];
    }
    ok
}

/// El primer salto de toda la historia de la app: el primer arranque a mitad de semana vuelca en la
/// primera muestra el % ya acumulado, y eso sale como «sin atribuir» aunque no lo sea. Devuelve
/// (inicio de la semana, día `AAAA-MM-DD`, hora) de la (semana, día, hora) más antigua de todo el
/// conjunto con `o` > 0, solo si en esa semana no hay antes ninguna hora con `h` > 0 ni `o` > 0
/// (en ningún equipo); si no, None. Se calcula al leer: no se reescribe ningún archivo.
pub fn primer_salto<A: AsRef<Archivo>>(archivos: &[A]) -> Option<(f64, String, usize)> {
    let validos = o_validos(archivos);
    let mut orden: Vec<usize> = (0..archivos.len()).collect();
    orden.sort_by(|a, b| archivos[*a].as_ref().inicio.total_cmp(&archivos[*b].as_ref().inicio));
    let mut claves: Vec<f64> = Vec::new();
    // (clave de semana, día, hora, tipo): tipo 0 = o > 0, 1 = h > 0
    let mut ev: Vec<(f64, &str, usize, u8)> = Vec::new();
    for i in orden {
        let a = archivos[i].as_ref();
        let clave = match claves.iter().find(|c| (a.inicio - **c).abs() <= 600.0) {
            Some(c) => *c,
            None => {
                claves.push(a.inicio);
                a.inicio
            }
        };
        for (d, dia) in &a.dias {
            for h in 0..24 {
                if dia.o[h] > 0.0 && validos[i] {
                    ev.push((clave, d, h, 0));
                }
                if dia.h[h] > 0.0 {
                    ev.push((clave, d, h, 1));
                }
            }
        }
    }
    let primero = ev
        .iter()
        .filter(|e| e.3 == 0)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(b.1)).then(a.2.cmp(&b.2)))?;
    let (c, d, h) = (primero.0, primero.1, primero.2);
    if ev.iter().any(|e| e.0 == c && (e.1, e.2) < (d, h)) {
        return None;
    }
    Some((c, d.to_string(), h))
}

/// True si el `o` de (semana que empieza en `inicio`, día, hora) es el primer salto y no debe contar.
pub fn salta(salto: &Option<(f64, String, usize)>, inicio: f64, d: &str, h: usize) -> bool {
    salto.as_ref().is_some_and(|s| d == s.1 && h == s.2 && (inicio - s.0).abs() <= 600.0)
}

/// Una sesión del resumen: sus totales, los días, modelos y equipos en que aparece y sus tareas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SesionAgr {
    pub a: Acc,
    pub dias: BTreeSet<String>,
    pub modelos: BTreeSet<String>,
    pub equipos: BTreeSet<String>,
    pub tareas: BTreeMap<String, f64>,
}

/// Lo que devuelve `agregar`. Cada entrada es [%, tokens, $, peso].
#[derive(Clone, Debug, PartialEq)]
pub struct Agregado {
    pub desde: NaiveDate,
    pub hasta: NaiveDate,
    pub dias: BTreeMap<String, Acc>,
    pub sin_atribuir: BTreeMap<String, f64>,
    pub dia_proy: BTreeMap<String, BTreeMap<String, Acc>>,
    pub horas_dia: BTreeMap<String, Vec<f64>>,
    pub horas_proy: BTreeMap<String, BTreeMap<String, Vec<f64>>>,
    pub otros_hora: BTreeMap<String, Vec<f64>>,
    /// [tokens leídos de caché, $ que ahorra, y tokens y $ de los días que traen ese dato].
    pub cache: [f64; 4],
    pub proyectos: BTreeMap<String, Acc>,
    pub modelos: BTreeMap<String, Acc>,
    pub tareas: BTreeMap<String, Acc>,
    pub equipos: BTreeMap<String, Acc>,
    pub sesiones: BTreeMap<(String, String), SesionAgr>,
    pub total: Acc,
    /// (inicio de la semana, %, tokens, $).
    pub semanas: Vec<(String, f64, f64, f64)>,
    pub maquinas: Vec<String>,
}

/// Suma los días de [desde, hasta) de todos los archivos. Cada equipo guarda lo suyo, así que se
/// suma; lo no atribuido lo calcula igual cada equipo, y entre equipos se toma el mayor (no se
/// cuenta dos veces). Con `semana` (el inicio de una semana de Claude) solo cuentan los archivos
/// de esa semana, enteros: el día del reset pertenece a dos semanas y por fechas se mezclarían.
pub fn agregar<A: AsRef<Archivo>>(archivos: &[A], desde: NaiveDate, hasta: NaiveDate, semana: Option<f64>) -> Agregado {
    let mut r = Agregado {
        desde,
        hasta,
        dias: BTreeMap::new(),
        sin_atribuir: BTreeMap::new(),
        dia_proy: BTreeMap::new(),
        horas_dia: BTreeMap::new(),
        horas_proy: BTreeMap::new(),
        otros_hora: BTreeMap::new(),
        cache: [0.0; 4],
        proyectos: BTreeMap::new(),
        modelos: BTreeMap::new(),
        tareas: BTreeMap::new(),
        equipos: BTreeMap::new(),
        sesiones: BTreeMap::new(),
        total: [0.0; 4],
        semanas: Vec::new(),
        maquinas: Vec::new(),
    };
    let mut maquinas: BTreeSet<String> = BTreeSet::new();
    let mut semana_de: HashMap<String, String> = HashMap::new();
    let salto = primer_salto(archivos);
    let validos = o_validos(archivos);
    for (i, a) in archivos.iter().enumerate() {
        let a = a.as_ref();
        if semana.is_some_and(|s| (a.inicio - s).abs() > 600.0) {
            continue;
        }
        for (d, dia) in &a.dias {
            if semana.is_none() {
                let f = fecha_iso(d).unwrap_or_default();
                if !(desde <= f && f < hasta) {
                    continue;
                }
            }
            maquinas.insert(a.maquina.clone());
            if let Some(k) = dia.k {
                r.cache[0] += k[0];
                r.cache[1] += k[1];
                r.cache[2] += dia.f.iter().map(|f| f.tokens).sum::<f64>();
                r.cache[3] += dia.f.iter().map(|f| f.coste).sum::<f64>();
            }
            semana_de.entry(d.clone()).or_insert_with(|| iso(fecha_de(a.inicio)));
            let hd = r.horas_dia.entry(d.clone()).or_insert_with(|| vec![0.0; 24]);
            for h in 0..24 {
                hd[h] += dia.t[h];
            }
            for (proy, arr) in &dia.hp {
                let hp = r.horas_proy.entry(d.clone()).or_default().entry(proy.clone()).or_insert_with(|| vec![0.0; 24]);
                for h in 0..24 {
                    hp[h] += arr[h];
                }
            }
            let od = r.otros_hora.entry(d.clone()).or_insert_with(|| vec![0.0; 24]);
            for h in 0..24 {
                if validos[i] && !salta(&salto, a.inicio, d, h) {
                    od[h] = od[h].max(dia.o[h]);
                }
            }
            for f in &dia.f {
                let (p, tok, coste, peso) = (f.pct, f.tokens, f.coste, f.peso);
                sumar(&mut r.dias, d, p, tok, coste, peso);
                sumar(r.dia_proy.entry(d.clone()).or_default(), &f.proyecto, p, tok, coste, peso);
                sumar(&mut r.proyectos, &f.proyecto, p, tok, coste, peso);
                sumar(&mut r.modelos, &f.modelo, p, tok, coste, peso);
                sumar(&mut r.tareas, &f.tarea, p, tok, coste, peso);
                sumar(&mut r.equipos, &f.equipo, p, tok, coste, peso);
                let s = r.sesiones.entry((f.proyecto.clone(), f.sesion.clone())).or_default();
                s.a[0] += p;
                s.a[1] += tok;
                s.a[2] += coste;
                s.a[3] += peso;
                s.dias.insert(d.clone());
                s.modelos.insert(f.modelo.clone());
                s.equipos.insert(f.equipo.clone());
                *s.tareas.entry(f.tarea.clone()).or_insert(0.0) += p;
            }
        }
    }
    for d in r.otros_hora.keys() {
        r.dias.entry(d.clone()).or_insert([0.0; 4]);
    }
    r.sin_atribuir = r.otros_hora.iter().map(|(d, arr)| (d.clone(), arr.iter().sum())).collect();
    let suma_sin: f64 = r.sin_atribuir.values().sum();
    r.total = [
        r.dias.values().map(|v| v[0]).sum::<f64>() + suma_sin,
        r.dias.values().map(|v| v[1]).sum(),
        r.dias.values().map(|v| v[2]).sum(),
        r.dias.values().map(|v| v[3]).sum(),
    ];
    let mut semanas: BTreeMap<String, [f64; 3]> = BTreeMap::new();
    for (d, v) in &r.dias {
        let k = semana_de.get(d).cloned().unwrap_or_else(|| d.clone());
        let s = semanas.entry(k).or_insert([0.0; 3]);
        s[0] += v[0] + r.sin_atribuir.get(d).copied().unwrap_or(0.0);
        s[1] += v[1];
        s[2] += v[2];
    }
    r.semanas = semanas.into_iter().map(|(k, v)| (k, v[0], v[1], v[2])).collect();
    r.maquinas = maquinas.into_iter().collect();
    r
}
