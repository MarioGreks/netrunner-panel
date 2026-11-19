//! Lógica de límites de Claude portada de la app Consumo: ritmo, estimaciones y avisos.
//! Funciones puras; solo se toca disco en leer/escribir los JSON, con la ruta recibida.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use serde::Serialize;
use serde_json::Value;

/// Segundos de una semana y de un día.
pub const SEMANA: f64 = 604_800.0;
pub const DIA: f64 = 86_400.0;
/// Duración de la ventana de 5 h (s).
pub const CINCO_HORAS: f64 = 18_000.0;
/// Segundos sin que el plugin actualice usage.json a partir de los cuales se da por «sin datos».
pub const VIEJO: f64 = 6.0 * 3600.0;
/// Historia que se conserva del % semanal (35 días) y del de 5 h (3 días).
pub const RETENER_SEMANAL: f64 = 35.0 * DIA;
pub const RETENER_5H: f64 = 259_200.0;
/// % de una ventana al que se avisa, una vez por ventana.
pub const UMBRALES: [u32; 2] = [80, 100];
/// Desde este riesgo (0..1) el nivel pasa a «riesgo» (Consumo mezcla el color de forma continua).
pub const UMBRAL_RIESGO: f64 = 0.5;

const DIAS: [&str; 7] = [
    "el lunes",
    "el martes",
    "el miércoles",
    "el jueves",
    "el viernes",
    "el sábado",
    "el domingo",
];

/// Muestra del historial: (instante, reset de la ventana, % usado). En el JSON es `[t, reset, pct]`.
pub type Muestra = (f64, f64, f64);

/// Una ventana de usage.json (`fiveHour` o `sevenDay`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Ventana {
    pub pct: f64,
    pub resets_at: Option<f64>,
}

/// Contenido de usage.json que nos interesa.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Uso {
    pub cinco_horas: Option<Ventana>,
    pub semanal: Option<Ventana>,
    /// Última vez que el plugin escribió el fichero (s desde la época); 0 si falta.
    pub ts: f64,
}

/// Nivel de la barra (en Consumo, su color).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Nivel {
    /// Al ritmo esperado o por debajo.
    #[default]
    Bien,
    /// Por encima del ritmo esperado.
    Encima,
    /// Por encima y con riesgo alto de quedarse sin cupo.
    Riesgo,
}

/// (% usado, % esperado por el tiempo transcurrido, segundos hasta el reset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ritmo {
    pub pct: f64,
    pub esperado: f64,
    pub falta: Option<f64>,
}

/// Resultado de `estimar_hoy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EstimHoy {
    pub estimado: f64,
    pub gastado: f64,
    pub riesgo: f64,
    pub ritmo_dia: Option<f64>,
}

/// Todo lo calculado de una ventana, listo para la interfaz.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoVentana {
    pub hay_datos: bool,
    pub pct: f64,
    pub esperado: f64,
    /// % estimado al acabar el día (semanal) o la ventana (5 h).
    pub fin: Option<f64>,
    pub riesgo: f64,
    pub nivel: Nivel,
    /// % estimado al acabar la semana (solo semanal).
    pub proyeccion_semana: Option<f64>,
    /// % por encima del 100 % a ese ritmo (0 si no se pasa).
    pub exceso: f64,
    /// «hoy gastarás ~X%» / «a este ritmo acabas en X%».
    pub texto_ritmo: Option<String>,
    /// «acabas en ~X% (+Y% el resto)».
    pub texto_semana: Option<String>,
    /// «sin límite el martes (+X%)».
    pub texto_sin_limite: Option<String>,
    pub resets_at: Option<i64>,
    pub segundos_hasta_reset: Option<i64>,
    /// Lo que falta para el reset en texto corto («1d 3h», «esperando datos»).
    pub texto_falta: String,
}

// ---------- lectura y escritura de ficheros ----------

fn leer_json(ruta: &Path) -> Option<Value> {
    let texto = fs::read_to_string(ruta).ok()?;
    serde_json::from_str(texto.trim_start_matches('\u{feff}')).ok()
}

fn ventana_de(v: &Value) -> Option<Ventana> {
    let o = v.as_object()?;
    if o.is_empty() {
        return None;
    }
    Some(Ventana {
        pct: o.get("pct").and_then(Value::as_f64).unwrap_or(0.0),
        resets_at: o.get("resetsAt").and_then(Value::as_f64),
    })
}

