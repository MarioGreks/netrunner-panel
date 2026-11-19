//! Reparto del % semanal entre lo que se hizo (cubos de todos los equipos), como `detalle.py` de
//! Consumo: cada subida del % semanal entre dos muestras se reparte entre los cubos de ese
//! intervalo en proporción a su peso. Es una estimación; lo que ningún cubo explica sale como
//! `OTROS`. Todo son funciones puras: no tocan disco ni reloj (reciben `ahora`).
//!
//! Las filas del reparto (`Fila`) no copian texto: apuntan a los cubos de los que salen.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use serde::Serialize;

use super::limites::{Muestra, SEMANA};
use super::transcripciones::CUBO;

/// Lo no atribuido, el resto de proyectos agrupados y las sesiones diminutas juntas.
pub const OTROS: &str = "(otros)";
pub const RESTO: &str = "(resto)";
#[allow(dead_code)] // se usa al pintar (encargos de interfaz)
pub const PEQUENAS: &str = "(pequeñas)";
/// Entre dos muestras del % más tiempo que esto (2 días), lo que subió se deja como «sin atribuir».
pub const GAP_MAX: f64 = 172_800.0;

const MITAD: f64 = CUBO as f64 / 2.0;
const CUBO_S: f64 = CUBO as f64;

/// Un cubo de 10 min de cualquier equipo (el propio o el publicado por otro). En los ficheros
/// de los equipos con Consumo antiguo falta la tarea: queda a «».
#[derive(Clone, Debug, PartialEq)]
pub struct CuboEq {
    pub inicio: f64,
    pub proyecto: String,
    pub sesion: String,
    pub modelo: String,
    pub peso: f64,
    pub tokens: f64,
    pub tarea: String,
    pub equipo: String,
}

/// Orden de Consumo (comparar las listas campo a campo): inicio, proyecto, sesión, modelo, peso,
/// tokens, tarea y equipo.
fn cmp_cubo(a: &CuboEq, b: &CuboEq) -> std::cmp::Ordering {
    a.inicio
        .total_cmp(&b.inicio)
        .then_with(|| a.proyecto.cmp(&b.proyecto))
        .then_with(|| a.sesion.cmp(&b.sesion))
        .then_with(|| a.modelo.cmp(&b.modelo))
        .then_with(|| a.peso.total_cmp(&b.peso))
        .then_with(|| a.tokens.total_cmp(&b.tokens))
        .then_with(|| a.tarea.cmp(&b.tarea))
        .then_with(|| a.equipo.cmp(&b.equipo))
}

/// Una porción del reparto: el % semanal atribuido a un cubo en el instante `t` (centro del cubo).
#[derive(Clone, Debug, PartialEq)]
pub struct Fila<'a> {
    pub t: f64,
    pub proyecto: &'a str,
    pub sesion: &'a str,
    pub modelo: &'a str,
    pub pct: f64,
    pub equipo: &'a str,
    pub tarea: &'a str,
}

/// Mapa que conserva el orden de inserción (como el `dict` de Python: los empates al ordenar
/// quedan en ese orden).
struct Mapa<'a, V> {
    idx: HashMap<&'a str, usize>,
    v: Vec<(&'a str, V)>,
}

impl<'a, V> Mapa<'a, V> {
    fn nuevo() -> Self {
        Mapa {
            idx: HashMap::new(),
            v: Vec::new(),
        }
    }
    fn entrada(&mut self, k: &'a str, def: impl FnOnce() -> V) -> &mut V {
        let i = match self.idx.get(k) {
            Some(&i) => i,
            None => {
                self.v.push((k, def()));
                self.idx.insert(k, self.v.len() - 1);
                self.v.len() - 1
            }
        };
        &mut self.v[i].1
    }
    fn contiene(&self, k: &str) -> bool {
        self.idx.contains_key(k)
    }
    fn obtener(&self, k: &str) -> Option<&V> {
        self.idx.get(k).map(|&i| &self.v[i].1)
    }
}

