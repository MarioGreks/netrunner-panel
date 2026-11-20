//! La vista del Histórico y lo que devuelven los comandos `historico_consumo` y
//! `semanas_archivadas` (ver `historico.rs` para la agregación y las series).

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::archivo::{fecha_de, fecha_iso, iso, Archivo, CacheArchivos};
use super::historico::*;
use super::limites::renovar_ventanas;
use super::Consumo;

// ---------- la vista ----------

/// El ajuste `historico_vista` (lo último que se vio), limpio y completo.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VistaHist {
    pub grupo: String,
    pub unidad: String,
    /// De `PERIODOS_HIST` o «personalizado».
    pub periodo: String,
    /// ISO o «».
    pub desde: String,
    pub hasta: String,
    pub modo: String,
    pub acum: bool,
    /// La serie «sin atribuir».
    pub sin: bool,
    /// La línea de horas activas sobre el gráfico.
    pub horas: bool,
    /// Nombres marcados por grupo.
    pub sel: BTreeMap<String, Vec<String>>,
    /// «AnchoxAlto+X+Y» o «».
    pub geom: String,
    /// «dia», «semana», «mes» o «auto».
    pub paso: String,
}

/// `\d{3,5}x\d{3,5}([+-]-?\d{1,5}){2}` completo.
fn geom_valida(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    let digitos = |i: &mut usize, min: usize, max: usize| -> bool {
        let ini = *i;
        while *i < b.len() && b[*i].is_ascii_digit() && *i - ini < max {
            *i += 1;
        }
        *i - ini >= min
    };
    if !digitos(&mut i, 3, 5) || b.get(i) != Some(&b'x') {
        return false;
    }
    i += 1;
    if !digitos(&mut i, 3, 5) {
        return false;
    }
    for _ in 0..2 {
        if !matches!(b.get(i), Some(b'+' | b'-')) {
            return false;
        }
        i += 1;
        if b.get(i) == Some(&b'-') {
            i += 1;
        }
        if !digitos(&mut i, 1, 5) {
            return false;
        }
    }
    i == b.len()
}

/// Sanea la vista que pide la interfaz: lo que falta o está mal vale lo de la primera vez (€,
/// proyecto, todo el histórico, apilado, por tramo); lo desconocido se descarta.
pub fn limpiar_vista_historico(v: &Value) -> VistaHist {
    let cad = |k: &str| v.get(k).and_then(Value::as_str);
    let en = |x: Option<&str>, lista: &[&str]| x.filter(|x| lista.contains(x)).map(str::to_string);
    let fecha = |k: &str| cad(k).filter(|x| fecha_iso(x).is_some()).unwrap_or("").to_string();
    let periodos: Vec<&str> = PERIODOS_HIST.iter().map(|p| p.0).chain(["personalizado"]).collect();
    let mut periodo = en(cad("periodo"), &periodos).unwrap_or_else(|| "todo".into());
    let (desde, hasta) = (fecha("desde"), fecha("hasta"));
    if periodo == "personalizado" && desde.is_empty() && hasta.is_empty() {
        periodo = "todo".into();
    }
    let mut sel = BTreeMap::new();
    if let Some(Value::Object(m)) = v.get("sel") {
        for (g, _) in GRUPOS {
            if let Some(Value::Array(l)) = m.get(g) {
                let nombres = l.iter().take(500).filter_map(Value::as_str).filter(|x| x.chars().count() <= 200).map(str::to_string).collect();
                sel.insert(g.to_string(), nombres);
            }
        }
    }
    let flag = |k: &str, def: bool| v.get(k).and_then(Value::as_bool).unwrap_or(def);
    let grupos: Vec<&str> = GRUPOS.iter().map(|g| g.0).collect();
    let unidades: Vec<&str> = UNIDADES.iter().map(|u| u.0).collect();
    VistaHist {
        grupo: en(cad("grupo"), &grupos).unwrap_or_else(|| "proyecto".into()),
        unidad: en(cad("unidad"), &unidades).unwrap_or_else(|| "eur".into()),
        periodo,
        desde,
        hasta,
        modo: en(cad("modo"), &MODOS_HIST).unwrap_or_else(|| "apilado".into()),
        acum: flag("acum", false),
        sin: flag("sin", true),
        horas: flag("horas", true),
        sel,
        geom: cad("geom").filter(|g| geom_valida(g)).unwrap_or("").to_string(),
        paso: en(cad("paso"), &["dia", "semana", "mes", "auto"]).unwrap_or_else(|| "auto".into()),
    }
}

