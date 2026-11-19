//! Histórico: la suma de todo el archivo semanal por proyecto, equipo, modelo y tarea, en €
//! (coste real: el % del límite semanal sobre lo que cuesta esa semana), días de límite, dólares a
//! precio de API, tokens y tiempo activo; con periodos, series por día / semana / mes y unidades
//! por hora. Port de la parte de histórico de `informe.py` (`agregar_historico`, `series_historico`,
//! `periodo_historico`, `limpiar_vista_historico`...). Todo son funciones puras salvo el comando,
//! que lee los ficheros con caché y calcula solo cuando la interfaz lo pide.

// Parte de la API (formatos de eje, HTML) la usarán los informes (encargos 8b/8c).
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, Duration, NaiveDate};

use super::agregado::{capitalizar, fecha_larga, o_validos, periodo, primer_salto, salta, Acc, DIAS, MESES};
use super::archivo::{fecha_de, fecha_iso, iso, nombre_tarea, Archivo};
use super::limites::SEMANA;
use super::reparto::OTROS;

/// € al mes de la suscripción por defecto (ajuste `suscripcion_eur`).
pub const PRECIO_EUR: f64 = 18.0;
pub const UNIDADES: [(&str, &str); 8] = [
    ("eur", "Coste real (€)"),
    ("dias", "Días de límite"),
    ("usd", "Dólares"),
    ("tokens", "Tokens"),
    ("horas", "Tiempo activo (h)"),
    ("eur_h", "€ por hora activa"),
    ("pct_h", "% del límite por hora"),
    ("tok_h", "Tokens por hora"),
];
/// Las que se suman.
pub const UNIDADES_BASE: [&str; 4] = ["eur", "dias", "usd", "tokens"];
pub const GRUPOS: [(&str, &str); 4] = [("proyecto", "Proyecto"), ("equipo", "Equipo"), ("modelo", "Modelo"), ("tarea", "Tipo de tarea")];
/// Lo no atribuido solo existe como % del límite: en $, tokens y horas no hay (0).
pub const SIN_ATRIBUIR: [&str; 2] = ["eur", "dias"];
pub const PERIODOS_HIST: [(&str, &str); 7] = [
    ("semana", "Esta semana"),
    ("semana_pasada", "Semana pasada"),
    ("mes", "Este mes"),
    ("mes_pasado", "Mes pasado"),
    ("3meses", "3 meses"),
    ("anio", "Año"),
    ("todo", "Todo"),
];
pub const MODOS_HIST: [&str; 2] = ["lineas", "apilado"];
pub const PASOS: [&str; 3] = ["dia", "semana", "mes"];

/// Unidades que son un cociente valor / horas activas: el numerador.
fn razon(u: &str) -> Option<&'static str> {
    match u {
        "eur_h" => Some("eur"),
        "pct_h" => Some("pct"),
        "tok_h" => Some("tokens"),
        _ => None,
    }
}

/// Lo que cuesta el 100 % del límite semanal: un mes son 52/12 semanas (18 €/mes dan 4,15 € a la semana).
pub fn eur_semana(precio_mes_eur: f64) -> f64 {
    precio_mes_eur * 12.0 / 52.0
}

/// El valor de una entrada en la unidad `u`: € de coste real, días de límite (un día = 100 % / 7),
/// dólares, tokens u horas de tiempo activo. Las de cociente son valor / horas: con 0 s valen None
/// (no hay cifra). Un cociente NO se suma ni se acumula sumando cocientes: se suman %, tokens y
/// segundos y se convierte el total. La conversión de unidades vive solo aquí.
pub fn valor_u(u: &str, pct: f64, tok: f64, usd: f64, precio_mes_eur: f64, seg: f64) -> Option<f64> {
    if let Some(n) = razon(u) {
        if seg <= 0.0 {
            return None;
        }
        return valor_u(n, pct, tok, usd, precio_mes_eur, 0.0).map(|v| v / (seg / 3600.0));
    }
    Some(match u {
        "pct" => pct,
        "eur" => pct / 100.0 * eur_semana(precio_mes_eur),
        "horas" => seg / 3600.0,
        "dias" => pct * 7.0 / 100.0,
        "usd" => usd,
        _ => tok,
    })
}