/// Orden descendente estable por la clave `f` (los empates conservan el orden de entrada).
fn orden_desc<T>(v: &mut [T], f: impl Fn(&T) -> f64) {
    v.sort_by(|a, b| f(b).total_cmp(&f(a)));
}

/// Reparte lo que subió el % semanal de la semana que acaba en `reset` entre los cubos de cada
/// intervalo entre dos muestras, en proporción a su peso. Lo que ningún cubo explica, a `OTROS`
/// (si ningún centro de cubo cae en el intervalo, valen los cubos que lo solapan, por solape ×
/// peso). `muestras` son las (instante, reset, %) del historial semanal; `cubos` deben ir
/// ordenados por inicio (`unir_cubos` los da así; si no, se ordenan aquí).
pub fn repartir<'a>(muestras: &[Muestra], reset: f64, cubos: &[&'a CuboEq]) -> Vec<Fila<'a>> {
    let ini = reset - SEMANA;
    let mut m: Vec<Muestra> = muestras
        .iter()
        .filter(|x| (x.1 - reset).abs() < 600.0 && x.0 >= ini)
        .copied()
        .collect();
    m.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    let mut pts = vec![(ini, 0.0)];
    let mut prev = 0.0;
    for (t, _, p) in m {
        if p > prev {
            pts.push((t, p));
            prev = p;
        }
    }
    let ordenados;
    let cs: &[&CuboEq] = if cubos.windows(2).all(|w| w[0].inicio <= w[1].inicio) {
        cubos
    } else {
        let mut v = cubos.to_vec();
        v.sort_by(|a, b| cmp_cubo(a, b));
        ordenados = v;
        &ordenados
    };
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let ((t0, p0), (t1, p1)) = (w[0], w[1]);
        // cubos cuyo centro cae en (t0, t1]
        let (lo, hi) = (
            cs.partition_point(|c| c.inicio + MITAD <= t0),
            cs.partition_point(|c| c.inicio + MITAD <= t1),
        );
        let mut en = &cs[lo..hi];
        let mut pesos: Vec<f64> = en.iter().map(|c| c.peso).collect();
        let mut total: f64 = pesos.iter().sum();
        if total <= 0.0 && t1 - t0 <= GAP_MAX {
            // ningún centro dentro (el % sube cada 4-5 min): los cubos que lo solapan, por solape
            let (lo, hi) = (
                cs.partition_point(|c| c.inicio + CUBO_S <= t0),
                cs.partition_point(|c| c.inicio < t1),
            );
            en = &cs[lo..hi];
            pesos = en
                .iter()
                .map(|c| {
                    c.peso * ((t1.min(c.inicio + CUBO_S) - t0.max(c.inicio)) / CUBO_S)
                })
                .collect();
            total = pesos.iter().sum();
        }
        if total <= 0.0 || t1 - t0 > GAP_MAX {
            // un hueco tan largo (p. ej. el historial empezó a mitad de semana) no se reparte; a
            // mitad del intervalo: en t1 caería en el día siguiente
            out.push(Fila {
                t: (t0 + t1) / 2.0,
                proyecto: OTROS,
                sesion: OTROS,
                modelo: OTROS,
                pct: p1 - p0,
                equipo: OTROS,
                tarea: OTROS,
            });
            continue;
        }
        for (c, w) in en.iter().zip(pesos) {
            out.push(Fila {
                t: c.inicio + MITAD,
                proyecto: &c.proyecto,
                sesion: &c.sesion,
                modelo: &c.modelo,
                pct: (p1 - p0) * w / total,
                equipo: &c.equipo,
                tarea: &c.tarea,
            });
        }
    }
    out
}

/// [(nombre, %)] del reparto agrupado por una columna: de mayor a menor y lo no atribuido
/// (`OTROS`) al final.
pub fn por_campo<'a>(filas: &[Fila<'a>], campo: impl Fn(&Fila<'a>) -> &'a str) -> Vec<(&'a str, f64)> {
    let mut tot: Mapa<f64> = Mapa::nuevo();
    for f in filas {
        *tot.entrada(campo(f), || 0.0) += f.pct;
    }
    let otros = tot.obtener(OTROS).copied();
    let mut reales: Vec<(&str, f64)> = tot.v.into_iter().filter(|(k, _)| *k != OTROS).collect();
    orden_desc(&mut reales, |x| x.1);
    if let Some(o) = otros {
        reales.push((OTROS, o));
    }
    reales
}