/// True si `v` sirve como suscripción en €/mes (ajuste `suscripcion_eur`): un número finito mayor
/// que 0 y no disparatado (hasta 5.000).
pub fn precio_valido(v: &Value) -> bool {
    v.as_f64().is_some_and(|x| x.is_finite() && x > 0.0 && x <= 5000.0)
}

// ---------- lo que devuelve el comando ----------

/// Un valor en una unidad, con su texto.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValorUnidad {
    pub unidad: String,
    pub nombre: String,
    pub valor: Option<f64>,
    pub texto: String,
}

fn valores_unidad(m: &BTreeMap<&'static str, Option<f64>>) -> Vec<ValorUnidad> {
    UNIDADES
        .iter()
        .map(|(u, n)| {
            let v = m.get(u).copied().flatten();
            ValorUnidad { unidad: (*u).into(), nombre: (*n).into(), valor: v, texto: texto_u(u, v) }
        })
        .collect()
}

/// Una serie para dibujar (un proyecto, un equipo...). Los valores están en la unidad pedida SIN
/// escalar (los tokens, en tokens: se dibujan dividiendo por `escala`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerieHistorico {
    pub clave: String,
    pub nombre: String,
    /// Un valor por tramo; None = sin cifra (cociente sin horas activas).
    pub valores: Vec<Option<f64>>,
    pub acumulado: Vec<Option<f64>>,
    /// Horas activas por tramo y acumuladas.
    pub horas: Vec<f64>,
    pub horas_acumuladas: Vec<f64>,
    /// Lo que suma en el periodo (None en un cociente sin horas) y su texto.
    pub total: Option<f64>,
    pub texto: String,
}

/// Una fila de la tabla del ranking.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilaHistorico {
    pub clave: String,
    pub nombre: String,
    pub valor: f64,
    pub texto: String,
    /// El valor en cada unidad (para las columnas extra).
    pub valores: Vec<ValorUnidad>,
    pub participacion: f64,
    pub texto_participacion: String,
    pub primero: String,
    pub ultimo: String,
    pub semanas: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricoConsumo {
    /// La vista ya saneada (lo que de verdad se calculó).
    pub vista: VistaHist,
    pub titulo: String,
    pub hay_datos: bool,
    /// «dia», «semana» o «mes».
    pub paso: String,
    pub unidad: String,
    pub unidad_nombre: String,
    pub grupo_nombre: String,
    /// Los valores se dibujan multiplicados por esto (tokens en millones: 1e-6).
    pub escala: f64,
    /// Etiqueta del eje X y clave (inicio del tramo) de cada tramo.
    pub etiquetas: Vec<String>,
    pub claves_tramo: Vec<String>,
    /// De mayor a menor en la unidad pedida (las que no entran en el ranking, al final por nombre).
    pub series: Vec<SerieHistorico>,
    /// Lo no atribuido por tramo (solo en € y días) y acumulado.
    pub sin_atribuir: Option<Vec<f64>>,
    pub sin_acumulado: Option<Vec<f64>>,
    /// Horas activas de todas las series por tramo y acumuladas.
    pub horas: Vec<f64>,
    pub horas_acumuladas: Vec<f64>,
    pub filas: Vec<FilaHistorico>,
    /// Totales del periodo en cada unidad (en € y días incluye lo sin atribuir).
    pub total: Vec<ValorUnidad>,
    /// Lo sin atribuir en € y días.
    pub sin: Vec<ValorUnidad>,
    pub desde: Option<String>,
    pub hasta: Option<String>,
    pub maquinas: Vec<String>,
    /// Archivos semanales leídos (uno por equipo y semana) y semanas distintas.
    pub archivos: usize,
    pub semanas: usize,
    /// Suscripción con la que se convirtió a € (€/mes) y lo que cuesta el 100 % semanal.
    pub precio_eur_mes: f64,
    pub eur_semana: f64,
}