/// Entre cuánto se divide el valor para dibujarlo: los tokens (y los tokens por hora) en millones.
pub fn escala_u(u: &str) -> f64 {
    if u == "tokens" || u == "tok_h" {
        1e-6
    } else {
        1.0
    }
}

/// La unidad con la que se reparte el «% del total» de `u`: la misma, o el numerador si es un cociente.
pub fn unidad_parte(u: &str) -> &str {
    match razon(u) {
        None => u,
        Some("pct") => "dias",
        Some(n) => n,
    }
}

// ---------- textos a la española ----------

/// Número a la española: 1.234,5.
pub fn num(x: f64, dec: usize) -> String {
    let s = format!("{:.*}", dec, x.abs());
    let (ent, frac) = match s.split_once('.') {
        Some((e, f)) => (e.to_string(), Some(f.to_string())),
        None => (s, None),
    };
    let mut miles = String::new();
    for (i, c) in ent.chars().enumerate() {
        if i > 0 && (ent.len() - i) % 3 == 0 {
            miles.push('.');
        }
        miles.push(c);
    }
    let signo = if x < 0.0 && s_no_cero(&miles, frac.as_deref()) { "-" } else { "" };
    match frac {
        Some(f) => format!("{signo}{miles},{f}"),
        None => format!("{signo}{miles}"),
    }
}

fn s_no_cero(ent: &str, frac: Option<&str>) -> bool {
    ent.chars().any(|c| c.is_ascii_digit() && c != '0') || frac.is_some_and(|f| f.chars().any(|c| c != '0'))
}

pub fn tokens_txt(n: f64) -> String {
    if n >= 1e9 {
        format!("{} G", num(n / 1e9, 2))
    } else if n >= 1e6 {
        format!("{} M", num(n / 1e6, 1))
    } else if n >= 1e3 {
        format!("{} k", num(n / 1e3, 0))
    } else {
        format!("{}", n as i64)
    }
}

pub fn texto_u(u: &str, v: Option<f64>) -> String {
    let Some(v) = v else {
        return "—".into();
    };
    match u {
        "eur_h" => format!("{} €/h", num(v, 2)),
        "pct_h" => format!("{} %/h", num(v, 2)),
        "tok_h" => format!("{}/h", tokens_txt(v)),
        "eur" => format!("{} €", num(v, 2)),
        "dias" => format!("{} días", num(v, 1)),
        "usd" => format!("{} $", num(v, 2)),
        "horas" => format!("{} h", num(v, 1)),
        _ => tokens_txt(v),
    }
}

/// Cómo se llama una clave de proyecto, modelo, tarea o equipo.
pub fn nombre_clave(k: &str, tipo: &str) -> String {
    if k == OTROS {
        return "Sin atribuir".into();
    }
    if tipo == "tarea" {
        return nombre_tarea(k).map_or_else(|| k.to_string(), str::to_string);
    }
    match k {
        "resto" => "Resto".into(),
        "" => "Sin dato".into(),
        _ if tipo == "modelo" => capitalizar(k),
        _ => k.to_string(),
    }
}

fn indice_grupo(g: &str) -> usize {
    GRUPOS.iter().position(|(k, _)| *k == g).unwrap_or(0)
}

// ---------- agregar ----------

/// Los totales de un nombre (un proyecto, un equipo...).
#[derive(Clone, Debug, PartialEq)]
pub struct EntradaH {
    pub pct: f64,
    pub tok: f64,
    pub usd: f64,
    pub seg: f64,
    pub primero: String,
    pub ultimo: String,
    pub semanas: usize,
}

/// Un día de una semana de Claude: lo de cada grupo ([%, tokens, $, segundos]) y el % sin atribuir.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Celda {
    pub g: [BTreeMap<String, Acc>; 4],
    pub o: f64,
}