pub fn por_equipo<'a>(filas: &[Fila<'a>]) -> Vec<(&'a str, f64)> {
    por_campo(filas, |f| f.equipo)
}

pub fn por_modelo<'a>(filas: &[Fila<'a>]) -> Vec<(&'a str, f64)> {
    por_campo(filas, |f| f.modelo)
}

pub fn por_tarea<'a>(filas: &[Fila<'a>]) -> Vec<(&'a str, f64)> {
    por_campo(filas, |f| f.tarea)
}

/// Fecha local de un instante.
fn fecha(t: f64) -> NaiveDate {
    Local
        .timestamp_opt(t.floor() as i64, 0)
        .earliest()
        .unwrap_or_else(Local::now)
        .date_naive()
}

/// Instante de la medianoche local de una fecha.
fn medianoche(d: NaiveDate) -> f64 {
    let n = d.and_hms_opt(0, 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&n)
        .earliest()
        .or_else(|| Local.from_local_datetime(&n).latest())
        .map_or(0.0, |x| x.timestamp() as f64)
}

/// Fechas (locales) que toca la semana que acaba en `reset`: de la del inicio a la del reset (8 si
/// el reset no es a medianoche, con el primer y el último día a medias).
pub fn dias_semana(reset: f64) -> Vec<NaiveDate> {
    let (a, b) = (fecha(reset - SEMANA), fecha(reset - 1.0));
    (a.num_days_from_ce()..=b.num_days_from_ce())
        .filter_map(NaiveDate::from_num_days_from_ce_opt)
        .collect()
}

/// [(nombre, [% de cada día natural de la semana])] por proyecto, sesión o modelo (según
/// `campo`) de lo que devuelve `repartir`, en orden de aparición.
pub fn agrupar<'a>(
    filas: &[Fila<'a>],
    reset: f64,
    campo: impl Fn(&Fila<'a>) -> &'a str,
) -> Vec<(&'a str, Vec<f64>)> {
    let dias = dias_semana(reset);
    let n = dias.len();
    let ini = dias.first().map_or(0, |d| d.num_days_from_ce());
    let mut out: Mapa<Vec<f64>> = Mapa::nuevo();
    for f in filas {
        let d = (fecha(f.t).num_days_from_ce() - ini).clamp(0, n as i32 - 1) as usize;
        out.entrada(campo(f), || vec![0.0; n])[d] += f.pct;
    }
    out.v
}

/// Los `n` mayores de lo que devuelve `agrupar`, luego `RESTO` (los demás sumados) y `OTROS`
/// (lo no atribuido), si los hay.
pub fn principales<'a>(g: &[(&'a str, Vec<f64>)], n: usize) -> Vec<(&'a str, Vec<f64>)> {
    let mut reales: Vec<&(&str, Vec<f64>)> = g.iter().filter(|x| x.0 != OTROS).collect();
    orden_desc(&mut reales, |x| x.1.iter().sum());
    let mut out: Vec<(&str, Vec<f64>)> = reales.iter().take(n).map(|x| (x.0, x.1.clone())).collect();
    if reales.len() > n {
        let largo = reales[n].1.len();
        let mut resto = vec![0.0; largo];
        for x in &reales[n..] {
            for (r, v) in resto.iter_mut().zip(&x.1) {
                *r += v;
            }
        }
        out.push((RESTO, resto));
    }
    if let Some(o) = g.iter().find(|x| x.0 == OTROS) {
        out.push((OTROS, o.1.clone()));
    }
    out
}