/// Calcula lo que pide la interfaz con los archivos ya leídos. Pura (recibe `ahora`, el reset
/// vigente y el precio).
pub fn calcular_historico<A: AsRef<Archivo>>(archivos: &[A], vista: &Value, ahora: f64, reset: Option<f64>, precio: f64) -> HistoricoConsumo {
    let v = limpiar_vista_historico(vista);
    let (pd, ph) = (fecha_iso(&v.desde), fecha_iso(&v.hasta));
    let (desde, hasta, semana, titulo) = periodo_historico(&v.periodo, ahora, reset, pd, ph)
        .or_else(|_| periodo_historico("todo", ahora, reset, None, None))
        .unwrap_or_default();
    let paso = (v.paso != "auto").then_some(v.paso.as_str());
    let d = series_historico(archivos, &v.grupo, &v.unidad, desde, hasta, semana, precio, paso);
    let u = v.unidad.as_str();
    let orden: Vec<&str> = d.filas.iter().map(|f| f.clave.as_str()).collect();
    let mut claves: Vec<&str> = orden.clone();
    claves.extend(d.series.keys().map(String::as_str).filter(|k| !orden.contains(k)));
    let series: Vec<SerieHistorico> = claves
        .iter()
        .map(|k| {
            let valores = d.series[*k].clone();
            let acumulado = d.series_acum[*k].clone();
            let total = d.filas.iter().find(|f| f.clave == *k).map(|f| f.valor);
            SerieHistorico {
                clave: (*k).into(),
                nombre: nombre_clave(k, &v.grupo),
                valores,
                acumulado,
                horas: d.horas[*k].clone(),
                horas_acumuladas: d.horas_acum[*k].clone(),
                total,
                texto: texto_u(u, total),
            }
        })
        .collect();
    let todas: Vec<&str> = d.series.keys().map(String::as_str).collect();
    let n_semanas = archivos.iter().map(|a| a.as_ref().inicio as i64).collect::<BTreeSet<_>>().len();
    let filas = d
        .filas
        .iter()
        .map(|f| FilaHistorico {
            clave: f.clave.clone(),
            nombre: f.nombre.clone(),
            valor: f.valor,
            texto: texto_u(u, Some(f.valor)),
            valores: valores_unidad(&f.valores),
            participacion: f.participacion,
            texto_participacion: format!("{} %", num(f.participacion, 1)),
            primero: f.primero.clone(),
            ultimo: f.ultimo.clone(),
            semanas: f.semanas,
        })
        .collect();
    let sin: Vec<ValorUnidad> = SIN_ATRIBUIR
        .iter()
        .map(|x| {
            let nombre = UNIDADES.iter().find(|y| y.0 == *x).map_or("", |y| y.1);
            let val = d.sin.get(x).copied();
            ValorUnidad { unidad: (*x).into(), nombre: nombre.into(), valor: val, texto: texto_u(x, val) }
        })
        .collect();
    HistoricoConsumo {
        hay_datos: !d.tramos.is_empty(),
        paso: d.paso.clone(),
        unidad: v.unidad.clone(),
        unidad_nombre: UNIDADES.iter().find(|y| y.0 == u).map_or("", |y| y.1).into(),
        grupo_nombre: GRUPOS.iter().find(|y| y.0 == v.grupo).map_or("", |y| y.1).into(),
        escala: escala_u(u),
        etiquetas: d.tramos.iter().map(|t| t.1.clone()).collect(),
        claves_tramo: d.tramos.iter().map(|t| t.0.clone()).collect(),
        series,
        sin_atribuir: d.sin_atribuir.clone(),
        sin_acumulado: d.sin_acum.clone(),
        horas: horas_tramo(&d, &todas, false),
        horas_acumuladas: horas_tramo(&d, &todas, true),
        filas,
        total: valores_unidad(&d.total),
        sin,
        desde: d.desde.clone(),
        hasta: d.hasta.clone(),
        maquinas: d.maquinas.clone(),
        archivos: archivos.len(),
        semanas: n_semanas,
        precio_eur_mes: precio,
        eur_semana: eur_semana(precio),
        titulo,
        vista: v,
    }
}