/// Lee usage.json; un fichero ausente o roto es «sin nada».
pub fn leer_uso(ruta: &Path) -> Uso {
    let Some(Value::Object(o)) = leer_json(ruta) else {
        return Uso::default();
    };
    Uso {
        cinco_horas: o.get("fiveHour").and_then(ventana_de),
        semanal: o.get("sevenDay").and_then(ventana_de),
        ts: o.get("ts").and_then(Value::as_f64).unwrap_or(0.0),
    }
}

fn cmp3(a: &Muestra, b: &Muestra) -> std::cmp::Ordering {
    a.0.total_cmp(&b.0)
        .then(a.1.total_cmp(&b.1))
        .then(a.2.total_cmp(&b.2))
}

/// Las muestras `[t, reset, pct]` bien formadas de un JSON; lo demás se descarta.
pub fn limpiar_hist(v: &Value) -> Vec<Muestra> {
    let Some(lista) = v.as_array() else {
        return Vec::new();
    };
    lista
        .iter()
        .filter_map(|m| {
            let a = m.as_array()?;
            if a.len() != 3 {
                return None;
            }
            Some((a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?))
        })
        .collect()
}

/// Carga el historial (ordenado y sin repetidos); un fichero roto es una lista vacía.
pub fn cargar_hist(ruta: &Path) -> Vec<Muestra> {
    let mut h = leer_json(ruta)
        .map(|v| limpiar_hist(&v))
        .unwrap_or_default();
    h.sort_by(cmp3);
    h.dedup();
    h
}

fn num(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 9e15 {
        Value::from(x as i64)
    } else {
        Value::from(x)
    }
}

/// Guarda el historial a un `.tmp` y lo renombra, para no dejarlo cortado si se cierra a medias.
pub fn guardar_hist(ruta: &Path, hist: &[Muestra]) -> io::Result<()> {
    let v: Vec<Value> = hist
        .iter()
        .map(|m| Value::Array(vec![num(m.0), num(m.1), num(m.2)]))
        .collect();
    let mut tmp = ruta.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, serde_json::to_vec(&v)?)?;
    fs::rename(&tmp, ruta).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Muestra que habría que añadir (si cambió el reset o el % respecto a la última) o None.
pub fn nueva_muestra(hist: &[Muestra], v: Option<&Ventana>, ahora: f64) -> Option<Muestra> {
    let v = v?;
    let reset = v.resets_at.filter(|r| *r > ahora)?;
    let nuevo = (ahora.trunc(), reset, v.pct);
    match hist.last() {
        Some(u) if u.1 == nuevo.1 && u.2 == nuevo.2 => None,
        _ => Some(nuevo),
    }
}

/// Añade la muestra si cambió el reset o el %, descartando lo más viejo que `retener`. True si cambió.
#[allow(dead_code)] // lo usan las pruebas; el hilo usa `apuntar_con`
pub fn apuntar(hist: &mut Vec<Muestra>, v: Option<&Ventana>, ahora: f64, retener: f64) -> bool {
    let Some(nuevo) = nueva_muestra(hist, v, ahora) else {
        return false;
    };
    hist.retain(|m| m.0 > ahora - retener);
    hist.push(nuevo);
    true
}

/// Como `apuntar`, pero decide con el historial fusionado de todos los equipos (`todas`, ordenado):
/// si otro equipo ya apuntó ese mismo % y reset, no se duplica. Solo se añade al historial propio.
pub fn apuntar_con(
    hist: &mut Vec<Muestra>,
    todas: &[Muestra],
    v: Option<&Ventana>,
    ahora: f64,
    retener: f64,
) -> bool {
    let Some(nuevo) = nueva_muestra(todas, v, ahora) else {
        return false;
    };
    hist.retain(|m| m.0 > ahora - retener);
    hist.push(nuevo);
    true
}

/// Si la semana ya venció, la de la siguiente con 0 % y el reset de 7 en 7 días hasta pasar de `ahora`.
/// La de 5 h no se toca. No modifica `uso`.
pub fn renovar_ventanas(uso: &Uso, ahora: f64) -> Uso {
    let mut out = uso.clone();
    if let Some(Ventana {
        resets_at: Some(r), ..
    }) = &uso.semanal
    {
        if *r <= ahora {
            let n = ((ahora - r) / SEMANA).floor() + 1.0;
            out.semanal = Some(Ventana {
                pct: 0.0,
                resets_at: Some(r + n * SEMANA),
            });
        }
    }
    out
}

// ---------- ritmo y estimaciones ----------