/// Lo que devuelve `agregar_historico`.
#[derive(Clone, Debug)]
pub struct Historico {
    /// Por grupo (el orden de `GRUPOS`): nombre -> totales.
    pub grupos: [BTreeMap<String, EntradaH>; 4],
    pub sin_atribuir: f64,
    /// (inicio de la semana, lo de cada grupo, % sin atribuir).
    pub semanas: Vec<(String, [BTreeMap<String, Acc>; 4], f64)>,
    /// (inicio de la semana en s, día) -> lo de ese día.
    pub celdas: BTreeMap<(i64, String), Celda>,
    /// [% (con lo no atribuido), tokens, $, segundos].
    pub total: Acc,
    pub desde: Option<String>,
    pub hasta: Option<String>,
    pub maquinas: Vec<String>,
}

/// Suma el archivo histórico por proyecto, equipo, modelo y tipo de tarea. Cada equipo guarda lo
/// suyo, así que se suma archivo a archivo y día a día (el día del reset está en dos archivos, uno
/// por semana, con filas distintas: cuenta cada uno una vez). Lo no atribuido lo calcula igual cada
/// equipo: por semana (±600 s) y por (día, hora) se toma el mayor entre equipos y luego se suma.
/// Sin filtro cuenta TODO; con `desde`/`hasta` ([desde, hasta)) solo los días de ese periodo, y con
/// `semana` solo los archivos de esa semana, enteros.
pub fn agregar_historico<A: AsRef<Archivo>>(archivos: &[A], desde: Option<NaiveDate>, hasta: Option<NaiveDate>, semana: Option<f64>) -> Historico {
    let mut grupos: [BTreeMap<String, EntradaH>; 4] = Default::default();
    let mut celdas: BTreeMap<(i64, String), Celda> = BTreeMap::new();
    let mut claves: Vec<f64> = Vec::new();
    let mut maquinas: BTreeSet<String> = BTreeSet::new();
    let mut dias_con: BTreeSet<String> = BTreeSet::new();
    let mut otros: HashMap<(i64, String, usize), f64> = HashMap::new();
    let mut sem_grupo: HashMap<(usize, String), BTreeSet<i64>> = HashMap::new();
    let validos = o_validos(archivos);
    let salto = primer_salto(archivos); // el primer salto de la historia no cuenta como sin atribuir
    let mut orden: Vec<usize> = (0..archivos.len()).collect();
    orden.sort_by(|a, b| archivos[*a].as_ref().inicio.total_cmp(&archivos[*b].as_ref().inicio));
    for i in orden {
        let a = archivos[i].as_ref();
        if semana.is_some_and(|s| (a.inicio - s).abs() > 600.0) {
            continue;
        }
        let clave = match claves.iter().find(|c| (a.inicio - **c).abs() <= 600.0) {
            Some(c) => *c as i64,
            None => {
                claves.push(a.inicio);
                a.inicio as i64
            }
        };
        for (d, dia) in &a.dias {
            if semana.is_none() {
                let f = fecha_iso(d).unwrap_or_default();
                if desde.is_some_and(|x| f < x) || hasta.is_some_and(|x| f >= x) {
                    continue;
                }
            }
            let cuenta_o = validos[i];
            let hay_o = cuenta_o && dia.o.iter().enumerate().any(|(h, x)| *x > 0.0 && !salta(&salto, a.inicio, d, h));
            if dia.f.is_empty() && !hay_o {
                continue;
            }
            let celda = celdas.entry((clave, d.clone())).or_default();
            for h in 0..24 {
                if cuenta_o && dia.o[h] > 0.0 && !salta(&salto, a.inicio, d, h) {
                    let e = otros.entry((clave, d.clone(), h)).or_insert(0.0);
                    *e = e.max(dia.o[h]);
                }
            }
            for f in &dia.f {
                let nombres = [&f.proyecto, &f.equipo, &f.modelo, &f.tarea];
                for (g, nombre) in nombres.into_iter().enumerate() {
                    let v = grupos[g].entry(nombre.clone()).or_insert_with(|| EntradaH {
                        pct: 0.0,
                        tok: 0.0,
                        usd: 0.0,
                        seg: 0.0,
                        primero: d.clone(),
                        ultimo: d.clone(),
                        semanas: 0,
                    });
                    v.pct += f.pct;
                    v.tok += f.tokens;
                    v.usd += f.coste;
                    v.seg += f.seg;
                    if *d < v.primero {
                        v.primero = d.clone();
                    }
                    if *d > v.ultimo {
                        v.ultimo = d.clone();
                    }
                    sem_grupo.entry((g, nombre.clone())).or_default().insert(clave);
                    let s = celda.g[g].entry(nombre.clone()).or_insert([0.0; 4]);
                    s[0] += f.pct;
                    s[1] += f.tokens;
                    s[2] += f.coste;
                    s[3] += f.seg;
                }
            }
            maquinas.insert(a.maquina.clone());
            dias_con.insert(d.clone());
        }
    }
    for ((clave, d, _), v) in otros {
        celdas.entry((clave, d)).or_default().o += v;
    }
    for ((g, nombre), s) in sem_grupo {
        if let Some(v) = grupos[g].get_mut(&nombre) {
            v.semanas = s.len();
        }
    }
    let mut por_semana: BTreeMap<i64, ([BTreeMap<String, Acc>; 4], f64)> = BTreeMap::new();
    for ((c, _), celda) in &celdas {
        let (gs, o) = por_semana.entry(*c).or_default();
        *o += celda.o;
        for g in 0..4 {
            for (nombre, v) in &celda.g[g] {
                let s = gs[g].entry(nombre.clone()).or_insert([0.0; 4]);
                for j in 0..4 {
                    s[j] += v[j];
                }
            }
        }
    }
    let semanas: Vec<(String, [BTreeMap<String, Acc>; 4], f64)> =
        por_semana.into_iter().map(|(c, (gs, o))| (iso(fecha_de(c as f64)), gs, o)).collect();
    let sin: f64 = semanas.iter().map(|s| s.2).sum();
    let pct: f64 = grupos[0].values().map(|v| v.pct).sum();
    let total = [
        pct + sin,
        grupos[0].values().map(|v| v.tok).sum(),
        grupos[0].values().map(|v| v.usd).sum(),
        grupos[0].values().map(|v| v.seg).sum(),
    ];
    Historico {
        grupos,
        sin_atribuir: sin,
        semanas,
        celdas,
        total,
        desde: dias_con.first().cloned(),
        hasta: dias_con.last().cloned(),
        maquinas: maquinas.into_iter().collect(),
    }
}