/// Un proyecto (o `RESTO` / `OTROS`) con su % y sus sesiones de mayor a menor.
pub type GrupoProy<'a> = (&'a str, f64, Vec<(&'a str, f64)>);

/// Los `n` proyectos mayores, `RESTO` (los demás juntos) y `OTROS` (lo no atribuido), cada uno
/// con su % y sus sesiones.
pub fn por_proyecto_sesion<'a>(filas: &[Fila<'a>], n: usize) -> Vec<GrupoProy<'a>> {
    let mut tot: Mapa<f64> = Mapa::nuevo();
    let mut ses: Mapa<Mapa<f64>> = Mapa::nuevo();
    for f in filas {
        *tot.entrada(f.proyecto, || 0.0) += f.pct;
        *ses.entrada(f.proyecto, Mapa::nuevo).entrada(f.sesion, || 0.0) += f.pct;
    }
    let mut reales: Vec<(&str, f64)> = tot.v.iter().filter(|x| x.0 != OTROS).copied().collect();
    orden_desc(&mut reales, |x| x.1);
    let nombres: Vec<&str> = reales.iter().map(|x| x.0).collect();
    let mut grupos: Vec<(&str, Vec<&str>)> = nombres.iter().take(n).map(|k| (*k, vec![*k])).collect();
    if nombres.len() > n {
        grupos.push((RESTO, nombres[n..].to_vec()));
    }
    if tot.contiene(OTROS) {
        grupos.push((OTROS, vec![OTROS]));
    }
    grupos
        .into_iter()
        .map(|(nombre, miembros)| {
            let mut d: Mapa<f64> = Mapa::nuevo();
            for m in miembros {
                if let Some(s) = ses.obtener(m) {
                    for (k, p) in &s.v {
                        *d.entrada(k, || 0.0) += p;
                    }
                }
            }
            let mut v = d.v;
            let suma = v.iter().map(|x| x.1).sum();
            orden_desc(&mut v, |x| x.1);
            (nombre, suma, v)
        })
        .collect()
}

/// [(sesión, %)] de mayor a menor: las de menos de `minimo` % se juntan en un solo trozo
/// (`PEQUENAS`) al final; si ese trozo tampoco llega al mínimo, se suma a la última sesión grande.
/// La suma no cambia.
#[allow(dead_code)] // se usa al pintar (encargos de interfaz)
pub fn agrupar_pequenas<'a>(sesiones: &[(&'a str, f64)], minimo: f64) -> Vec<(&'a str, f64)> {
    let mut grandes: Vec<(&str, f64)> = sesiones.iter().filter(|s| s.1 >= minimo).copied().collect();
    let pequenas: f64 = sesiones.iter().filter(|s| s.1 < minimo).map(|s| s.1).sum();
    if pequenas <= 0.0 {
        return grandes;
    }
    if pequenas >= minimo || grandes.is_empty() {
        grandes.push((PEQUENAS, pequenas));
    } else if let Some(u) = grandes.last_mut() {
        u.1 += pequenas;
    }
    grandes
}

/// Los cubos de varios equipos en uno solo. Si la misma sesión sale en dos equipos (un chat que
/// se continuó en otro PC, o una carpeta de transcripciones compartida), el mismo cubo no se
/// cuenta dos veces: se queda el de más peso (a igualdad, el primero). Ordenados.
pub fn unir_cubos<'a>(listas: &[&'a [CuboEq]]) -> Vec<&'a CuboEq> {
    let mut out: HashMap<(u64, &str, &str, &str, &str), &CuboEq> = HashMap::new();
    for c in listas.iter().flat_map(|l| l.iter()) {
        let k = (c.inicio.to_bits(), &*c.proyecto, &*c.sesion, &*c.modelo, &*c.tarea);
        match out.get(&k) {
            Some(p) if c.peso <= p.peso => {}
            _ => {
                out.insert(k, c);
            }
        }
    }
    let mut v: Vec<&CuboEq> = out.into_values().collect();
    v.sort_by(|a, b| cmp_cubo(a, b));
    v
}

/// Series por hora: (nombres de los grupos, [% de cada hora de los últimos 7 días (168), desde la
/// medianoche de hace 6 días] por grupo, inicio).
pub type PorHoraRaw<'a> = (Vec<&'a str>, Vec<Vec<f64>>, f64);