/// Ritmo de una ventana de duración `largo`; None sin datos. Vencida: todo a cero.
pub fn ritmo(v: Option<&Ventana>, largo: f64, ahora: f64) -> Option<Ritmo> {
    let v = v?;
    let reset = v.resets_at.unwrap_or(0.0);
    if reset <= ahora {
        return Some(Ritmo {
            pct: 0.0,
            esperado: 0.0,
            falta: None,
        });
    }
    let esperado = ((ahora - (reset - largo)) / largo * 100.0).clamp(0.0, 100.0);
    Some(Ritmo {
        pct: v.pct,
        esperado,
        falta: Some(reset - ahora),
    })
}

/// Último % apuntado hasta el instante `t` para esa ventana (mismo reset ±10 min).
pub fn pct_en(muestras: &[Muestra], reset: f64, t: f64) -> Option<f64> {
    let mut mejor: Option<&Muestra> = None;
    for m in muestras {
        if m.0 <= t
            && (m.1 - reset).abs() < 600.0
            && mejor.map_or(true, |b| cmp3(m, b).is_gt())
        {
            mejor = Some(m);
        }
    }
    mejor.map(|m| m.2)
}

/// Gasto por ventana completa como media móvil ponderada: la anterior pesa 1, cada más antigua la mitad
/// y la actual 2 × la fracción transcurrida (`fr`). `previos` va de la más reciente a la más antigua.
pub fn ritmo_medio(previos: &[f64], gastado: f64, fr: f64) -> f64 {
    let mut suma = 0.0;
    let mut pesos = 0.0;
    let mut w = 1.0;
    for p in previos {
        suma += w * p;
        pesos += w;
        w *= 0.5;
    }
    (suma + 2.0 * gastado) / (pesos + 2.0 * fr)
}

/// Estimación del gasto de hoy; None al empezar la semana. `base` = % al empezar el día;
/// `previos` = % gastado en cada día anterior.
pub fn estimar_hoy(
    pct: f64,
    reset: f64,
    ahora: f64,
    base: Option<f64>,
    previos: &[f64],
) -> Option<EstimHoy> {
    let dias = (ahora - (reset - SEMANA)) / DIA;
    if dias < 0.1 {
        return None;
    }
    let dia = (dias.trunc() as i64).min(6);
    let fd = (dias - dia as f64).min(1.0);
    let media = pct / dias;
    let gastado = base.map_or(media * fd, |b| pct - b);
    let (est, ritmo_dia) = if !previos.is_empty() {
        let r = ritmo_medio(previos, gastado, fd);
        (gastado.max(gastado + (1.0 - fd) * r), Some(r))
    } else {
        let hoy = if base.is_some() && fd >= 0.05 {
            gastado / fd
        } else {
            media
        };
        (gastado.max(fd * hoy + (1.0 - fd) * media), None)
    };
    let queda = 100.0 - (pct - gastado);
    let cuota = queda / (7 - dia) as f64;
    let riesgo = if dia == 6 || queda <= cuota {
        0.0
    } else {
        ((est - cuota) / (queda - cuota)).clamp(0.0, 1.0)
    };
    Some(EstimHoy {
        estimado: est,
        gastado,
        riesgo,
        ritmo_dia,
    })
}

/// (% al acabar la semana, % que se gastará en lo que queda) al ritmo diario medio o al promedio de la semana.
pub fn proyeccion_semana(
    pct: f64,
    reset: f64,
    ahora: f64,
    ritmo_dia: Option<f64>,
) -> Option<(f64, f64)> {
    let dias = (ahora - (reset - SEMANA)) / DIA;
    let resto = match ritmo_dia {
        Some(r) => r * (7.0 - dias).max(0.0),
        None if dias < 0.5 => return None,
        None => pct / dias * (7.0 - dias).max(0.0),
    };
    Some((pct + resto, resto))
}

/// Instante en que se llegaría al 100 % al ritmo de `proyeccion_semana`, o None si no antes del reset.
pub fn agotado_el(pct: f64, reset: f64, ahora: f64, ritmo_dia: Option<f64>) -> Option<f64> {
    let dias = (ahora - (reset - SEMANA)) / DIA;
    let r = match ritmo_dia {
        Some(r) => r,
        None if dias < 0.5 => return None,
        None => pct / dias,
    };
    if r <= 0.0 {
        return None;
    }
    let t = ahora + (100.0 - pct) / r * DIA;
    (t < reset).then_some(t)
}