// ---------- periodos y series ----------

/// (desde, hasta, semana, título) de un periodo de la ventana Histórico: las fechas [desde, hasta)
/// (None = sin límite), el inicio de la semana de Claude si es de reset a reset (entonces se filtra
/// por archivo, no por fecha) y el título. `clave`: 'semana' / 'semana_pasada', 'mes', 'mes_pasado',
/// '3meses' (los últimos 90 días hasta hoy incluido), 'anio', 'todo' y 'personalizado' (`desde` y
/// `hasta` incluidos, cualquiera puede faltar).
pub type PeriodoH = (Option<NaiveDate>, Option<NaiveDate>, Option<f64>, String);

pub fn periodo_historico(clave: &str, ahora: f64, reset: Option<f64>, desde: Option<NaiveDate>, hasta: Option<NaiveDate>) -> Result<PeriodoH, String> {
    let hoy = fecha_de(ahora);
    match clave {
        "semana" | "semana_pasada" => {
            let ant = clave == "semana_pasada";
            let (d, h, t) = periodo("semana", ahora, reset, ant)?;
            let sem = reset.filter(|r| *r != 0.0).map(|r| r - SEMANA * if ant { 2.0 } else { 1.0 });
            Ok((Some(d), Some(h), sem, t))
        }
        "mes" | "mes_pasado" => {
            let (d, h, t) = periodo("mes", ahora, None, clave == "mes_pasado")?;
            Ok((Some(d), Some(h), None, t))
        }
        "3meses" => {
            let d = hoy - Duration::days(89);
            Ok((
                Some(d),
                Some(hoy + Duration::days(1)),
                None,
                format!("Últimos 3 meses (del {} al {})", fecha_larga(d, true), fecha_larga(hoy, true)),
            ))
        }
        "anio" => {
            let (d, h, t) = periodo("anio", ahora, None, false)?;
            Ok((Some(d), Some(h), None, t))
        }
        "personalizado" => {
            let h = hasta.map(|x| x + Duration::days(1));
            let t = match (desde, hasta) {
                (Some(d), Some(h)) => format!("Del {} al {}", fecha_larga(d, true), fecha_larga(h, true)),
                (Some(d), None) => format!("Desde el {}", fecha_larga(d, true)),
                (None, Some(h)) => format!("Hasta el {}", fecha_larga(h, true)),
                (None, None) => "Todo el histórico".into(),
            };
            Ok((desde, h, None, t))
        }
        "todo" => Ok((None, None, None, "Todo el histórico".into())),
        _ => Err(clave.to_string()),
    }
}

