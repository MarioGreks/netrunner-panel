//! Informes HTML de un solo fichero: el resumen de un periodo (semana, mes, trimestre, semestre o
//! año) y el «Histórico total». Port de `informe_html`, `generar`, `historico_html` y
//! `generar_historico` de `informe.py`, con la paleta HUD de netrunner (fondo casi negro, amarillo
//! y cian, Orbitron / JetBrains Mono con fuentes del sistema como alternativa: el fichero es suelto,
//! no incrusta fuentes ni pide nada a internet).
//!
//! Los informes solo se generan a petición (los comandos `generar_informe_consumo` y
//! `generar_historico_consumo`): nada corre en segundo plano.
//!
//! Diferencias con Consumo:
//! - Sin `graficos` en los comandos (los gráficos de la tarjeta incrustados como imagen no se
//!   portan: se pasa `None`); la función `informe_html` conserva el parámetro.
//! - Los colores de proyecto salen de un hash del nombre sobre una paleta HUD (el mismo que usa la
//!   ventana), así que un proyecto tiene el mismo color en todos los informes; en Consumo, del
//!   puesto en el ranking.
//! - «Hoy» para el nº de días del periodo es el día de generación (en Python, `date.today()`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::agregado::{agregar, capitalizar, nombre_informe, periodo, Acc, Agregado, DIAS};
use super::archivo::{fecha_iso, Archivo};
use super::historico::*;
use super::informe_svg::{esc, horas_periodo, svg_calor, svg_columnas, Col};
use super::limites::SEMANA;
use super::reparto::OTROS;
use super::Consumo;

/// Gris de «sin atribuir» y de «resto».
pub const GRIS: &str = "#6f7585";
pub const RESTO: &str = "#4d5363";
/// Cian de lo que no es de un proyecto concreto (mapa de calor, tendencia).
pub const CIAN: &str = "#00e5ff";
/// Colores de proyecto: las variantes de los dos acentos HUD que usa la ventana, en el mismo orden.
const PALETA: [&str; 8] = ["#00e5ff", "#fcee0a", "#038192", "#8e870b", "#7eea84", "#77dfef", "#e9e481", "#b0eb54"];

/// FNV-1a de 32 bits (el mismo de `colores.ts`).
fn hash(nombre: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for u in nombre.encode_utf16() {
        h = (h ^ u32::from(u)).wrapping_mul(16_777_619);
    }
    h
}

/// Color estable de un nombre: el mismo en todos los informes.
pub fn color(k: &str) -> &'static str {
    match k {
        OTROS => GRIS,
        "resto" => RESTO,
        _ => PALETA[hash(k) as usize % PALETA.len()],
    }
}

pub const CSS: &str = r#"
:root{--bg:#07080c;--card:#0b0e14;--borde:#1d2531;--txt:#d9dbe3;--tenue:#7c8090;--pista:#1a2230;--verde:#00e5ff;--amarillo:#fcee0a;--aviso:#fcee0a;--titulos:"Orbitron","Segoe UI",system-ui,sans-serif;--datos:"JetBrains Mono","Cascadia Mono",Consolas,"Courier New",monospace}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--txt);font:14px/1.5 var(--datos)}
main{max-width:1100px;margin:0 auto;padding:32px 20px 64px}h1,h2,h3{font-family:var(--titulos);letter-spacing:.08em;text-transform:uppercase}
h1{font-size:26px;margin:0 0 4px;color:var(--amarillo)}h2{font-size:15px;margin:36px 0 12px;color:var(--verde)}h3{font-size:13px;margin:0 0 10px}
.sub{color:var(--tenue);margin:0 0 20px}.aviso{border:1px solid var(--aviso);color:var(--aviso);padding:10px 14px;margin:0 0 20px}
.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px}.kpi{background:var(--card);border:1px solid var(--borde);border-left:2px solid var(--verde);padding:14px 16px}
.kpi b{display:block;font-family:var(--titulos);font-size:22px;font-weight:700}.kpi span{color:var(--tenue);font-size:12px;letter-spacing:.06em;text-transform:uppercase}.kpi small{display:block;color:var(--tenue);font-size:12px;margin-top:2px}
.caja{background:var(--card);border:1px solid var(--borde);padding:16px;margin:0 0 12px;overflow-x:auto}.rejillas{display:grid;grid-template-columns:repeat(auto-fit,minmax(480px,1fr));gap:12px}
.tarjetas{display:grid;grid-template-columns:repeat(auto-fit,minmax(250px,1fr));gap:12px}
.tarjeta{background:var(--card);border:1px solid var(--borde);overflow:hidden;margin:0 0 12px}.tarjeta svg{display:block;width:100%;height:auto}
.tarjetas .tarjeta{margin:0}.grafico{width:100%;height:auto;display:block}.rejilla{stroke:var(--pista);stroke-width:1;opacity:.9}.eje{fill:var(--tenue);font-size:12px;font-family:var(--datos)}
table{border-collapse:collapse;width:100%}th{color:var(--tenue);font-weight:500;text-align:left;font-size:12px;padding:6px 8px;border-bottom:1px solid var(--borde);letter-spacing:.06em;text-transform:uppercase}
td{padding:6px 8px;border-bottom:1px solid var(--borde);font-size:13px}.n{text-align:right;white-space:nowrap}.t{color:var(--tenue)}
.reparto td{border:0;padding:4px 8px}.nom{white-space:nowrap;width:34%}.nom i{display:inline-block;width:10px;height:10px;margin-right:8px}
.barra{width:30%}.barra b{display:block;height:8px}.leyenda{display:flex;flex-wrap:wrap;gap:6px 16px;margin:10px 0 0;color:var(--tenue);font-size:12px}
.leyenda i{display:inline-block;width:10px;height:10px;margin-right:6px}footer{color:var(--tenue);font-size:12px;margin-top:40px;border-top:1px solid var(--borde);padding-top:16px}footer p{margin:6px 0}footer b{color:var(--txt)}
"#;