/// «hoy», «mañana» o «el jueves» para el instante `t`, en hora local.
pub fn nombre_dia(t: f64, ahora: f64) -> String {
    use chrono::Datelike;
    let local = |x: f64| {
        Local
            .timestamp_opt(x as i64, 0)
            .single()
            .map(|d| d.date_naive())
    };
    let (Some(dt), Some(da)) = (local(t), local(ahora)) else {
        return "hoy".into();
    };
    let d = (dt - da).num_days();
    if d <= 0 {
        "hoy".into()
    } else if d == 1 {
        "mañana".into()
    } else {
        DIAS[dt.weekday().num_days_from_monday() as usize].into()
    }
}

/// % semanal gastado en cada día completo anterior (24 h contadas desde el reset), del más reciente
/// al más antiguo, hasta 7. Cruza a la semana anterior y se salta los días sin datos.
pub fn gastos_previos(muestras: &[Muestra], reset: f64, ahora: f64) -> Vec<f64> {
    let ini_sem = reset - SEMANA;
    let hoy0 = ini_sem + ((ahora - ini_sem) / DIA).floor() * DIA;
    let mut out = Vec::new();
    for i in 1..8 {
        let a = hoy0 - i as f64 * DIA;
        let b = hoy0 - (i - 1) as f64 * DIA;
        let sem = if a >= ini_sem { reset } else { ini_sem };
        let fin = pct_en(muestras, sem, b);
        let mut ini = pct_en(muestras, sem, a);
        if ini.is_none() && a <= sem - SEMANA + 1.0 {
            ini = Some(0.0);
        }
        if let (Some(f), Some(i)) = (fin, ini) {
            out.push((f - i).max(0.0));
        }
    }
    out
}

/// % final de cada ventana de 5 h anterior a la actual en la que hubo consumo, de la más reciente a la más antigua.
pub fn previos_5h(muestras: &[Muestra], reset: f64) -> Vec<f64> {
    let mut orden: Vec<&Muestra> = muestras.iter().collect();
    orden.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
    let mut fin: Vec<f64> = Vec::new();
    let mut w0: Option<f64> = None;
    for m in orden {
        if m.1 >= reset - 600.0 {
            break;
        }
        match (w0, fin.last_mut()) {
            (Some(w), Some(u)) if m.1 - w <= 600.0 => *u = m.2,
            _ => fin.push(m.2),
        }
        w0 = Some(m.1);
    }
    fin.into_iter().rev().filter(|p| *p > 0.0).collect()
}

/// Proyección simple al final de la ventana: % usado / fracción transcurrida (None muy al principio).
pub fn proyeccion(pct: f64, esp: f64) -> Option<f64> {
    (esp >= 2.0).then(|| pct / esp * 100.0)
}

/// Nivel de la barra y su riesgo (0..1): «bien» si va al ritmo o por debajo, «encima» si por encima,
/// «riesgo» si además el riesgo pasa de `UMBRAL_RIESGO`.
pub fn nivel_barra(pct: f64, esp: f64, riesgo: f64) -> (Nivel, f64) {
    let nivel = if pct > esp && riesgo >= UMBRAL_RIESGO {
        Nivel::Riesgo
    } else if pct > esp {
        Nivel::Encima
    } else {
        Nivel::Bien
    };
    (nivel, riesgo)
}