/// Consumo por hora de los últimos 7 días de lo que devuelve `repartir`. Grupos como en
/// `por_proyecto_sesion`: los `n` proyectos mayores, `RESTO` y `OTROS`. Las horas se cuentan en
/// segundos desde ese inicio (en un cambio de hora se desplazan 1 h). Con `arrastrar`, lo anterior
/// al inicio (p. ej. el primer día de la semana en su último día) se suma a la primera hora en
/// vez de perderse: así, pasándole solo la semana actual, el total es el % real de la semana.
pub fn por_hora<'a>(filas: &[Fila<'a>], ahora: f64, n: usize, arrastrar: bool) -> PorHoraRaw<'a> {
    let hoy = fecha(ahora);
    let ini = medianoche(hoy - chrono::Duration::days(6));
    let en: Vec<&Fila> = filas
        .iter()
        .filter(|f| (arrastrar || ini <= f.t) && f.t < ini + 7.0 * 86_400.0)
        .collect();
    let mut tot: Mapa<f64> = Mapa::nuevo();
    for f in &en {
        *tot.entrada(f.proyecto, || 0.0) += f.pct;
    }
    let mut reales: Vec<(&str, f64)> = tot.v.iter().filter(|x| x.0 != OTROS).copied().collect();
    orden_desc(&mut reales, |x| x.1);
    let grandes: HashSet<&str> = reales.iter().take(n).map(|x| x.0).collect();
    let mut nombres: Vec<&str> = reales.iter().take(n).map(|x| x.0).collect();
    if reales.len() > n {
        nombres.push(RESTO);
    }
    if tot.contiene(OTROS) {
        nombres.push(OTROS);
    }
    let pos: HashMap<&str, usize> = nombres.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let mut series = vec![vec![0.0; 168]; nombres.len()];
    for f in en {
        let g = if f.proyecto == OTROS {
            OTROS
        } else if grandes.contains(f.proyecto) {
            f.proyecto
        } else {
            RESTO
        };
        let h = (((f.t - ini) / 3600.0).floor().max(0.0) as usize).min(167);
        series[pos[g]][h] += f.pct;
    }
    (nombres, series, ini)
}

// ---------- estado en un instante (la semana repetida, p. ej. para un vídeo) ----------

/// Todo lo que la tarjeta sabía en un instante de la semana (ver `estado_en`).
#[derive(Debug, PartialEq)]
#[allow(dead_code)] // para el vídeo de la semana (otro encargo)
pub struct EstadoEn<'a> {
    /// (% semanal, reset).
    pub semanal: (f64, f64),
    /// (% de 5 h, reset) si esa ventana seguía abierta.
    pub cinco_horas: Option<(f64, f64)>,
    pub todas: Vec<Muestra>,
    pub todas5: Vec<Muestra>,
    pub atrib: (f64, Vec<Fila<'a>>),
    pub equipos: Vec<(&'a str, f64)>,
    pub modelos: Vec<(&'a str, f64)>,
    pub tareas: Vec<(&'a str, f64)>,
    /// Última actividad de cada equipo.
    pub ultima: BTreeMap<&'a str, f64>,
    pub horas: PorHoraRaw<'a>,
}

/// Todo lo que la tarjeta sabía en el instante `t` de la semana que acaba en `r`: el uso (% semanal
/// y de 5 h), el historial hasta `t`, el reparto por proyecto, equipo, modelo y tarea de lo
/// repartido hasta `t`, la última actividad de cada equipo y el consumo por hora.
#[allow(dead_code)] // para el vídeo de la semana (otro encargo)
pub fn estado_en<'a>(
    t: f64,
    r: f64,
    todas: &[Muestra],
    todas5: &[Muestra],
    filas: &[Fila<'a>],
    cubos: &[&'a CuboEq],
) -> EstadoEn<'a> {
    let h: Vec<Muestra> = todas.iter().filter(|x| x.0 <= t).copied().collect();
    let h5: Vec<Muestra> = todas5.iter().filter(|x| x.0 <= t).copied().collect();
    let pct = h
        .iter()
        .rev()
        .find(|x| (x.1 - r).abs() < 600.0)
        .map_or(0.0, |x| x.2);
    let cinco = h5.last().filter(|x| x.1 > t).map(|x| (x.2, x.1));
    let f: Vec<Fila<'a>> = filas.iter().filter(|x| x.t <= t).cloned().collect();
    let mut ultima = BTreeMap::new();
    for c in cubos.iter().filter(|c| c.inicio + CUBO_S <= t) {
        let e = ultima.entry(&*c.equipo).or_insert(0.0_f64);
        *e = e.max(c.inicio + CUBO_S);
    }
    let horas = por_hora(&f, t, 5, true);
    EstadoEn {
        semanal: (pct, r),
        cinco_horas: cinco,
        equipos: por_equipo(&f),
        modelos: por_modelo(&f),
        tareas: por_tarea(&f),
        todas: h,
        todas5: h5,
        ultima,
        horas,
        atrib: (r, f),
    }
}

// ---------- el detalle que ve la interfaz ----------

/// Nombre y % (un modelo, una tarea, una sesión...).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parte {
    pub nombre: String,
    pub pct: f64,
}

/// Un proyecto (o «(resto)» / «(otros)») con su % y sus sesiones, de mayor a menor.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrupoProyecto {
    pub nombre: String,
    pub pct: f64,
    pub sesiones: Vec<Parte>,
}

/// Una serie de valores (de cada hora o de cada día) de un grupo.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Serie {
    pub nombre: String,
    pub valores: Vec<f64>,
}