pub const HISTORICO_CSS: &str = r#"
[hidden]{display:none!important}
.botones{display:flex;flex-wrap:wrap;gap:8px 24px;margin:0 0 20px}.botones div{display:flex;flex-wrap:wrap;align-items:center;gap:6px;color:var(--tenue);font-size:12px;letter-spacing:.06em;text-transform:uppercase}
.botones button{font:inherit;font-size:12px;letter-spacing:.06em;text-transform:uppercase;color:var(--txt);background:var(--card);border:1px solid var(--borde);padding:5px 12px;cursor:pointer}
.botones button:hover{border-color:var(--amarillo);color:var(--amarillo)}
.botones button.activo{background:var(--amarillo);border-color:var(--amarillo);color:var(--bg);font-weight:700}
"#;

fn cabecera(titulo_pagina: &str, css: &str) -> String {
    format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Consumo · {}</title><style>{css}</style></head><body><main>",
        esc(titulo_pagina)
    )
}

fn fecha_hora(t: NaiveDateTime) -> String {
    t.format("%d/%m/%Y a las %H:%M").to_string()
}

fn tabla_reparto(titulo: &str, dic: &BTreeMap<String, Acc>, tipo: &str, con_coste: bool) -> String {
    let mut filas: Vec<(&String, &Acc)> = dic.iter().collect();
    filas.sort_by(|a, b| b.1[0].total_cmp(&a.1[0]));
    let mayor = filas.iter().map(|(_, v)| v[0]).fold(1e-9, f64::max);
    let mut o = format!("<div class=\"caja\"><h3>{}</h3><table class=\"reparto\">", esc(titulo));
    for (k, v) in filas {
        let c = color(k);
        let nombre = nombre_clave(k, tipo);
        let _ = write!(
            o,
            "<tr><td class=\"nom\"><i style=\"background:{c}\"></i>{}</td><td class=\"barra\"><b style=\"width:{:.1}%;background:{c}\"></b></td><td class=\"n\">{} %</td><td class=\"n t\">{}</td>{}</tr>",
            esc(&nombre),
            v[0] / mayor * 100.0,
            num(v[0], 1),
            tokens_txt(v[1]),
            if con_coste { format!("<td class=\"n t\">{} $</td>", num(v[2], 2)) } else { String::new() }
        );
    }
    o.push_str("</table></div>");
    o
}

/// Una leyenda de series (cuadrado de color y nombre).
fn leyenda(series: &[(String, String)], tipo: &str) -> String {
    let mut o = String::from("<div class=\"leyenda\">");
    for (k, c) in series {
        let _ = write!(o, "<span><i style=\"background:{c}\"></i>{}</span>", esc(&nombre_clave(k, tipo)));
    }
    o.push_str("</div>");
    o
}

const DESCRIPCION_PIE: &str = "<footer><p><b>Cómo se calcula.</b> El límite semanal de Claude solo da un porcentaje global. Consumo reparte cada subida de ese % entre lo que \
hizo cada sesión en ese intervalo, en proporción a su peso (tokens ponderados por precio y modelo): es una estimación, la fórmula real no es pública.</p>\
<p><b>Sin atribuir.</b> Es el % del límite que subió pero que ninguna sesión guardada explica: Consumo mira cuánto subió el % entre dos lecturas y lo \
reparte entre lo que hubo en ese intervalo; si no hubo nada, esa subida entera queda sin atribuir. Pasa cuando se gasta el mismo límite desde claude.ai, \
el móvil u otro equipo sin Consumo, cuando entre dos lecturas pasan más de 2 días (Consumo estaba cerrado o el historial empezó a mitad de semana) o cuando \
Claude Code ya borró las transcripciones. En los repartos, con la base «sobre el actual», cuenta como una parte más de lo consumido.</p>\
<p><b>Coste.</b> Lo que habría costado a precio de API (entrada, salida y caché). Con suscripción no se paga así: es una referencia para comparar.</p>\
<p><b>Tokens.</b> Todos los que procesa el modelo, también los de la caché de contexto, por eso salen cifras tan grandes.</p>\
<p><b>Caché y suscripción.</b> El ahorro de la caché es lo que costaría leer esos tokens como entrada normal menos lo que cuesta leerlos de la caché. \
Frente a la suscripción compara el coste a precio de API con lo que pagas al mes (ajuste «suscripcion» de consumo_ajustes.json, repartido por días): \
es solo una referencia, con suscripción no se paga por token.</p></footer></main></body></html>";