// ---------- semanas archivadas ----------

/// Un equipo de una semana archivada.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipoSemana {
    pub nombre: String,
    /// % del límite semanal que se le atribuye.
    pub pct: f64,
    pub tokens: f64,
}

/// Una semana de Claude archivada (de reset a reset), con lo de cada equipo.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanaArchivada {
    /// Inicio de la semana, `AAAA-MM-DD`.
    pub inicio: String,
    pub inicio_ts: f64,
    pub reset: f64,
    /// % atribuido a algo, % sin atribuir y total repartido.
    pub pct_atribuido: f64,
    pub pct_sin_atribuir: f64,
    pub pct_repartido: f64,
    /// % que marcaba la cuenta al cerrar la semana, si algún archivo lo guarda.
    pub pct_final: Option<f64>,
    pub equipos: Vec<EquipoSemana>,
    pub dias: usize,
}

/// Las semanas archivadas, de la más nueva a la más vieja.
pub fn calcular_semanas<A: AsRef<Archivo>>(archivos: &[A]) -> Vec<SemanaArchivada> {
    let mut claves: Vec<f64> = Vec::new();
    let mut orden: Vec<&Archivo> = archivos.iter().map(AsRef::as_ref).collect();
    orden.sort_by(|a, b| a.inicio.total_cmp(&b.inicio));
    let mut grupos: Vec<(f64, Vec<&Archivo>)> = Vec::new();
    for a in orden {
        match claves.iter().position(|c| (a.inicio - c).abs() <= 600.0) {
            Some(i) => grupos[i].1.push(a),
            None => {
                claves.push(a.inicio);
                grupos.push((a.inicio, vec![a]));
            }
        }
    }
    let mut out = Vec::new();
    for (ini, lista) in grupos {
        let h = agregar_historico(&lista, None, None, Some(ini));
        let mut por_eq: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        for f in lista.iter().flat_map(|a| a.dias.values()).flat_map(|d| d.f.iter()) {
            let e = por_eq.entry(&f.equipo).or_insert((0.0, 0.0));
            e.0 += f.pct;
            e.1 += f.tokens;
        }
        let atribuido: f64 = h.grupos[0].values().map(|v| v.pct).sum();
        out.push(SemanaArchivada {
            inicio: iso(fecha_de(ini)),
            inicio_ts: ini,
            reset: lista.iter().map(|a| a.reset).fold(f64::MIN, f64::max),
            pct_atribuido: atribuido,
            pct_sin_atribuir: h.sin_atribuir,
            pct_repartido: atribuido + h.sin_atribuir,
            pct_final: lista.iter().filter_map(|a| a.pct).reduce(f64::max),
            equipos: por_eq.into_iter().map(|(n, (p, t))| EquipoSemana { nombre: n.into(), pct: p, tokens: t }).collect(),
            dias: lista.iter().flat_map(|a| a.dias.keys()).collect::<BTreeSet<_>>().len(),
        });
    }
    out.reverse();
    out
}

// ---------- lectura con caché y comandos ----------