/// % de cada hora de los últimos 7 días (168 valores por serie) desde `inicio` (s, medianoche
/// local de hace 6 días).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PorHora {
    pub inicio: f64,
    pub series: Vec<Serie>,
}

/// Un equipo con su % y su última actividad (s desde la época; None si no se sabe).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipoPct {
    pub nombre: String,
    pub pct: f64,
    pub ultima: Option<f64>,
}

/// % de cada día natural de la semana (`fechas` `AAAA-MM-DD`, con el primer y el último a
/// medias si el reset no es a medianoche), en total y por proyecto.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dias {
    pub fechas: Vec<String>,
    pub total: Vec<f64>,
    pub series: Vec<Serie>,
}

/// La semana anterior, resumida.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Previas {
    pub reset: f64,
    /// % repartido en total.
    pub pct: f64,
    pub proyectos: Vec<GrupoProyecto>,
    pub equipos: Vec<Parte>,
    pub modelos: Vec<Parte>,
    pub tareas: Vec<Parte>,
}

/// Lo que pinta la interfaz de Proyectos, Uso (modelo y tarea), Equipos, Consumo por hora y Días.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetalleConsumo {
    /// Fin de la semana del reparto (s): comparar con el `resetsAt` del estado para saber si es la vigente.
    pub reset: f64,
    /// % semanal usado cuando se calculó (None si no se sabía).
    pub pct: Option<f64>,
    /// Cuándo se calculó (s desde la época).
    pub calculado: f64,
    pub proyectos: Vec<GrupoProyecto>,
    pub por_hora: PorHora,
    pub equipos: Vec<EquipoPct>,
    pub modelos: Vec<Parte>,
    pub tareas: Vec<Parte>,
    pub dias: Dias,
    /// Equipos cuyo Consumo no guarda la tarea (sus cubos salen sin tarea).
    pub sin_tarea: Vec<String>,
    /// None si esa semana no se repartió nada.
    pub previas: Option<Previas>,
}

fn partes(v: Vec<(&str, f64)>) -> Vec<Parte> {
    v.into_iter()
        .map(|(n, p)| Parte {
            nombre: n.into(),
            pct: p,
        })
        .collect()
}

fn grupos(g: Vec<GrupoProy>) -> Vec<GrupoProyecto> {
    g.into_iter()
        .map(|(n, p, s)| GrupoProyecto {
            nombre: n.into(),
            pct: p,
            sesiones: partes(s),
        })
        .collect()
}