/// El informe del periodo agregado (ver `agregar`) como un HTML de un solo archivo, sin
/// dependencias. `graficos` son los de la tarjeta ya dibujados como SVG (`ritmo`, `proyecto`,
/// `equipos`, `modelo`, `tarea`, `stack`): los que vengan se usan tal cual (los comandos pasan
/// `None`). `precio_mes` es lo que pagas al mes de suscripción (en $): con él sale cuánto «ahorras»
/// frente al coste a precio de API.
pub fn informe_html(agr: &Agregado, titulo: &str, tipo: &str, generado: NaiveDateTime, graficos: Option<&BTreeMap<String, String>>, precio_mes: Option<f64>) -> String {
    let (total, dias) = (agr.total, &agr.dias);
    let semana = tipo == "semana";
    let (desde, hasta) = (agr.desde, agr.hasta);
    let hoy = generado.date();
    let n_dias = (hasta.min(hoy + Duration::days(1)) - desde).num_days();
    let con_datos = dias.keys().filter(|d| fecha_iso(d).is_some_and(|f| desde <= f && f < hasta)).count() as i64;
    let sesiones = &agr.sesiones;
    let horas_activas: usize = agr.horas_dia.values().map(|a| a.iter().filter(|v| **v > 0.0).count()).sum();
    let mut o = cabecera(titulo, CSS);
    let _ = write!(
        o,
        "<h1>{}</h1><p class=\"sub\">Resumen de Consumo · generado el {} · equipos: {}</p>",
        esc(titulo),
        fecha_hora(generado),
        esc(&if agr.maquinas.is_empty() { "ninguno".to_string() } else { agr.maquinas.join(", ") })
    );
    if dias.is_empty() {
        o.push_str(
            "<p class=\"aviso\">No hay datos guardados de este periodo todavía. Consumo guarda el detalle de cada semana en cuanto \
la app está en marcha; los periodos anteriores a la primera vez que se guardó no se pueden reconstruir.</p></main></body></html>",
        );
        return o;
    }
    if n_dias > 0 && (con_datos as f64) < 0.6 * n_dias as f64 {
        let _ = write!(
            o,
            "<p class=\"aviso\">Solo hay datos de {con_datos} de los {n_dias} días del periodo: el archivo histórico empieza cuando se \
guardó por primera vez, así que las cifras son de lo que sí está guardado.</p>"
        );
    }
    let sin: f64 = agr.sin_atribuir.values().sum();
    if total[0] > 0.0 && sin / total[0] > 0.5 {
        let _ = write!(
            o,
            "<p class=\"aviso\">La mayor parte de este periodo ({} %) está sin atribuir: el detalle por sesión solo existe desde que \
Consumo empezó a guardarlo, así que los proyectos, modelos y tareas de aquí cubren solo lo que sí está guardado.</p>",
            num(sin / total[0] * 100.0, 0)
        );
    }
    o.push_str("<section class=\"kpis\">");
    let _ = write!(
        o,
        "<div class=\"kpi\"><span>Consumo del límite semanal</span><b>{} %</b>{}</div>",
        num(total[0], 1),
        if semana { "<small>de la semana</small>".to_string() } else { format!("<small>= {} límites semanales</small>", num(total[0] / 100.0, 2)) }
    );
    let _ = write!(o, "<div class=\"kpi\"><span>Tokens procesados</span><b>{}</b><small>con la caché</small></div>", tokens_txt(total[1]));
    let _ = write!(o, "<div class=\"kpi\"><span>Coste estimado</span><b>{} $</b><small>a precio de API; es una referencia</small></div>", num(total[2], 2));
    let ch = agr.cache; // [caché leída, $ que ahorra, tokens y $ de los días que traen el dato]
    if ch[2] > 0.0 && ch[0] > 0.0 {
        let _ = write!(
            o,
            "<div class=\"kpi\"><span>Caché de contexto</span><b>{} %</b><small>de los tokens se leen de la caché: ahorra {} $ ({} % del coste){}</small></div>",
            num(ch[0] / ch[2] * 100.0, 0),
            num(ch[1], 2),
            num(ch[1] / (ch[3] + ch[1]) * 100.0, 0),
            if ch[2] < 0.95 * total[1] { " · solo de los días con este dato" } else { "" }
        );
    }
    let dias_pago = if n_dias > 0 { n_dias } else { con_datos };
    if let Some(precio) = precio_mes.filter(|p| *p != 0.0) {
        if dias_pago > 0 && total[2] > 0.0 {
            let pagado = precio * dias_pago as f64 / 30.44; // la suscripción se reparte por días
            let ahorro = total[2] - pagado;
            let _ = write!(
                o,
                "<div class=\"kpi\"><span>Frente a la suscripción</span><b>{}{} $</b><small>{} $ a precio de API frente a ~{} $ de suscripción ({} $/mes)</small></div>",
                if ahorro >= 0.0 { "+" } else { "−" },
                num(ahorro.abs(), 2),
                num(total[2], 2),
                num(pagado, 2),
                num(precio, 0)
            );
        }
    }
    let con_actividad = dias.values().filter(|v| v[1] > 0.0).count();
    let _ = write!(o, "<div class=\"kpi\"><span>Días con actividad</span><b>{con_actividad}</b><small>de {}</small></div>", n_dias.max(con_datos));
    let _ = write!(o, "<div class=\"kpi\"><span>Horas con actividad</span><b>{horas_activas}</b><small>horas distintas</small></div>");
    let n_proy = agr.proyectos.keys().filter(|k| k.as_str() != OTROS).count();
    let _ = write!(o, "<div class=\"kpi\"><span>Sesiones</span><b>{}</b><small>{n_proy} proyectos</small></div>", sesiones.len());
    o.push_str("</section>");
    let graficos = graficos.cloned().unwrap_or_default();
    let pintados: Vec<&str> = ["ritmo", "proyecto", "equipos", "modelo", "tarea"].into_iter().filter(|k| graficos.contains_key(*k)).collect();
    if !pintados.is_empty() {
        o.push_str("<h2>Los gráficos de la tarjeta</h2><div class=\"tarjetas\">");
        for k in pintados {
            let _ = write!(o, "<div class=\"tarjeta\">{}</div>", graficos[k]);
        }
        o.push_str("</div>");
    }
    if let Some(s) = graficos.get("stack") {
        let _ = write!(o, "<h2>Consumo por hora</h2><div class=\"tarjeta\">{s}</div>");
    }
    // --- en el tiempo: hora a hora, de seguido en todo el periodo
    let mut orden: Vec<(&String, &Acc)> = agr.proyectos.iter().filter(|(k, _)| k.as_str() != OTROS).collect();
    orden.sort_by(|a, b| b.1[0].total_cmp(&a.1[0]));
    let top: Vec<String> = orden.iter().take(5).map(|(k, _)| (*k).clone()).collect();
    let mut series: Vec<(String, String)> = top.iter().map(|k| (k.clone(), color(k).to_string())).collect();
    series.push(("resto".into(), RESTO.into()));
    series.push((OTROS.into(), GRIS.into()));
    let (cols, eje, sep, por) = horas_periodo(agr, &top, generado);
    let _ = write!(
        o,
        "<h2>Consumo hora a hora (% del límite semanal, por proyecto)</h2><div class=\"caja\">{}{}{}</div>",
        svg_columnas(&cols, &series, 1040.0, 250.0, Some(&eje), &sep, "Consumo hora a hora", None),
        leyenda(&series, "proyecto"),
        if por > 1 { format!("<p class=\"sub\" style=\"margin:8px 0 0\">Cada columna junta {por} horas: el periodo es largo para dibujar una por hora.</p>") } else { String::new() }
    );
    // --- tendencia semanal (periodos largos)
    if tipo != "semana" && agr.semanas.len() > 1 {
        let cs: Vec<Col> = agr
            .semanas
            .iter()
            .map(|(k, v, _, _)| {
                let et = fecha_iso(k).map_or_else(|| k.clone(), |d| d.format("%d/%m").to_string());
                (et, BTreeMap::from([("semana".to_string(), *v)]))
            })
            .collect();
        let _ = write!(
            o,
            "<h2>Tendencia semanal (% del límite que se gastó cada semana)</h2><div class=\"caja\">{}</div>",
            svg_columnas(&cs, &[("semana".to_string(), CIAN.to_string())], 1040.0, 250.0, None, &[], "Consumo en el tiempo", None)
        );
    }
    // --- repartos
    o.push_str("<h2>Reparto</h2><div class=\"rejillas\">");
    o.push_str(&tabla_reparto("Por proyecto", &agr.proyectos, "proyecto", true));
    o.push_str(&tabla_reparto("Por equipo", &agr.equipos, "equipo", true));
    o.push_str(&tabla_reparto("Por modelo", &agr.modelos, "modelo", true));
    o.push_str(&tabla_reparto("Por tipo de tarea", &agr.tareas, "tarea", true));
    o.push_str("</div>");
    if agr.sin_atribuir.values().any(|v| *v != 0.0) {
        let _ = write!(
            o,
            "<p class=\"sub\">Sin atribuir: {} % del límite que subió sin que ninguna sesión guardada lo explique \
(otro equipo sin Consumo, claude.ai, el móvil, un hueco de más de 2 días entre dos lecturas o transcripciones ya borradas). \
No está en las tablas de arriba; en los gráficos sale en gris. Más abajo, en «Cómo se calcula», se explica de dónde sale.</p>",
            num(agr.sin_atribuir.values().sum(), 1)
        );
    }
    // --- cuándo trabajas
    let (filas_h, etiquetas): (Vec<Vec<f64>>, Vec<String>) = if semana {
        let fs = agr.horas_dia.keys().filter_map(|d| fecha_iso(d).map(|f| (d, f)));
        fs.map(|(d, f)| (agr.horas_dia[d].clone(), format!("{} {}", dia3(f), f.format("%d/%m")))).unzip()
    } else {
        let mut acc = vec![vec![0.0; 24]; 7];
        for (d, arr) in &agr.horas_dia {
            if let Some(f) = fecha_iso(d) {
                let w = f.weekday().num_days_from_monday() as usize;
                for h in 0..24 {
                    acc[w][h] += arr[h];
                }
            }
        }
        (acc, (0..7).map(|i| capitalizar_dia(i)).collect())
    };
    let _ = write!(
        o,
        "<h2>Cuándo trabajas con Claude</h2><div class=\"caja\">{}<p class=\"sub\" style=\"margin:8px 0 0\">Más intenso = más tokens procesados en esa hora.</p></div>",
        svg_calor(&filas_h, &etiquetas, 1040.0, CIAN)
    );
    // --- detalle
    if semana {
        o.push_str(
            "<h2>Detalle por día</h2><div class=\"caja\"><table><tr><th>Día</th><th class=\"n\">% del límite</th><th class=\"n\">Sin atribuir</th>\
<th class=\"n\">Tokens</th><th class=\"n\">Coste</th><th>Proyecto principal</th></tr>",
        );
        for (d, v) in dias {
            let principal = agr
                .dia_proy
                .get(d)
                .and_then(|dp| dp.iter().max_by(|a, b| a.1[0].total_cmp(&b.1[0]).then_with(|| b.0.cmp(a.0))))
                .map_or("—".to_string(), |(k, _)| k.clone());
            let dd = fecha_iso(d).unwrap_or_default();
            let _ = write!(
                o,
                "<tr><td>{} {}</td><td class=\"n\">{} %</td><td class=\"n t\">{} %</td><td class=\"n t\">{}</td><td class=\"n t\">{} $</td><td>{}</td></tr>",
                capitalizar_dia(dd.weekday().num_days_from_monday() as usize),
                dd.format("%d/%m"),
                num(v[0], 1),
                num(agr.sin_atribuir.get(d).copied().unwrap_or(0.0), 1),
                tokens_txt(v[1]),
                num(v[2], 2),
                esc(&principal)
            );
        }
        o.push_str("</table></div>");
    }
    let mut lista: Vec<(&(String, String), &super::agregado::SesionAgr)> = sesiones.iter().collect();
    lista.sort_by(|a, b| b.1.a[0].total_cmp(&a.1.a[0]));
    if !semana {
        lista.truncate(20);
    }
    let _ = write!(
        o,
        "<h2>{}</h2><div class=\"caja\"><table><tr><th>Proyecto</th><th>Sesión</th><th>Modelo</th><th>Tarea principal</th><th>Equipo</th><th class=\"n\">Días</th>\
<th class=\"n\">% del límite</th><th class=\"n\">Tokens</th><th class=\"n\">Coste</th></tr>",
        if semana { "Todas las sesiones" } else { "Las 20 sesiones que más gastaron" }
    );
    for ((proy, ses), s) in lista {
        let tarea = s.tareas.iter().max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(a.0))).map_or(String::new(), |(k, _)| k.clone());
        let modelos: Vec<String> = s.modelos.iter().map(|m| nombre_clave(m, "modelo")).collect();
        let mut modelos = modelos;
        modelos.sort();
        let _ = write!(
            o,
            "<tr><td>{}</td><td class=\"t\">{}</td><td>{}</td><td>{}</td><td class=\"t\">{}</td><td class=\"n\">{}</td><td class=\"n\">{} %</td><td class=\"n t\">{}</td><td class=\"n t\">{} $</td></tr>",
            esc(proy),
            esc(ses),
            esc(&modelos.join(", ")),
            esc(&nombre_clave(&tarea, "tarea")),
            esc(&s.equipos.iter().cloned().collect::<Vec<_>>().join(", ")),
            s.dias.len(),
            num(s.a[0], 2),
            tokens_txt(s.a[1]),
            num(s.a[2], 2)
        );
    }
    o.push_str("</table></div>");
    o.push_str(DESCRIPCION_PIE);
    o
}