/// Lo leído de disco y el último resultado calculado: si no cambió ningún fichero ni la vista, la
/// respuesta sale de aquí sin leer ni calcular nada.
#[derive(Default)]
pub struct LecturaHistorico {
    pub cache: CacheArchivos,
    ultimo: Option<(u64, Arc<HistoricoConsumo>)>,
    ultimas_semanas: Option<(u64, Arc<Vec<SemanaArchivada>>)>,
}

fn hash<T: Hash + ?Sized>(x: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    x.hash(&mut h);
    h.finish()
}

impl LecturaHistorico {
    pub fn historico(&mut self, carpetas: &[PathBuf], vista: &Value, ahora: f64, reset: Option<f64>, precio: f64) -> Arc<HistoricoConsumo> {
        // la fecha de hoy entra en la clave: los periodos (este mes, 3 meses...) cambian con el día
        let clave = hash(&(self.cache.firma(carpetas), vista.to_string(), precio.to_bits(), iso(fecha_de(ahora)), reset.map(f64::to_bits)));
        if let Some((k, r)) = &self.ultimo {
            if *k == clave {
                return r.clone();
            }
        }
        let archivos = self.cache.leer_archivos_en(carpetas);
        let r = Arc::new(calcular_historico(&archivos, vista, ahora, reset, precio));
        self.ultimo = Some((clave, r.clone()));
        r
    }

    pub fn semanas(&mut self, carpetas: &[PathBuf]) -> Arc<Vec<SemanaArchivada>> {
        let clave = self.cache.firma(carpetas);
        if let Some((k, r)) = &self.ultimas_semanas {
            if *k == clave {
                return r.clone();
            }
        }
        let archivos = self.cache.leer_archivos_en(carpetas);
        let r = Arc::new(calcular_semanas(&archivos));
        self.ultimas_semanas = Some((clave, r.clone()));
        r
    }
}

/// Los datos que hacen falta para calcular, sacados de la app (rápido, sin tocar disco más que
/// `consumo_ajustes.json`).
fn contexto_de(app: &AppHandle) -> (Vec<PathBuf>, Option<f64>, f64) {
    let c = app.state::<Consumo>();
    let carpetas = c.carpetas_historial();
    let reset = {
        let g = super::bloquear(&c.interno);
        renovar_ventanas(&g.uso, super::ahora_s()).semanal.and_then(|v| v.resets_at)
    };
    (carpetas, reset, c.suscripcion_eur())
}

/// El histórico para dibujar: `vista` = {periodo (clave de PERIODOS_HIST, o «personalizado» con
/// `desde` y `hasta`), grupo (proyecto|equipo|modelo|tarea), unidad (eur|dias|usd|tokens|horas|
/// eur_h|pct_h|tok_h), modo (lineas|apilado), paso (dia|semana|mes|auto)}; lo que falte o esté mal
/// vale lo de la primera vez. Se calcula solo al pedirlo, en otro hilo, con los ficheros en caché.
#[tauri::command]
pub async fn historico_consumo(app: AppHandle, vista: Value) -> Result<HistoricoConsumo, String> {
    let (carpetas, reset, precio) = contexto_de(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let c = app.state::<Consumo>();
        let mut l = super::bloquear(&c.lectura);
        (*l.historico(&carpetas, &vista, super::ahora_s(), reset, precio)).clone()
    })
    .await
    .map_err(|e| format!("No se pudo calcular el histórico: {e}"))
}

/// Las semanas archivadas (de la más nueva a la más vieja) con lo de cada equipo.
#[tauri::command]
pub async fn semanas_archivadas(app: AppHandle) -> Result<Vec<SemanaArchivada>, String> {
    let (carpetas, _, _) = contexto_de(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let c = app.state::<Consumo>();
        let mut l = super::bloquear(&c.lectura);
        (*l.semanas(&carpetas)).clone()
    })
    .await
    .map_err(|e| format!("No se pudieron leer las semanas: {e}"))
}