/// El paso del eje X según lo que dura el periodo con datos (`desde` y `hasta` incluidos): hasta 31
/// días, por día; hasta 26 semanas (182 días), por semana de Claude; más, por mes natural.
pub fn paso_auto(desde: NaiveDate, hasta: NaiveDate) -> &'static str {
    let dias = (hasta - desde).num_days() + 1;
    if dias <= 31 {
        "dia"
    } else if dias <= 182 {
        "semana"
    } else {
        "mes"
    }
}

/// Las sumas acumuladas de una lista de valores.
pub fn acumular(valores: &[f64]) -> Vec<f64> {
    let mut t = 0.0;
    valores
        .iter()
        .map(|v| {
            t += v;
            t
        })
        .collect()
}

/// Una fila del ranking de un grupo.
#[derive(Clone, Debug)]
pub struct FilaH {
    pub clave: String,
    pub nombre: String,
    pub valor: f64,
    /// El valor en cada unidad de `UNIDADES`.
    pub valores: BTreeMap<&'static str, Option<f64>>,
    /// % del total (en los cocientes, el del numerador).
    pub participacion: f64,
    pub primero: String,
    pub ultimo: String,
    pub semanas: usize,
}

/// Lo que enseña la ventana Histórico (y de lo que se comprueba todo).
#[derive(Clone, Debug)]
pub struct SeriesH {
    /// 'dia' | 'semana' | 'mes' (el de `paso_auto`, o el pedido).
    pub paso: String,
    /// (clave, etiqueta) de cada tramo, de la primera a la última actividad, también los vacíos.
    pub tramos: Vec<(String, String)>,
    pub series: BTreeMap<String, Vec<Option<f64>>>,
    /// Lo mismo acumulado: en los cocientes, suma de numeradores / suma de horas hasta cada tramo.
    pub series_acum: BTreeMap<String, Vec<Option<f64>>>,
    pub horas: BTreeMap<String, Vec<f64>>,
    pub horas_acum: BTreeMap<String, Vec<f64>>,
    /// Solo en € y días (en las demás, None).
    pub sin_atribuir: Option<Vec<f64>>,
    pub sin_acum: Option<Vec<f64>>,
    /// Ranking de TODOS los nombres por esa unidad.
    pub filas: Vec<FilaH>,
    /// Por unidad; en € y días incluye lo sin atribuir.
    pub total: BTreeMap<&'static str, Option<f64>>,
    pub sin: BTreeMap<&'static str, f64>,
    pub desde: Option<String>,
    pub hasta: Option<String>,
    pub maquinas: Vec<String>,
}

fn nombre_paso(p: Option<&str>) -> Option<&'static str> {
    PASOS.iter().find(|x| Some(**x) == p).copied()
}