fn dia3(d: NaiveDate) -> String {
    DIAS[d.weekday().num_days_from_monday() as usize].chars().take(3).collect()
}

fn capitalizar_dia(i: usize) -> String {
    capitalizar(DIAS[i])
}

/// `(nombre del archivo, HTML)` del resumen `tipo` (`semana`, `mes`, `trimestre`, `semestre` o
/// `anio`) de los archivos históricos. Err con un tipo desconocido.
pub fn generar<A: AsRef<Archivo>>(
    archivos: &[A],
    tipo: &str,
    ahora: f64,
    reset: Option<f64>,
    anterior: bool,
    graficos: Option<&BTreeMap<String, String>>,
    precio_mes: Option<f64>,
) -> Result<(String, String), String> {
    let reset = reset.filter(|r| *r != 0.0);
    let (desde, hasta, titulo) = periodo(tipo, ahora, reset, anterior)?;
    let semana = if tipo == "semana" { reset.map(|r| r - SEMANA * if anterior { 2.0 } else { 1.0 }) } else { None };
    let agr = agregar(archivos, desde, hasta, semana);
    let t = Local.timestamp_opt(ahora as i64, 0).earliest().unwrap_or_else(Local::now).naive_local();
    Ok((nombre_informe(tipo, desde), informe_html(&agr, &titulo, tipo, t, graficos, precio_mes)))
}