/// Texto corto de lo que falta: «1d 3h», «2h 05m», «12m»; «esperando datos» sin dato.
pub fn falta(s: Option<f64>) -> String {
    let Some(s) = s else {
        return "esperando datos".into();
    };
    let s = s as i64;
    let (d, h, m) = (s / 86400, s % 86400 / 3600, s % 3600 / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

// ---------- cálculo por ventana (el método `calcular` de Consumo) ----------

fn base_estado(res: Option<Ritmo>, v: Option<&Ventana>) -> EstadoVentana {
    let reset = v.and_then(|v| v.resets_at);
    EstadoVentana {
        hay_datos: res.is_some(),
        pct: res.map_or(0.0, |r| r.pct),
        esperado: res.map_or(0.0, |r| r.esperado),
        resets_at: reset.map(|r| r as i64),
        segundos_hasta_reset: res.and_then(|r| r.falta).map(|f| f as i64),
        texto_falta: falta(res.and_then(|r| r.falta)),
        ..Default::default()
    }
}

fn cerrar_nivel(mut d: EstadoVentana) -> EstadoVentana {
    d.nivel = nivel_barra(d.pct, d.esperado, d.riesgo).0;
    d
}

/// Ventana semanal: hoy, resto de la semana y cuándo se acabaría. `hist` es el historial del % semanal.
pub fn calcular_semanal(v: Option<&Ventana>, hist: &[Muestra], ahora: f64) -> EstadoVentana {
    let res = ritmo(v, SEMANA, ahora);
    let mut d = base_estado(res, v);
    if let (Some(r), Some(v)) = (res, v) {
        let reset = v.resets_at.unwrap_or(0.0);
        let pct = r.pct;
        let dia = (((ahora - (reset - SEMANA)) / DIA).floor() as i64).min(6);
        let base = if dia == 0 {
            Some(0.0)
        } else {
            pct_en(hist, reset, reset - SEMANA + dia as f64 * DIA)
        };
        let est = estimar_hoy(pct, reset, ahora, base, &gastos_previos(hist, reset, ahora));
        if let Some(e) = &est {
            d.fin = Some((pct + (e.estimado - e.gastado).max(0.0)).min(100.0));
            d.riesgo = e.riesgo;
            d.texto_ritmo = Some(format!("hoy gastarás ~{:.0}%", e.estimado));
        }
        let rd = est.and_then(|e| e.ritmo_dia);
        if let Some((total, resto)) = proyeccion_semana(pct, reset, ahora, rd) {
            d.proyeccion_semana = Some(total.min(100.0));
            d.texto_semana = Some(format!("acabas en ~{total:.0}% (+{resto:.0}% el resto)"));
            if total - 100.0 >= 0.5 {
                d.exceso = total - 100.0;
                if let Some(t100) = agotado_el(pct, reset, ahora, rd) {
                    d.texto_sin_limite = Some(format!(
                        "sin límite {} (+{:.0}%)",
                        nombre_dia(t100, ahora),
                        d.exceso
                    ));
                }
            }
        }
    }
    cerrar_nivel(d)
}

/// Ventana de 5 h. `hist5` es el historial del % de 5 h.
pub fn calcular_cinco(v: Option<&Ventana>, hist5: &[Muestra], ahora: f64) -> EstadoVentana {
    let res = ritmo(v, CINCO_HORAS, ahora);
    let mut d = base_estado(res, v);
    if let (Some(r), Some(v)) = (res, v) {
        let previos = previos_5h(hist5, v.resets_at.unwrap_or(0.0));
        let fr = r.esperado / 100.0;
        let proy = if previos.is_empty() {
            proyeccion(r.pct, r.esperado)
        } else {
            Some(r.pct + (1.0 - fr) * ritmo_medio(&previos, r.pct, fr))
        };
        if let Some(p) = proy {
            d.fin = Some(p.min(100.0));
            d.riesgo = ((p - 70.0) / 30.0).clamp(0.0, 1.0);
            d.texto_ritmo = Some(format!("a este ritmo acabas en {p:.0}%"));
        }
    }
    cerrar_nivel(d)
}

// ---------- avisos ----------

/// (ventana, reset, umbral) de un aviso ya dado.
pub type ClaveAviso = (String, i64, u32);

/// Un aviso por dar: su texto y las claves que cubre.
#[derive(Clone, Debug, PartialEq)]
pub struct Aviso {
    pub texto: String,
    pub claves: Vec<ClaveAviso>,
}

/// Avisos de las ventanas que han llegado a un umbral del que aún no se avisó en esa ventana.
/// Si se cruzan dos a la vez solo se avisa del mayor. `hechos` son las claves ya avisadas.
pub fn avisos_nuevos(uso: &Uso, hechos: &HashSet<ClaveAviso>, ahora: f64) -> Vec<Aviso> {
    let ventanas = [
        ("sevenDay", "Semanal", SEMANA, uso.semanal.as_ref()),
        (
            "fiveHour",
            "Sesión 5 h",
            CINCO_HORAS,
            uso.cinco_horas.as_ref(),
        ),
    ];
    let mut out = Vec::new();
    for (clave, nombre, largo, v) in ventanas {
        let Some(r) = ritmo(v, largo, ahora) else {
            continue;
        };
        let reset = v.and_then(|v| v.resets_at).unwrap_or(0.0) as i64;
        let cruzados: Vec<ClaveAviso> = UMBRALES
            .iter()
            .map(|u| (clave.to_string(), reset, *u))
            .filter(|k| r.pct >= k.2 as f64 && !hechos.contains(k))
            .collect();
        let Some(ultimo) = cruzados.last() else {
            continue;
        };
        let texto = if ultimo.2 >= 100 {
            format!(
                "{nombre} al {:.0} %: has llegado al límite. Se renueva en {}.",
                r.pct,
                falta(r.falta)
            )
        } else {
            format!(
                "{nombre} al {:.0} %: te queda un {:.0} % hasta el reset (en {}).",
                r.pct,
                100.0 - r.pct,
                falta(r.falta)
            )
        };
        out.push(Aviso {
            texto,
            claves: cruzados,
        });
    }
    out
}