/// El consumo por `grupo` en `unidad` de un periodo (como `agregar_historico`), en tramos de día,
/// semana de Claude o mes.
#[allow(clippy::too_many_arguments)]
pub fn series_historico<A: AsRef<Archivo>>(
    archivos: &[A],
    grupo: &str,
    unidad: &str,
    desde: Option<NaiveDate>,
    hasta: Option<NaiveDate>,
    semana: Option<f64>,
    precio_mes_eur: f64,
    paso: Option<&str>,
) -> SeriesH {
    let h = agregar_historico(archivos, desde, hasta, semana);
    let g = indice_grupo(grupo);
    let sin_ok = SIN_ATRIBUIR.contains(&unidad);
    let total: BTreeMap<&'static str, Option<f64>> =
        UNIDADES.iter().map(|(u, _)| (*u, valor_u(u, h.total[0], h.total[1], h.total[2], precio_mes_eur, h.total[3]))).collect();
    let sin: BTreeMap<&'static str, f64> =
        SIN_ATRIBUIR.iter().map(|u| (*u, valor_u(u, h.sin_atribuir, 0.0, 0.0, precio_mes_eur, 0.0).unwrap_or(0.0))).collect();
    let (Some(d0s), Some(d1s)) = (&h.desde, &h.hasta) else {
        return SeriesH {
            paso: nombre_paso(paso).unwrap_or("dia").to_string(),
            tramos: Vec::new(),
            series: BTreeMap::new(),
            series_acum: BTreeMap::new(),
            horas: BTreeMap::new(),
            horas_acum: BTreeMap::new(),
            sin_atribuir: sin_ok.then(Vec::new),
            sin_acum: sin_ok.then(Vec::new),
            filas: Vec::new(),
            total,
            sin,
            desde: None,
            hasta: None,
            maquinas: Vec::new(),
        };
    };
    let (d0, d1) = (fecha_iso(d0s).unwrap_or_default(), fecha_iso(d1s).unwrap_or_default());
    let paso = nombre_paso(paso).unwrap_or_else(|| paso_auto(d0, d1));
    let c0 = h.celdas.keys().map(|k| k.0).min().unwrap_or(0);
    let c1 = h.celdas.keys().map(|k| k.0).max().unwrap_or(0);
    let meses = |d: NaiveDate| d.year() * 12 + d.month() as i32;
    let (n, tramos): (usize, Vec<(String, String)>) = match paso {
        "dia" => {
            let n = ((d1 - d0).num_days() + 1) as usize;
            let t = (0..n as i64)
                .map(|i| {
                    let x = d0 + Duration::days(i);
                    let nom: String = DIAS[x.weekday().num_days_from_monday() as usize].chars().take(3).collect();
                    (iso(x), format!("{nom} {}", x.format("%d/%m")))
                })
                .collect();
            (n, t)
        }
        "semana" => {
            let n = (((c1 - c0) as f64 / SEMANA).round_ties_even() as usize) + 1;
            let t = (0..n)
                .map(|i| {
                    let a = fecha_de((c0 + i as i64 * SEMANA as i64) as f64);
                    let b = fecha_de((c0 + (i as i64 + 1) * SEMANA as i64 - 1) as f64);
                    (iso(a), format!("{}–{}", a.format("%d/%m"), b.format("%d/%m")))
                })
                .collect();
            (n, t)
        }
        _ => {
            let n = (meses(d1) - meses(d0) + 1) as usize;
            let m0 = meses(d0) - 1;
            let t = (m0..m0 + n as i32)
                .map(|m| (format!("{}-{:02}", m.div_euclid(12), m.rem_euclid(12) + 1), format!("{} {}", MESES[m.rem_euclid(12) as usize].chars().take(3).collect::<String>(), m.div_euclid(12))))
                .collect();
            (n, t)
        }
    };
    let idx = |c: i64, d: &str| -> usize {
        let i = match paso {
            "dia" => (fecha_iso(d).unwrap_or(d0) - d0).num_days(),
            "semana" => ((c - c0) as f64 / SEMANA).round_ties_even() as i64,
            _ => i64::from(meses(fecha_iso(d).unwrap_or(d0)) - meses(d0)),
        };
        i.clamp(0, n as i64 - 1) as usize
    };
    // por nombre y tramo, [%, tokens, $, segundos] SIN convertir: los cocientes se calculan de las sumas
    let mut crudo: BTreeMap<String, Vec<Acc>> = BTreeMap::new();
    let mut sin_serie = vec![0.0; n];
    for ((c, d), celda) in &h.celdas {
        let i = idx(*c, d);
        for (nombre, v) in &celda.g[g] {
            let r = &mut crudo.entry(nombre.clone()).or_insert_with(|| vec![[0.0; 4]; n])[i];
            for j in 0..4 {
                r[j] += v[j];
            }
        }
        if sin_ok {
            sin_serie[i] += valor_u(unidad, celda.o, 0.0, 0.0, precio_mes_eur, 0.0).unwrap_or(0.0);
        }
    }
    let (mut series, mut series_acum, mut horas, mut horas_acum) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    for (nombre, rs) in &crudo {
        let mut ac = [0.0; 4];
        let (mut va, mut ha) = (Vec::new(), Vec::new());
        for r in rs {
            for j in 0..4 {
                ac[j] += r[j];
            }
            va.push(valor_u(unidad, ac[0], ac[1], ac[2], precio_mes_eur, ac[3]));
            ha.push(ac[3] / 3600.0);
        }
        series.insert(nombre.clone(), rs.iter().map(|r| valor_u(unidad, r[0], r[1], r[2], precio_mes_eur, r[3])).collect());
        series_acum.insert(nombre.clone(), va);
        horas.insert(nombre.clone(), rs.iter().map(|r| r[3] / 3600.0).collect());
        horas_acum.insert(nombre.clone(), ha);
    }
    let pu = unidad_parte(unidad);
    let mut filas: Vec<FilaH> = Vec::new();
    for (nombre, e) in &h.grupos[g] {
        let valores: BTreeMap<&'static str, Option<f64>> =
            UNIDADES.iter().map(|(u, _)| (*u, valor_u(u, e.pct, e.tok, e.usd, precio_mes_eur, e.seg))).collect();
        let Some(Some(valor)) = valores.get(unidad).copied() else {
            continue; // un cociente sin horas: no hay cifra, no entra en el ranking
        };
        let tp = total.get(pu).copied().flatten().unwrap_or(0.0);
        let vp = valores.get(pu).copied().flatten().unwrap_or(0.0);
        filas.push(FilaH {
            clave: nombre.clone(),
            nombre: nombre_clave(nombre, grupo),
            valor,
            valores,
            participacion: if tp > 0.0 { vp / tp * 100.0 } else { 0.0 },
            primero: e.primero.clone(),
            ultimo: e.ultimo.clone(),
            semanas: e.semanas,
        });
    }
    filas.sort_by(|a, b| b.valor.total_cmp(&a.valor).then_with(|| a.nombre.cmp(&b.nombre)));
    let sin_acum = sin_ok.then(|| acumular(&sin_serie));
    SeriesH {
        paso: paso.to_string(),
        tramos,
        series,
        series_acum,
        horas,
        horas_acum,
        sin_atribuir: sin_ok.then_some(sin_serie),
        sin_acum,
        filas,
        total,
        sin,
        desde: h.desde.clone(),
        hasta: h.hasta.clone(),
        maquinas: h.maquinas.clone(),
    }
}

/// La línea de «Horas activas»: las horas activas por tramo de las series `claves` sumadas; con
/// `acum`, acumuladas. Un nombre sin horas cuenta 0.
pub fn horas_tramo(datos: &SeriesH, claves: &[&str], acum: bool) -> Vec<f64> {
    let mut out = vec![0.0; datos.tramos.len()];
    let fuente = if acum { &datos.horas_acum } else { &datos.horas };
    for k in claves {
        if let Some(v) = fuente.get(*k) {
            for (i, x) in v.iter().enumerate() {
                out[i] += x;
            }
        }
    }
    out
}