// ---------- histórico total ----------

/// El formato de `svg_columnas` para la unidad `u` (los tokens se dibujan en millones).
fn fmt_eje_u(u: &str) -> Box<dyn Fn(f64, f64) -> String> {
    let u = u.to_string();
    Box::new(move |v, paso| {
        let d = |dec_entero: usize, dec: usize| if paso >= 1.0 { dec_entero } else { dec };
        match u.as_str() {
            "eur" => format!("{} €", num(v, d(0, 2))),
            "dias" => format!("{} d", num(v, d(0, 1))),
            "usd" => format!("{} $", num(v, d(0, 2))),
            "horas" => format!("{} h", num(v, d(0, 1))),
            "eur_h" => format!("{} €/h", num(v, d(0, 2))),
            "pct_h" => format!("{} %/h", num(v, d(0, 1))),
            "tok_h" => format!("{}/h", tokens_txt(v * 1e6)),
            _ => tokens_txt(v * 1e6),
        }
    })
}

fn num_corto(x: f64) -> String {
    format!("{x}").replace('.', ",")
}

fn es_razon(u: &str) -> bool {
    matches!(u, "eur_h" | "pct_h" | "tok_h")
}

/// Un bloque (agrupar por `g`, unidad `u`) del histórico: la tabla ordenada por esa unidad (con la
/// columna fija de horas activas) y el gráfico de columnas por semana (salvo en los cocientes por
/// hora, que no se apilan).
fn vista_historico(h: &Historico, g: &str, u: &str, visible: bool, precio: f64) -> String {
    let gi = GRUPOS.iter().position(|x| x.0 == g).unwrap_or(0);
    let val = |e: &EntradaH| valor_u(u, e.pct, e.tok, e.usd, precio, e.seg);
    let mut filas: Vec<(&String, &EntradaH, f64)> = h.grupos[gi].iter().filter_map(|(k, e)| val(e).map(|v| (k, e, v))).collect();
    filas.sort_by(|a, b| b.2.total_cmp(&a.2)); // un cociente sin horas no tiene cifra: fuera
    let con_sin = SIN_ATRIBUIR.contains(&u);
    let sin = if con_sin { valor_u(u, h.sin_atribuir, 0.0, 0.0, precio, 0.0).unwrap_or(0.0) } else { 0.0 };
    let pu = unidad_parte(u);
    let parte = |e: &EntradaH| valor_u(pu, e.pct, e.tok, e.usd, precio, e.seg).unwrap_or(0.0);
    let total = valor_u(pu, h.total[0], h.total[1], h.total[2], precio, h.total[3]).unwrap_or(0.0);
    let mayor = filas.iter().map(|f| f.2).chain([sin, 1e-9]).fold(1e-9, f64::max);
    let otras: Vec<&str> = UNIDADES_BASE.iter().copied().filter(|x| *x != u).collect();
    let nombre_g = GRUPOS[gi].1;
    let nombre_u = UNIDADES.iter().find(|x| x.0 == u).map_or(u, |x| x.1);
    let mut o = format!(
        "<section class=\"vista\" data-g=\"{g}\" data-u=\"{u}\"{}><h2>{} por {}</h2><div class=\"caja\"><table class=\"reparto\"><tr><th>{nombre_g}</th><th></th><th class=\"n\">{}</th><th class=\"n\">% del total</th><th class=\"n\">Horas</th>",
        if visible { "" } else { " hidden" },
        esc(nombre_u),
        nombre_g.to_lowercase(),
        esc(nombre_u)
    );
    for x in &otras {
        let n = UNIDADES.iter().find(|y| y.0 == *x).map_or(*x, |y| y.1);
        let _ = write!(o, "<th class=\"n\">{}</th>", esc(n));
    }
    o.push_str("<th class=\"n\">Primera</th><th class=\"n\">Última</th><th class=\"n\">Semanas</th></tr>");
    let f_larga = |s: &str| fecha_iso(s).map_or_else(|| s.to_string(), |d| d.format("%d/%m/%Y").to_string());
    for (k, e, v) in &filas {
        let c = color(k);
        let _ = write!(
            o,
            "<tr><td class=\"nom\"><i style=\"background:{c}\"></i>{}</td><td class=\"barra\"><b style=\"width:{:.1}%;background:{c}\"></b></td><td class=\"n\">{}</td><td class=\"n\">{} %</td><td class=\"n t\">{}</td>",
            esc(&nombre_clave(k, g)),
            v / mayor * 100.0,
            texto_u(u, Some(*v)),
            if total > 0.0 { num(parte(e) / total * 100.0, 1) } else { "0,0".to_string() },
            texto_u("horas", Some(e.seg / 3600.0))
        );
        for x in &otras {
            let _ = write!(o, "<td class=\"n t\">{}</td>", texto_u(x, valor_u(x, e.pct, e.tok, e.usd, precio, e.seg)));
        }
        let _ = write!(o, "<td class=\"n t\">{}</td><td class=\"n t\">{}</td><td class=\"n t\">{}</td></tr>", f_larga(&e.primero), f_larga(&e.ultimo), e.semanas);
    }
    if con_sin && sin > 0.0 {
        let _ = write!(
            o,
            "<tr><td class=\"nom\"><i style=\"background:{GRIS}\"></i>Sin atribuir</td><td class=\"barra\"><b style=\"width:{:.1}%;background:{GRIS}\"></b></td><td class=\"n\">{}</td><td class=\"n\">{} %</td><td class=\"n t\" colspan=\"{}\"></td></tr>",
            sin / mayor * 100.0,
            texto_u(u, Some(sin)),
            if total > 0.0 { num(sin / total * 100.0, 1) } else { "0,0".to_string() },
            otras.len() + 4
        );
    }
    o.push_str("</table></div>");
    if es_razon(u) {
        // un cociente no se apila (la suma de cocientes no es el cociente de la suma): sin gráfico de columnas
        o.push_str(
            "<div class=\"caja\"><p class=\"aviso\">Un valor por hora no se apila, así que esta vista no tiene gráfico de columnas: mira la tabla. \
Cada cifra es la suma del valor dividida entre la suma de horas activas.</p></div></section>",
        );
        return o;
    }
    // --- columnas por semana: los 5 mayores, el resto y (en € y días) lo sin atribuir
    let top: Vec<String> = filas.iter().take(5).map(|f| f.0.clone()).collect();
    let mut series: Vec<(String, String)> = top.iter().map(|k| (k.clone(), color(k).to_string())).collect();
    series.push(("resto".into(), RESTO.into()));
    if con_sin {
        series.push((OTROS.into(), GRIS.into()));
    }
    let escala = escala_u(u); // los tokens, en millones
    let mut cols: Vec<Col> = Vec::new();
    for (ini, gs, o_pct) in &h.semanas {
        let mut c: BTreeMap<String, f64> = BTreeMap::new();
        for (k, v) in &gs[gi] {
            let kk = if top.contains(k) { k.clone() } else { "resto".to_string() };
            *c.entry(kk).or_insert(0.0) += valor_u(u, v[0], v[1], v[2], precio, v[3]).unwrap_or(0.0) * escala;
        }
        if con_sin && *o_pct != 0.0 {
            c.insert(OTROS.into(), valor_u(u, *o_pct, 0.0, 0.0, precio, 0.0).unwrap_or(0.0));
        }
        let et = fecha_iso(ini).map_or_else(|| ini.clone(), |d| d.format("%d/%m").to_string());
        cols.push((et, c));
    }
    let fmt = fmt_eje_u(u);
    let _ = write!(
        o,
        "<div class=\"caja\"><h3>{} por semana (semana que empieza el día indicado)</h3>{}{}</div></section>",
        esc(nombre_u),
        svg_columnas(&cols, &series, 1040.0, 250.0, None, &[], &format!("{nombre_u} por semana"), Some(&*fmt)),
        leyenda(&series, g)
    );
    o
}

