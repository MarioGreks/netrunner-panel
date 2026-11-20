//! Piezas SVG de los informes HTML (port de `paso_eje`, `_svg_columnas`,
//! `_horas_periodo` y `_svg_calor` de `informe.py`). Todo son funciones puras que devuelven texto.
//!
//! Diferencia con Consumo: las series de una columna van ordenadas por nombre (en Python, por
//! orden de inserción); solo cambia el orden del texto de ayuda de cada columna.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::{Datelike, Duration, NaiveDateTime};

use super::agregado::{Agregado, DIAS};
use super::historico::{num, tokens_txt, nombre_clave};
use super::reparto::OTROS;

/// Una columna del gráfico: etiqueta y valor por serie.
pub type Col = (String, BTreeMap<String, f64>);

/// `html.escape(s, quote=True)`.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#x27;"),
            _ => o.push(c),
        }
    }
    o
}

/// `{:g}` de Python para anchos y grosores: sin ceros sobrantes.
fn g(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

// ---------- gráficos ----------

/// El paso «redondo» de la rejilla de un eje que llega a `top` con como mucho `n` rayas.
pub fn paso_eje(top: f64, n: f64) -> f64 {
    if top <= 0.0 {
        return 1.0;
    }
    const PASOS: [f64; 18] = [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0];
    PASOS.iter().copied().find(|v| top / v <= n).unwrap_or_else(|| 10f64.powf((top / n).log10().ceil()))
}

/// Columnas apiladas: `cols` = [(etiqueta, {serie: valor})], `series` = [(nombre, color)] de abajo
/// arriba. `eje` = [(columna, texto)] pone esas etiquetas en el eje (si no, una de cada pocas
/// columnas) y `sep` son columnas ante las que va una raya vertical. `fmt(valor, paso)` da el texto
/// de un valor (en el eje, con el paso de la rejilla; en la ayuda de cada columna, con 0); sin él,
/// % del límite.
#[allow(clippy::too_many_arguments)]
pub fn svg_columnas(
    cols: &[Col],
    series: &[(String, String)],
    ancho: f64,
    alto: f64,
    eje: Option<&[(usize, String)]>,
    sep: &[usize],
    titulo: &str,
    fmt: Option<&dyn Fn(f64, f64) -> String>,
) -> String {
    let (izq, abajo, arriba) = (if fmt.is_none() { 46.0 } else { 74.0 }, 34.0, 10.0);
    let por_defecto = |v: f64, paso: f64| format!("{} %", num(v, if paso >= 1.0 { 0 } else { 1 }));
    let fmt: &dyn Fn(f64, f64) -> String = fmt.unwrap_or(&por_defecto);
    let top = cols.iter().map(|(_, v)| v.values().sum::<f64>()).fold(1e-9, f64::max);
    let paso = paso_eje(top, 6.0);
    let ym = ((top / paso).floor() + 1.0) * paso;
    let n = cols.len().max(1);
    let cw = (ancho - izq - 10.0) / n as f64;
    let x = |i: usize| izq + i as f64 * cw;
    let y = |v: f64| alto - abajo - v / ym * (alto - abajo - arriba);
    let mut o = format!("<svg viewBox=\"0 0 {} {}\" role=\"img\" class=\"grafico\"><title>{}</title>", g(ancho), g(alto), esc(titulo));
    for j in 0..=((ym / paso).round_ties_even() as i64) {
        let v = j as f64 * paso;
        let _ = write!(
            o,
            "<line x1=\"{izq}\" x2=\"{}\" y1=\"{:.1}\" y2=\"{:.1}\" class=\"rejilla\"/><text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\" class=\"eje\">{}</text>",
            ancho - 10.0,
            y(v),
            y(v),
            izq - 6.0,
            y(v) + 4.0,
            esc(&fmt(v, paso))
        );
    }
    for &i in sep {
        let _ = write!(o, "<line x1=\"{:.1}\" x2=\"{:.1}\" y1=\"{arriba}\" y2=\"{}\" class=\"rejilla\"/>", x(i), x(i), alto - abajo + 6.0);
    }
    let cada = ((n as f64 / 16.0 + 0.999) as usize).max(1);
    for (i, (et, vals)) in cols.iter().enumerate() {
        let mut base = 0.0;
        let tip = vals
            .iter()
            .filter(|(_, v)| **v > 0.05)
            .map(|(k, v)| format!("{} {}", nombre_clave(k, "proyecto"), fmt(*v, 0.0)))
            .collect::<Vec<_>>()
            .join(" · ");
        for (nombre, color) in series {
            let v = vals.get(nombre).copied().unwrap_or(0.0);
            if v > 0.0 {
                let _ = write!(
                    o,
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{color}\"><title>{}: {}</title></rect>",
                    x(i) + cw * 0.12,
                    y(base + v),
                    cw * 0.76,
                    (y(base) - y(base + v)).max(0.5),
                    esc(et),
                    esc(&tip)
                );
                base += v;
            }
        }
        if eje.is_none() && i % cada == 0 {
            let _ = write!(o, "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"middle\" class=\"eje\">{}</text>", x(i) + cw / 2.0, alto - 14.0, esc(et));
        }
    }
    for (i, texto) in eje.unwrap_or(&[]) {
        let _ = write!(o, "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"middle\" class=\"eje\">{}</text>", x(*i) + cw / 2.0, alto - 14.0, esc(texto));
    }
    o.push_str("</svg>");
    o
}

/// Tres letras de un nombre de día.
fn dia3(d: chrono::NaiveDate) -> String {
    DIAS[d.weekday().num_days_from_monday() as usize].chars().take(3).collect()
}

/// (columnas, eje, separadores, horas por columna) del consumo hora a hora de todo el periodo hasta
/// `ahora`: una columna por hora con el % por proyecto (los `top` mayores; el resto, `resto`) y lo
/// sin atribuir. Con más de ~400 horas se juntan varias por columna.
pub fn horas_periodo(agr: &Agregado, top: &[String], ahora: NaiveDateTime) -> (Vec<Col>, Vec<(usize, String)>, Vec<usize>, usize) {
    let desde = agr.desde;
    let hasta = agr.hasta.min(ahora.date() + Duration::days(1));
    let dias: Vec<chrono::NaiveDate> = (0..(hasta - desde).num_days().max(0)).map(|i| desde + Duration::days(i)).collect();
    let mut horas: Vec<(chrono::NaiveDate, u32, BTreeMap<String, f64>)> = Vec::new();
    for d in &dias {
        let iso = d.format("%Y-%m-%d").to_string();
        for h in 0..24u32 {
            if d.and_hms_opt(h, 0, 0).is_some_and(|t| t > ahora) {
                break; // las horas que aún no han llegado no se dibujan
            }
            let mut gr: BTreeMap<String, f64> = BTreeMap::new();
            if let Some(m) = agr.horas_proy.get(&iso) {
                for (proy, arr) in m {
                    let k = if top.contains(proy) { proy.clone() } else { "resto".to_string() };
                    *gr.entry(k).or_insert(0.0) += arr[h as usize];
                }
            }
            let o = agr.otros_hora.get(&iso).map_or(0.0, |a| a[h as usize]);
            if o != 0.0 {
                gr.insert(OTROS.to_string(), o);
            }
            horas.push((*d, h, gr));
        }
    }
    let por = horas.len().div_ceil(400).max(1);
    let mut cols: Vec<Col> = Vec::new();
    for grupo in horas.chunks(por) {
        let mut gr: BTreeMap<String, f64> = BTreeMap::new();
        for (_, _, v) in grupo {
            for (k, x) in v {
                *gr.entry(k.clone()).or_insert(0.0) += x;
            }
        }
        let (d0, h0, _) = &grupo[0];
        let (d1, h1, _) = &grupo[grupo.len() - 1];
        let mut hora = format!("{} {} {:02}:00", dia3(*d0), d0.format("%d/%m"), h0);
        if por > 1 {
            let _ = write!(hora, " – {} {} {:02}:00", dia3(*d1), d1.format("%d/%m"), h1 + 1);
        }
        cols.push((hora, gr));
    }
    let corto = dias.len() <= 16; // pocos días: el nombre de cada día centrado en él y una raya entre días; si no, una fecha cada pocos días
    let cada = if corto { 1 } else { dias.len().div_ceil(16) };
    let ini = |j: usize| j * 24 / por;
    let centro = |j: usize| if corto { (ini(j) + cols.len().min(ini(j + 1)) - 1) / 2 } else { ini(j) };
    let eje = dias
        .iter()
        .enumerate()
        .filter(|(j, _)| j % cada == 0 && ini(*j) < cols.len())
        .map(|(j, d)| (centro(j), if corto { format!("{} {}", dia3(*d), d.format("%d/%m")) } else { d.format("%d/%m").to_string() }))
        .collect();
    let sep = (1..dias.len()).filter(|j| corto && ini(*j) < cols.len()).map(ini).collect();
    (cols, eje, sep, por)
}

/// Mapa de calor hora (24 columnas) por fila; la intensidad es la raíz del valor relativo al mayor.
pub fn svg_calor(filas: &[Vec<f64>], etiquetas: &[String], ancho: f64, color: &str) -> String {
    let (izq, ch) = (92.0, 22.0);
    let cw = (ancho - 92.0 - 10.0) / 24.0;
    let top = filas.iter().flat_map(|f| f.iter().copied()).fold(1e-9, f64::max);
    let alto = 20.0 + ch * filas.len() as f64 + 8.0;
    let mut o = format!("<svg viewBox=\"0 0 {} {}\" role=\"img\" class=\"grafico\"><title>Actividad por hora</title>", g(ancho), g(alto));
    for h in (0..24).step_by(3) {
        let _ = write!(o, "<text x=\"{:.1}\" y=\"12\" text-anchor=\"middle\" class=\"eje\">{h:02}h</text>", izq + h as f64 * cw + cw / 2.0);
    }
    for (r, (fila, et)) in filas.iter().zip(etiquetas).enumerate() {
        let y = 20.0 + r as f64 * ch;
        let _ = write!(o, "<text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\" class=\"eje\">{}</text>", izq - 8.0, y + ch / 2.0 + 4.0, esc(et));
        for (h, v) in fila.iter().enumerate() {
            let op = if *v <= 0.0 { 0.0 } else { 0.15 + 0.85 * (v / top).sqrt() };
            let _ = write!(
                o,
                "<rect x=\"{:.1}\" y=\"{}\" width=\"{:.1}\" height=\"{}\" rx=\"3\" fill=\"{}\" opacity=\"{:.2}\"><title>{} {h:02}:00 · {} tokens</title></rect>",
                izq + h as f64 * cw + 1.0,
                y + 1.0,
                cw - 2.0,
                ch - 2.0,
                if *v > 0.0 { color } else { "var(--pista)" },
                if *v > 0.0 { op } else { 0.45 },
                esc(et),
                tokens_txt(v * 1000.0)
            );
        }
    }
    o.push_str("</svg>");
    o
}