fn series(nombres: Vec<&str>, valores: Vec<Vec<f64>>) -> Vec<Serie> {
    nombres
        .into_iter()
        .zip(valores)
        .map(|(n, v)| Serie {
            nombre: n.into(),
            valores: v,
        })
        .collect()
}

/// Calcula el detalle de la semana que acaba en `reset` (la orquestación de `leer_detalle` de
/// Consumo). `todas` es el historial semanal fusionado de todos los equipos, `cubos` los de todos
/// (ya unidos), `otros_ts` el (equipo, última lectura) de lo que publican los demás en
/// `consumo.json` (para saber la última actividad de quien no publica detalle) y `pct` el %
/// semanal de ahora.
pub fn calcular_detalle(
    todas: &[Muestra],
    reset: f64,
    cubos: &[&CuboEq],
    otros_ts: &[(String, f64)],
    pct: Option<f64>,
    ahora: f64,
) -> DetalleConsumo {
    let filas = repartir(todas, reset, cubos);
    let (nombres, vals, inicio) = por_hora(&filas, ahora, 5, true);
    let mut ultima: BTreeMap<&str, f64> = BTreeMap::new();
    for c in cubos {
        let e = ultima.entry(&c.equipo).or_insert(0.0_f64);
        *e = e.max(c.inicio + CUBO_S);
    }
    for (m, ts) in otros_ts {
        if !m.is_empty() {
            ultima.entry(m).or_insert(*ts);
        }
    }
    let eq = por_equipo(&filas);
    let conocidos: HashSet<&str> = eq.iter().map(|x| x.0).collect();
    let mut equipos: Vec<EquipoPct> = Vec::new();
    let mut al_final: Option<EquipoPct> = None;
    for (n, p) in &eq {
        let e = EquipoPct {
            nombre: (*n).into(),
            pct: *p,
            ultima: ultima.get(n).copied(),
        };
        if *n == OTROS {
            al_final = Some(e);
        } else {
            equipos.push(e);
        }
    }
    // los que han publicado algo aunque no hayan gastado nada (así se ve cuál está parado)
    for (m, t) in &ultima {
        if !conocidos.contains(m) {
            equipos.push(EquipoPct {
                nombre: (*m).into(),
                pct: 0.0,
                ultima: Some(*t),
            });
        }
    }
    equipos.extend(al_final);
    let mut sin_tarea: Vec<String> = filas
        .iter()
        .filter(|f| f.tarea.is_empty() && f.equipo != OTROS)
        .map(|f| f.equipo.to_string())
        .collect();
    sin_tarea.sort();
    sin_tarea.dedup();
    let dias = dias_semana(reset);
    let g = principales(&agrupar(&filas, reset, |f| f.proyecto), 5);
    let total: Vec<f64> = (0..dias.len()).map(|i| g.iter().map(|x| x.1[i]).sum()).collect();
    let previas = {
        let ant = repartir(todas, reset - SEMANA, cubos);
        (!ant.is_empty()).then(|| Previas {
            reset: reset - SEMANA,
            pct: ant.iter().map(|f| f.pct).sum(),
            proyectos: grupos(por_proyecto_sesion(&ant, 5)),
            equipos: partes(por_equipo(&ant)),
            modelos: partes(por_modelo(&ant)),
            tareas: partes(por_tarea(&ant)),
        })
    };
    DetalleConsumo {
        reset,
        pct,
        calculado: ahora,
        proyectos: grupos(por_proyecto_sesion(&filas, 5)),
        por_hora: PorHora {
            inicio,
            series: series(nombres, vals),
        },
        equipos,
        modelos: partes(por_modelo(&filas)),
        tareas: partes(por_tarea(&filas)),
        dias: Dias {
            fechas: dias.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect(),
            total,
            series: g
                .into_iter()
                .map(|(n, v)| Serie {
                    nombre: n.into(),
                    valores: v,
                })
                .collect(),
        },
        sin_tarea,
        previas,
    }
}