const SCRIPT_HISTORICO: &str = "<script>(function(){var g=\"proyecto\",u=\"eur\";function r(){document.querySelectorAll(\".vista\").forEach(function(s){s.hidden=!(s.dataset.g===g&&s.dataset.u===u)});\
document.querySelectorAll(\"[data-bg]\").forEach(function(b){b.classList.toggle(\"activo\",b.dataset.bg===g)});\
document.querySelectorAll(\"[data-bu]\").forEach(function(b){b.classList.toggle(\"activo\",b.dataset.bu===u)})}\
document.querySelectorAll(\"[data-bg]\").forEach(function(b){b.onclick=function(){g=b.dataset.bg;r()}});\
document.querySelectorAll(\"[data-bu]\").forEach(function(b){b.onclick=function(){u=b.dataset.bu;r()}});r()})()</script></main></body></html>";

/// El informe «Histórico total» (ver `agregar_historico`) como un HTML de un solo archivo: lo que ha
/// consumido cada proyecto, equipo, modelo y tipo de tarea desde que hay archivo histórico (o en el
/// periodo `titulo`, si `h` es de un periodo), en € de coste real, días de límite, dólares, tokens,
/// horas activas o esas cifras por hora activa. Las 32 vistas (agrupar × unidad) van en el HTML y un
/// script mínimo enseña la elegida; sin JS se ve la de € por proyecto.
pub fn historico_html(h: &Historico, generado: NaiveDateTime, precio_mes_eur: f64, titulo: Option<&str>) -> String {
    let nombre = match titulo {
        Some(t) if !t.is_empty() && t != "Todo el histórico" => format!("Histórico · {t}"),
        _ => "Histórico total".to_string(),
    };
    let mut o = cabecera(&nombre, &format!("{CSS}{HISTORICO_CSS}"));
    let fechas = if h.semanas.is_empty() { None } else { h.desde.as_deref().and_then(fecha_iso).zip(h.hasta.as_deref().and_then(fecha_iso)) };
    let Some((d0, d1)) = fechas else {
        let _ = write!(
            o,
            "<h1>{}</h1><p class=\"sub\">Generado el {}</p><p class=\"aviso\">No hay datos guardados{}. Consumo guarda el detalle de cada semana en cuanto la app está en marcha; \
lo anterior a la primera vez que se guardó no se puede reconstruir.</p></main></body></html>",
            esc(&nombre),
            fecha_hora(generado),
            if nombre != "Histórico total" { " en ese periodo" } else { " todavía" }
        );
        return o;
    };
    let [pct, tok, usd, seg] = h.total;
    let eur = valor_u("eur", pct, tok, usd, precio_mes_eur, 0.0).unwrap_or(0.0);
    let _ = write!(
        o,
        "<h1>{}</h1><p class=\"sub\">Generado el {} · del {} al {} · {} semanas · equipos: {}</p>",
        esc(&nombre),
        fecha_hora(generado),
        d0.format("%d/%m/%Y"),
        d1.format("%d/%m/%Y"),
        h.semanas.len(),
        esc(&if h.maquinas.is_empty() { "ninguno".to_string() } else { h.maquinas.join(", ") })
    );
    o.push_str("<div class=\"botones\"><div>Agrupar por: ");
    for (g, n) in GRUPOS {
        let _ = write!(o, "<button type=\"button\" data-bg=\"{g}\"{}>{n}</button>", if g == "proyecto" { " class=activo" } else { "" });
    }
    o.push_str("</div><div>Unidad: ");
    for (u, n) in UNIDADES {
        let _ = write!(o, "<button type=\"button\" data-bu=\"{u}\"{}>{n}</button>", if u == "eur" { " class=activo" } else { "" });
    }
    o.push_str("</div></div>");
    let _ = write!(
        o,
        "<section class=\"kpis\">\
<div class=\"kpi\"><span>Coste real</span><b>{} €</b><small>{} €/mes; 100 % semanal = {} €</small></div>\
<div class=\"kpi\"><span>Días de límite</span><b>{} días</b><small>= {} % del límite semanal ({} semanas enteras)</small></div>\
<div class=\"kpi\"><span>Coste a precio de API</span><b>{} $</b><small>es una referencia</small></div>\
<div class=\"kpi\"><span>Tiempo activo</span><b>{} h</b><small>huecos de menos de 10 min entre mensajes</small></div>\
<div class=\"kpi\"><span>Tokens procesados</span><b>{}</b><small>con la caché</small></div>\
<div class=\"kpi\"><span>Semanas con datos</span><b>{}</b><small>{} proyectos · {} equipos</small></div></section>",
        num(eur, 2),
        num_corto(precio_mes_eur),
        num(eur_semana(precio_mes_eur), 2),
        num(pct * 7.0 / 100.0, 1),
        num(pct, 1),
        num(pct / 100.0, 2),
        num(usd, 2),
        num(seg / 3600.0, 1),
        tokens_txt(tok),
        h.semanas.len(),
        h.grupos[0].len(),
        h.grupos[1].len()
    );
    for (g, _) in GRUPOS {
        for (u, _) in UNIDADES {
            o.push_str(&vista_historico(h, g, u, g == "proyecto" && u == "eur", precio_mes_eur));
        }
    }
    let _ = write!(
        o,
        "<footer><p><b>Coste real (€).</b> Es el % del límite semanal convertido a euros con tu suscripción: un mes son 52/12 semanas, así que \
{} €/mes dan {} € por el 100 % del límite semanal (ajuste «suscripcion_eur»). \
Es lo que cuesta lo que usas si siempre gastaras el 100 % del límite: no un recibo.</p>\
<p><b>Dólares.</b> Lo que habría costado a precio de API; con suscripción no se paga así, es solo una referencia para comparar.</p>\
<p><b>Tiempo activo.</b> Suma de los huecos entre mensajes seguidos de una misma sesión cuando duran menos de 10 minutos (una pausa más larga no cuenta). Los archivos antiguos sin ese dato cuentan 0 h.</p>\
<p><b>Por hora activa.</b> €, % del límite o tokens divididos entre las horas activas: se calcula con las sumas (suma del valor / suma de horas), nunca sumando cocientes, y lo que no tiene horas no sale. No lleva gráfico de columnas porque un cociente no se apila.</p>\
<p><b>Día de límite.</b> Un día de límite es 1/7 del límite semanal de Claude (100 % / 7 ≈ 14,3 %): una semana entera de límite son 7 días.</p>\
<p><b>Estimación.</b> El reparto por proyecto, sesión, equipo, modelo o tarea es una estimación: el límite semanal de Claude solo da un porcentaje global y Consumo lo reparte \
según lo que hizo cada sesión en cada intervalo (ver el pie de los resúmenes).</p>\
<p><b>Alcance.</b> Solo cuenta desde que se guarda el archivo histórico: lo anterior no se puede reconstruir.</p>\
<p><b>Sin atribuir.</b> Los dólares y los tokens no tienen «sin atribuir»: solo existe para el % del límite, así que solo sale en € y días y cuenta en el total de esas unidades.</p></footer>{SCRIPT_HISTORICO}",
        num_corto(precio_mes_eur),
        num(eur_semana(precio_mes_eur), 2)
    );
    o
}

/// `(nombre del archivo, HTML)` del histórico: todo, o el periodo [desde, hasta) / `semana` con su
/// `titulo` (ver `periodo_historico`).
pub fn generar_historico<A: AsRef<Archivo>>(
    archivos: &[A],
    ahora: f64,
    precio_mes_eur: f64,
    desde: Option<NaiveDate>,
    hasta: Option<NaiveDate>,
    semana: Option<f64>,
    titulo: Option<&str>,
) -> (String, String) {
    let h = agregar_historico(archivos, desde, hasta, semana);
    let fecha = super::archivo::fecha_de(ahora).format("%Y-%m-%d").to_string();
    let nombre = match titulo {
        Some(t) if !t.is_empty() && t != "Todo el histórico" => {
            let limpio: String = t.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == ' ').collect();
            let limpio = limpio.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("Consumo - histórico {limpio} {fecha}.html")
        }
        _ => format!("Consumo - histórico total {fecha}.html"),
    };
    let t = Local.timestamp_opt(ahora as i64, 0).earliest().unwrap_or_else(Local::now).naive_local();
    (nombre, historico_html(&h, t, precio_mes_eur, titulo))
}

// ---------- comandos ----------

/// Guarda el informe en Descargas, lo abre en el navegador predeterminado y devuelve el nombre.
fn guardar_y_abrir(app: &AppHandle, nombre: &str, cuerpo: &str) -> Result<(), String> {
    let home = app.path().home_dir().ok().or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from)).unwrap_or_default();
    let carpeta = super::disco::carpeta_descargas(&home);
    let _ = std::fs::create_dir_all(&carpeta);
    let destino = carpeta.join(nombre);
    std::fs::write(&destino, cuerpo).map_err(|e| format!("No se pudo guardar el informe: {e}"))?;
    crate::accesos::abrir_shell(destino.to_string_lossy().into_owned())
}

/// Lo que necesitan los informes de la app: las carpetas del historial, el reset vigente y las
/// suscripciones (USD/mes del resumen, €/mes del histórico).
fn contexto(app: &AppHandle) -> (Vec<PathBuf>, Option<f64>, f64, f64) {
    let c = app.state::<Consumo>();
    let reset = {
        let g = super::bloquear(&c.interno);
        super::limites::renovar_ventanas(&g.uso, super::ahora_s()).semanal.and_then(|v| v.resets_at)
    };
    (c.carpetas_historial(), reset, c.suscripcion_usd(), c.suscripcion_eur())
}

fn leer_todo(app: &AppHandle, carpetas: &[PathBuf]) -> Vec<std::sync::Arc<Archivo>> {
    let c = app.state::<Consumo>();
    let mut l = super::bloquear(&c.lectura);
    l.cache.leer_archivos_en(carpetas)
}

/// Genera el resumen del periodo (`tipo`: semana, mes, trimestre, semestre o anio; el actual o el
/// anterior) con los archivos históricos de todos los equipos, lo guarda en Descargas y lo abre en
/// el navegador. Antes fuerza el archivado (sin saltarse la comprobación de Consumo.exe). Solo a
/// petición.
#[tauri::command]
pub async fn generar_informe_consumo(app: AppHandle, tipo: String, anterior: bool) -> Result<String, String> {
    if !super::archivo::TIPOS.contains(&tipo.as_str()) {
        return Err(format!("Tipo de informe no válido: {tipo}"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        super::forzar_archivo(&app);
        let (carpetas, reset, usd, _) = contexto(&app);
        let archivos = leer_todo(&app, &carpetas);
        let (nombre, cuerpo) = generar(&archivos, &tipo, super::ahora_s(), reset, anterior, None, Some(usd))?;
        guardar_y_abrir(&app, &nombre, &cuerpo)?;
        Ok(format!("RESUMEN GUARDADO EN DESCARGAS: {nombre}"))
    })
    .await
    .map_err(|e| format!("No se pudo generar el resumen: {e}"))?
}

/// Genera el «Histórico total» (o el del periodo de `vista`: `periodo` y, si es «personalizado»,
/// `desde` y `hasta`, como en `historico_consumo`), lo guarda en Descargas y lo abre en el
/// navegador. Antes fuerza el archivado. Solo a petición.
#[tauri::command]
pub async fn generar_historico_consumo(app: AppHandle, vista: Option<Value>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        super::forzar_archivo(&app);
        let (carpetas, reset, _, eur) = contexto(&app);
        let archivos = leer_todo(&app, &carpetas);
        let v = vista.unwrap_or(Value::Null);
        let cad = |k: &str| v.get(k).and_then(Value::as_str);
        let clave = cad("periodo").unwrap_or("todo");
        let (pd, ph) = (cad("desde").and_then(fecha_iso), cad("hasta").and_then(fecha_iso));
        let ahora = super::ahora_s();
        let (desde, hasta, semana, titulo) = periodo_historico(clave, ahora, reset, pd, ph)
            .or_else(|_| periodo_historico("todo", ahora, reset, None, None))?;
        let (nombre, cuerpo) = generar_historico(&archivos, ahora, eur, desde, hasta, semana, Some(&titulo));
        guardar_y_abrir(&app, &nombre, &cuerpo)?;
        Ok(format!("HISTÓRICO GUARDADO EN DESCARGAS: {nombre}"))
    })
    .await
    .map_err(|e| format!("No se pudo generar el histórico: {e}"))?
}
