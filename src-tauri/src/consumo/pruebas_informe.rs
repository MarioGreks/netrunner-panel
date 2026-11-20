//! Pruebas de los informes HTML, portadas de la función `prueba` de `informe.py`. Archivos
//! sintéticos y carpetas temporales propias: nunca se toca `~/.claude`, Documentos ni Descargas.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};

use super::agregado::*;
use super::archivo::*;
use super::historico::*;
use super::informe::*;
use super::informe_svg::*;
use super::limites::SEMANA;
use super::reparto::OTROS;

fn local(a: i32, m: u32, d: u32, h: u32, mi: u32) -> f64 {
    Local.with_ymd_and_hms(a, m, d, h, mi, 0).unwrap().timestamp() as f64
}

fn fecha(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

fn ndt(a: i32, m: u32, d: u32, h: u32) -> NaiveDateTime {
    fecha(a, m, d).and_hms_opt(h, 0, 0).unwrap()
}

fn cerca(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("netrunner_informe_{nombre}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn fl(proy: &str, eq: &str, p: f64, tok: f64, usd: f64, seg: f64) -> FilaArch {
    FilaArch {
        proyecto: proy.into(),
        sesion: format!("ses-{proy}"),
        modelo: "opus".into(),
        tarea: "codigo".into(),
        equipo: eq.into(),
        pct: p,
        peso: 1.0,
        tokens: tok,
        coste: usd,
        seg,
    }
}

/// Un día con sus filas, % sin atribuir por hora, % por hora y proyecto, y la caché leída.
fn dia(f: Vec<FilaArch>, o: &[(usize, f64)], hp: &[(&str, usize, f64)], k: Option<[f64; 2]>) -> DiaArch {
    let mut d = DiaArch { h: vec![0.0; 24], t: vec![0.0; 24], o: vec![0.0; 24], hp: BTreeMap::new(), k, f };
    for (h, v) in o {
        d.o[*h] = *v;
    }
    for (p, h, v) in hp {
        d.hp.entry((*p).to_string()).or_insert_with(|| vec![0.0; 24])[*h] += v;
        d.t[*h] += 1.0;
    }
    // algo atribuido antes de lo sin atribuir: así el primer `o` de la historia no es «el primer salto»
    if d.f.iter().any(|f| f.pct > 0.0) {
        d.h[0] = 0.1;
    }
    d
}

fn arch(maq: &str, ini: f64, dias: Vec<(&str, DiaArch)>) -> Archivo {
    Archivo {
        maquina: maq.into(),
        inicio: ini,
        reset: ini + SEMANA,
        pct: Some(10.0),
        generado: 0,
        rep: 2,
        dias: dias.into_iter().map(|(d, x)| (d.to_string(), x)).collect(),
    }
}

/// Un SVG (o el HTML entero) con las etiquetas bien cerradas y sin `&` sueltos.
fn bien_formado(texto: &str, solo_svg: bool) -> bool {
    let mut pila: Vec<String> = Vec::new();
    let mut resto = texto;
    while let Some(i) = resto.find('<') {
        let Some(j) = resto[i..].find('>') else { return false };
        let etiqueta = &resto[i + 1..i + j];
        resto = &resto[i + j + 1..];
        if etiqueta.starts_with('!') || etiqueta.starts_with('?') {
            continue;
        }
        if let Some(c) = etiqueta.strip_prefix('/') {
            if pila.pop().as_deref() != Some(c.trim()) {
                return false;
            }
        } else if !etiqueta.ends_with('/') {
            let nombre = etiqueta.split_whitespace().next().unwrap_or("").to_string();
            if !["meta", "br"].contains(&nombre.as_str()) || solo_svg {
                if nombre == "script" || nombre == "style" {
                    // su contenido no se mira
                    let cierre = format!("</{nombre}>");
                    let Some(k) = resto.find(&cierre) else { return false };
                    resto = &resto[k + cierre.len()..];
                    continue;
                }
                pila.push(nombre);
            }
        }
    }
    pila.is_empty()
}

fn svgs(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut resto = html;
    while let Some(i) = resto.find("<svg") {
        let j = resto[i..].find("</svg>").unwrap() + 6;
        out.push(&resto[i..i + j]);
        resto = &resto[i + j..];
    }
    out
}

struct Mundo {
    a: Archivo,
    b: Archivo,
    ref_: f64,
    reset: f64,
}

/// Dos equipos y una semana (de la 31/10 16:00 a la 7/11 16:00): PCA hizo 9 %, PCB 4 % y hay 2 %
/// sin atribuir (lo calcula igual cada equipo).
fn mundo() -> Mundo {
    let reset = local(2026, 11, 7, 16, 0);
    let ini = reset - SEMANA;
    let a = arch(
        "PCA",
        ini,
        vec![
            ("2026-11-01", dia(vec![fl("web", "PCA", 6.0, 5000.0, 1.5, 0.0)], &[], &[("web", 12, 6.0)], Some([4000.0, 0.5]))),
            ("2026-11-02", dia(vec![fl("web", "PCA", 3.0, 2000.0, 0.5, 0.0)], &[], &[("web", 5, 3.0)], Some([1000.0, 0.25]))),
            ("2026-11-03", dia(vec![], &[(16, 2.0)], &[], None)),
        ],
    );
    let b = arch(
        "PCB",
        ini,
        vec![("2026-11-03", dia(vec![], &[(16, 2.0)], &[], None)), ("2026-11-04", dia(vec![fl("app", "PCB", 4.0, 3000.0, 0.0, 0.0)], &[], &[("app", 22, 4.0)], None))],
    );
    Mundo { a, b, ref_: local(2026, 11, 6, 12, 0), reset }
}

// ---------- piezas SVG ----------

#[test]
fn escapado_y_pasos() {
    assert_eq!(esc("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#x27;");
    assert!(cerca(paso_eje(0.0, 6.0), 1.0) && cerca(paso_eje(30.0, 6.0), 5.0) && cerca(paso_eje(0.45, 6.0), 0.1));
    assert!(paso_eje(1e6, 6.0) >= 1e5);
}

#[test]
fn columnas_y_calor_bien_formados() {
    let cols: Vec<Col> = vec![
        ("a <1>".into(), BTreeMap::from([("web".to_string(), 5.0), (OTROS.to_string(), 1.0)])),
        ("b".into(), BTreeMap::from([("web".to_string(), 3.0)])),
        ("c".into(), BTreeMap::new()),
    ];
    let series = vec![("web".to_string(), "#fc0".to_string()), (OTROS.to_string(), GRIS.to_string())];
    let s = svg_columnas(&cols, &series, 1040.0, 250.0, None, &[1], "T & <t>", None);
    assert!(bien_formado(&s, true) && s.contains("T &amp; &lt;t&gt;") && s.contains("a &lt;1&gt;: Sin atribuir 1,0 % · web 5,0 %") && s.contains(">0 %<"));
    let f = |v: f64, _p: f64| format!("{} €", num(v, 1));
    assert!(svg_columnas(&cols, &series, 1040.0, 250.0, None, &[], "x", Some(&f)).contains("5,0 €"));
    assert!(bien_formado(&svg_columnas(&[], &series, 1040.0, 250.0, None, &[], "vacío", None), true));
    let c = svg_calor(&[vec![0.0; 24], (0..24).map(f64::from).collect()], &["lun".into(), "<mar>".into()], 1040.0, CIAN);
    assert!(bien_formado(&c, true) && c.contains("&lt;mar&gt;") && c.contains("var(--pista)") && c.contains("23:00 · 23 k tokens"));
}

#[test]
fn horas_del_periodo() {
    let m = mundo();
    let agr = agregar(&[&m.a, &m.b], fecha(2026, 10, 31), fecha(2026, 11, 8), None);
    let ahora = ndt(2026, 11, 6, 12);
    let (cols, eje, sep, por) = horas_periodo(&agr, &["web".to_string()], ahora);
    assert_eq!(por, 1);
    assert_eq!(cols.len(), 6 * 24 + 13);
    assert_eq!(cols[0].0, "sáb 31/10 00:00");
    assert_eq!(sep, vec![24, 48, 72, 96, 120, 144]);
    assert_eq!((eje[0].0, eje[0].1.as_str()), (11, "sáb 31/10"));
    assert!(cerca(cols.iter().map(|c| c.1.values().sum::<f64>()).sum(), 15.0));
    assert!(cerca(cols[36].1["web"], 6.0));
    assert!(cols.last().unwrap().0.ends_with("06/11 12:00"));
    // un año: columnas de 19 horas
    let ano = agregar(&[&m.a], fecha(2026, 1, 1), fecha(2027, 1, 1), None);
    assert_eq!(horas_periodo(&ano, &["web".to_string()], ahora).3, 19);
}

// ---------- informe del periodo ----------

#[test]
fn informe_semanal_contenido() {
    let m = mundo();
    let agr = agregar(&[&m.a, &m.b], fecha(2026, 10, 31), fecha(2026, 11, 8), None);
    assert!(cerca(agr.total[0], 15.0));
    let gen = ndt(2026, 11, 6, 12);
    let h = informe_html(&agr, "Semana de prueba", "semana", gen, None, None);
    assert!(h.starts_with("<!doctype html>") && h.ends_with("</html>"));
    for t in ["15,0 %", "PCA, PCB", "Todas las sesiones", "Detalle por día", "Sin atribuir", "<svg", "Cómo se calcula", "Consumo hora a hora", "Cuándo trabajas con Claude", "Por tipo de tarea", "Escribir código"] {
        assert!(h.contains(t), "falta «{t}»");
    }
    assert!(!h.contains("Cada columna junta") && !h.contains("Los gráficos de la tarjeta") && !h.contains("$/mes"));
    assert!(h.contains("<span>Caché de contexto"));
    assert!(h.contains("<b>Sin atribuir.</b>") && h.contains("más de 2 días") && h.contains("<title>Consumo · Semana de prueba</title>"));
    assert!(h.contains("Orbitron") && h.contains("JetBrains Mono") && !h.contains("http://fonts") && !h.contains("https://"));
    assert!(bien_formado(&h, false), "HTML mal formado");
    for s in svgs(&h) {
        assert!(bien_formado(s, true));
    }
    assert!(svgs(&h).len() >= 2);
    // el pie y el aviso de lo sin atribuir solo salen con motivo
    assert!(!h.contains("está sin atribuir"));
}

#[test]
fn informe_largo_vacio_y_avisos() {
    let m = mundo();
    let gen = ndt(2026, 11, 6, 12);
    let agr = agregar(&[&m.a, &m.b], fecha(2026, 1, 1), fecha(2027, 1, 1), None);
    let h2 = informe_html(&agr, "Año 2026", "anio", gen, None, None);
    assert!(!h2.contains("Todas las sesiones") && h2.contains("Las 20 sesiones") && h2.contains("límites semanales") && h2.contains("Solo hay datos"));
    assert!(h2.contains("Cada columna junta 19 horas") && !h2.contains("Detalle por día") && h2.contains("Lunes"));
    // la mayor parte sin atribuir
    let mut d_o = dia(vec![], &[(16, 9.0)], &[], None);
    d_o.h[0] = 0.5; // algo atribuido antes: lo sin atribuir de después no es «el primer salto»
    let solo_o = arch("PCA", m.reset - SEMANA, vec![("2026-11-03", d_o)]);
    let a3 = agregar(&[&solo_o], fecha(2026, 10, 24), fecha(2026, 11, 8), None);
    assert!(informe_html(&a3, "Prueba", "semana", gen, None, None).contains("está sin atribuir"));
    // sin datos
    let vacio_agr = agregar::<Archivo>(&[], fecha(2026, 1, 1), fecha(2026, 2, 1), None);
    let vacio = informe_html(&vacio_agr, "Enero 2026", "mes", gen, None, None);
    assert!(vacio.contains("No hay datos guardados") && vacio.ends_with("</html>") && bien_formado(&vacio, false));
}

#[test]
fn informe_graficos_cache_y_suscripcion() {
    let m = mundo();
    let gen = ndt(2026, 11, 6, 12);
    let agr = agregar(&[&m.a, &m.b], fecha(2026, 10, 31), fecha(2026, 11, 8), None);
    let gr: BTreeMap<String, String> = [("equipos", "<svg id=\"g1\"></svg>"), ("dias", "<svg id=\"g2\"></svg>"), ("stack", "<svg id=\"g3\"></svg>")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let hg = informe_html(&agr, "Con gráficos", "semana", gen, Some(&gr), None);
    assert!(hg.contains("Los gráficos de la tarjeta") && hg.contains("id=\"g1\"") && !hg.contains("id=\"g2\"") && hg.contains("id=\"g3\"") && !hg.contains("Consumo por día") && hg.contains("Consumo hora a hora"));
    // caché: lo leído y lo que ahorra; los días sin ese dato no cuentan en el porcentaje
    assert!(cerca(agr.cache[0], 5000.0) && cerca(agr.cache[2], 7000.0) && cerca(agr.cache[1], 0.75));
    let hc = informe_html(&agr, "Caché", "semana", gen, None, Some(20.0));
    assert!(hc.contains("<span>Caché de contexto") && hc.contains("71 %") && hc.contains("solo de los días con este dato") && hc.contains("$/mes"));
    assert!(informe_html(&agr, "S", "semana", gen, None, Some(20000.0)).contains('−'));
    assert!(!informe_html(&agr, "S", "semana", gen, None, Some(0.0)).contains("$/mes"));
}

#[test]
fn informe_escapa_nombres_de_proyecto() {
    let mut m = mundo();
    m.a.dias.get_mut("2026-11-01").unwrap().f[0].proyecto = "<script>alert(1)</script>".into();
    m.a.dias.get_mut("2026-11-01").unwrap().hp.insert("<script>alert(1)</script>".into(), vec![1.0; 24]);
    let agr = agregar(&[&m.a, &m.b], fecha(2026, 10, 31), fecha(2026, 11, 8), None);
    let h = informe_html(&agr, "Semana <x>", "semana", ndt(2026, 11, 6, 12), None, None);
    assert!(!h.contains("<script>alert") && h.contains("&lt;script&gt;") && h.contains("Semana &lt;x&gt;") && !h.contains("<x>"));
    assert!(bien_formado(&h, false));
    for s in svgs(&h) {
        assert!(bien_formado(s, true));
    }
    let (_, h2) = generar(&[&m.a], "semana", m.ref_, Some(m.reset), false, None, None).unwrap();
    assert!(!h2.contains("<script>alert"));
}

#[test]
fn generar_filtra_por_semana_de_claude() {
    let m = mundo();
    // el día del reset pertenece a dos semanas: el semanal solo cuenta el archivo de su semana; el mensual, los dos
    let ant = arch(
        "PCA",
        m.reset - 2.0 * SEMANA,
        vec![("2026-11-01", dia(vec![fl("vieja", "PCA", 7.0, 100.0, 0.1, 0.0)], &[], &[], None))],
    );
    let todos = [&m.a, &m.b, &ant];
    let (nombre, h) = generar(&todos, "semana", m.ref_, Some(m.reset), false, None, None).unwrap();
    assert_eq!(nombre, "Consumo - resumen semanal 2026-10-31.html");
    assert!(h.contains("Semana del 31 de octubre") && !h.contains("vieja"));
    let (_, mes) = generar(&todos, "mes", local(2026, 11, 6, 0, 0), None, false, None, None).unwrap();
    assert!(mes.contains("vieja") && mes.contains("Noviembre 2026"));
    let (n2, h2) = generar(&todos, "semana", m.ref_, Some(m.reset), true, None, None).unwrap();
    assert_eq!(n2, "Consumo - resumen semanal 2026-10-24.html");
    assert!(h2.contains("vieja") && !h2.contains(">web<"));
    assert!(generar(&todos, "siglo", m.ref_, None, false, None, None).is_err());
}

#[test]
fn informe_desde_ficheros_en_carpeta_temporal() {
    let m = mundo();
    let d = carpeta("ficheros");
    guardar_archivo(&d, &m.a, false).unwrap();
    guardar_archivo(&d, &m.b, false).unwrap();
    let mut cache = CacheArchivos::default();
    let leidos = cache.leer_archivos_en(&[d.clone()]);
    assert_eq!(leidos.len(), 2);
    let (nombre, h) = generar(&leidos, "semana", m.ref_, Some(m.reset), false, None, Some(20.0)).unwrap();
    assert!(nombre.starts_with("Consumo - resumen semanal") && h.contains("15,0 %") && h.contains("PCA, PCB"));
    let (nh, hh) = generar_historico(&leidos, m.ref_, 18.0, None, None, None, None);
    assert!(nh.starts_with("Consumo - histórico total ") && hh.contains("Histórico total"));
    let _ = fs::remove_dir_all(&d);
}

// ---------- histórico total ----------

fn hist() -> (Archivo, Archivo, Archivo) {
    let w1 = local(2026, 10, 24, 16, 0);
    let w2 = w1 + SEMANA;
    let ha1 = arch(
        "PCA",
        w1,
        vec![
            ("2026-10-30", dia(vec![fl("web", "PCA", 6.0, 1000.0, 1.0, 0.0)], &[(3, 2.0)], &[], None)),
            ("2026-10-31", dia(vec![fl("web", "PCA", 2.0, 500.0, 0.5, 0.0)], &[], &[], None)),
        ],
    );
    let ha2 = arch(
        "PCA",
        w2 + 60.0,
        vec![
            ("2026-10-31", dia(vec![fl("web", "PCA", 3.0, 300.0, 0.3, 0.0), fl("app", "PCA", 1.0, 100.0, 0.1, 0.0)], &[(5, 0.5)], &[], None)),
            ("2026-11-02", dia(vec![fl("app", "PCA", 5.0, 2000.0, 2.0, 0.0)], &[], &[], None)),
        ],
    );
    let hb1 = arch(
        "PCB",
        w1,
        vec![("2026-10-30", dia(vec![], &[(3, 1.0)], &[], None)), ("2026-10-31", dia(vec![fl("app", "PCB", 4.0, 700.0, 0.7, 0.0)], &[(5, 1.5)], &[], None))],
    );
    (ha1, ha2, hb1)
}

#[test]
fn historico_total_contenido() {
    let (ha1, ha2, hb1) = hist();
    let hh = agregar_historico(&[&hb1, &ha2, &ha1], None, None, None);
    let ht = historico_html(&hh, ndt(2026, 11, 14, 9), 18.0, None);
    assert!(ht.starts_with("<!doctype html>") && ht.ends_with("</html>") && bien_formado(&ht, false));
    for t in ["Histórico total", "30/10/2026 al 02/11/2026", "2 semanas", "PCA, PCB", "data-bg=\"tarea\"", "data-bu=\"eur\"", "data-bu=\"tokens\"", "Coste a precio de API", "Coste real (€) por tipo de tarea", "ajuste «suscripcion_eur»"] {
        assert!(ht.contains(t), "falta «{t}»");
    }
    for g in ["proyecto", "equipo", "modelo", "tarea"] {
        for u in ["eur", "dias", "usd", "tokens", "horas", "eur_h", "pct_h", "tok_h"] {
            assert!(ht.contains(&format!("data-g=\"{g}\" data-u=\"{u}\"")), "falta la vista {g}/{u}");
        }
    }
    assert_eq!(ht.matches("<section class=\"vista\"").count(), 32);
    assert_eq!(ht.matches(" hidden>").count(), 31);
    assert!(ht.contains("<section class=\"vista\" data-g=\"proyecto\" data-u=\"eur\">"));
    // días de límite (% × 7 / 100): total 25 % = 1,75 días
    assert!(ht.contains("<b>1,8 días</b>") && ht.contains("= 25,0 % del límite semanal"));
    assert!(ht.contains("4,60 $") && ht.contains("<b>5 k</b>") && ht.contains("0,8 días") && ht.contains("0,7 días") && ht.contains("0,3 días"));
    let trozo = |a: &str, b: &str| {
        let i = ht.find(a).unwrap();
        &ht[i..i + ht[i..].find(b).unwrap()]
    };
    let usd = trozo("data-g=\"proyecto\" data-u=\"usd\"", "data-g=\"proyecto\" data-u=\"tokens\"");
    assert!(!usd.contains("Sin atribuir") && usd.contains("<svg"));
    assert!(trozo("data-g=\"proyecto\" data-u=\"dias\"", "data-g=\"proyecto\" data-u=\"usd\"").contains("Sin atribuir"));
    assert!(!trozo("data-g=\"tarea\" data-u=\"usd\"", "data-g=\"tarea\" data-u=\"tokens\"").contains("Sin atribuir"));
    // coste real en €: 25 % con 18 €/mes
    assert!(ht.contains("<b>1,04 €</b>") && ht.contains("18 €/mes; 100 % semanal = 4,15 €"));
    let eur = trozo("data-g=\"proyecto\" data-u=\"eur\"", "data-g=\"proyecto\" data-u=\"dias\"");
    assert!(eur.contains("Sin atribuir") && eur.contains("0,17 €") && eur.contains("0,46 €"));
    assert!(ht[ht.find("data-g=\"modelo\" data-u=\"dias\"").unwrap()..].contains("Opus") && ht.contains("Escribir código"));
    for s in svgs(&ht) {
        assert!(bien_formado(s, true));
    }
    // un cociente sin horas no se dibuja; con horas, la tabla y sin gráfico de columnas
    let ph = trozo("data-g=\"proyecto\" data-u=\"pct_h\"", "data-g=\"proyecto\" data-u=\"tok_h\"");
    assert!(ph.contains("no se apila") && !ph.contains("<svg") && !ph.contains(">web<"));
    assert!(ph.contains("<th class=\"n\">Horas</th>"));
    // un solo equipo y una semana
    assert!(!historico_html(&agregar_historico(&[&ha1], None, None, None), ndt(2026, 11, 14, 9), 18.0, None).is_empty());
}

#[test]
fn historico_con_tiempo_activo_y_cocientes() {
    let w1 = local(2026, 10, 24, 16, 0);
    let hr = arch(
        "PCR",
        w1,
        vec![
            ("2026-10-30", dia(vec![fl("web", "PCR", 6.0, 1000.0, 1.0, 3600.0), fl("app", "PCR", 2.0, 400.0, 0.4, 0.0)], &[], &[], None)),
            ("2026-10-31", dia(vec![fl("web", "PCR", 4.0, 500.0, 0.5, 3600.0)], &[], &[], None)),
        ],
    );
    let h = historico_html(&agregar_historico(&[&hr], None, None, None), ndt(2026, 11, 14, 9), 18.0, None);
    let i = h.find("data-g=\"proyecto\" data-u=\"pct_h\"").unwrap();
    let v = &h[i..i + h[i..].find("data-g=\"proyecto\" data-u=\"tok_h\"").unwrap()];
    assert!(v.contains("5,00 %/h") && !v.contains(">app<") && !v.contains("<svg") && v.contains("2,0 h"));
    assert!(h.contains("<b>2,0 h</b>"));
    let j = h.find("data-g=\"proyecto\" data-u=\"horas\"").unwrap();
    let vh = &h[j..j + h[j..].find("data-g=\"proyecto\" data-u=\"eur_h\"").unwrap()];
    assert!(vh.contains("<svg") && vh.contains("<th class=\"n\">Horas</th>"));
}

#[test]
fn historico_nombres_periodos_y_vacio() {
    let (ha1, ha2, hb1) = hist();
    let ahora = local(2026, 11, 14, 9, 30);
    let (nh, vh) = generar_historico::<Archivo>(&[], ahora, 18.0, None, None, None, None);
    assert_eq!(nh, "Consumo - histórico total 2026-11-14.html");
    assert!(vh.contains("No hay datos") && vh.contains("todavía") && vh.ends_with("</html>"));
    assert_eq!(generar_historico(&[&ha1, &ha2, &hb1], ahora, 18.0, None, None, None, None).0, nh);
    let titulo = "Del 31 de octubre de 2026 al 31 de octubre de 2026";
    let (np, vp) = generar_historico(&[&hb1, &ha2, &ha1], ahora, 30.0, Some(fecha(2026, 10, 31)), Some(fecha(2026, 11, 1)), None, Some(titulo));
    assert_eq!(np, "Consumo - histórico Del 31 de octubre de 2026 al 31 de octubre de 2026 2026-11-14.html");
    assert!(vp.contains("Histórico · Del 31 de octubre") && vp.contains("30 €/mes") && vp.contains("del 31/10/2026 al 31/10/2026") && vp.contains("2 semanas") && vp.contains("<b>0,8 días</b>"));
    assert_eq!(vp.matches("<section class=\"vista\"").count(), 32);
    let (nv, vv) = generar_historico(&[&ha1], ahora, 18.0, Some(fecha(2030, 1, 1)), None, None, Some("Desde el 1 de enero de 2030"));
    assert!(vv.contains("en ese periodo") && nv.contains("Desde el 1 de enero de 2030"));
    assert!(generar_historico(&[&ha1], 1.0e9, 18.0, None, None, None, Some("Todo el histórico")).1.contains("Histórico total"));
    // el nombre del fichero no lleva nada raro aunque el título sí
    let (n3, _) = generar_historico(&[&ha1], ahora, 18.0, None, None, None, Some("a/b\\c:*? (d)\t e"));
    assert_eq!(n3, "Consumo - histórico abc d e 2026-11-14.html");
}

#[test]
fn historico_escapa_html() {
    let w1 = local(2026, 10, 24, 16, 0);
    let x = arch("X<y>", w1, vec![("2026-10-30", dia(vec![fl("<script>alert(1)</script>", "X<y>", 1.0, 1.0, 0.1, 0.0)], &[], &[], None))]);
    let h = historico_html(&agregar_historico(&[&x], None, None, None), ndt(2026, 11, 14, 9), 18.0, None);
    assert!(!h.contains("<script>alert") && h.contains("&lt;script&gt;") && !h.contains("Sin dato") && bien_formado(&h, false));
}

#[test]
fn colores_estables_por_proyecto() {
    assert_eq!(color("web"), color("web"));
    assert_eq!(color(OTROS), GRIS);
    assert_eq!(color("resto"), RESTO);
    // el mismo color que en la ventana (FNV-1a sobre el nombre)
    assert_eq!(color("netrunner-panel"), color("netrunner-panel"));
    let distintos: std::collections::BTreeSet<_> = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"].iter().map(|k| color(k)).collect();
    assert!(distintos.len() >= 4);
}

// ---------- humo con datos reales: solo lectura ----------

/// `cargo test humo_informe_real -- --ignored --nocapture`: lee las carpetas `historial` reales y
/// genera EN MEMORIA el informe semanal y el histórico total. No guarda nada en Descargas ni abre
/// nada.
#[test]
#[ignore]
fn humo_informe_real() {
    use super::equipos::Equipos;
    let t0 = std::time::Instant::now();
    let home = PathBuf::from(std::env::var_os("USERPROFILE").expect("USERPROFILE"));
    let barra = home.join(".claude").join("statusbar");
    let eq = Equipos::real(&barra, &home);
    let carpetas = super::disco::carpetas_historial(&eq.activas(), &barra);
    let mut cache = CacheArchivos::default();
    let archivos = cache.leer_archivos_en(&carpetas);
    let usd = super::ajustes::leer_suscripcion_usd(&barra.join(super::ajustes::FICHERO_AJUSTES));
    let eur = super::ajustes::leer_suscripcion(&barra.join(super::ajustes::FICHERO_AJUSTES));
    let ahora = super::ahora_s();
    // la semana vigente: el reset más próximo de los archivos (sin usage.json: solo lectura de lo archivado)
    let reset = archivos.iter().map(|a| a.reset).filter(|r| *r > ahora).fold(f64::MAX, f64::min);
    let reset = (reset != f64::MAX).then_some(reset);
    println!("archivos: {} (carpetas: {carpetas:?}); reset {reset:?}", archivos.len());
    let t1 = std::time::Instant::now();
    let (nombre, html) = generar(&archivos, "semana", ahora, reset, false, None, Some(usd)).unwrap();
    println!("semanal: {nombre}: {} bytes ({:.1?}), {} svg", html.len(), t1.elapsed(), svgs(&html).len());
    assert!(bien_formado(&html, false));
    let t2 = std::time::Instant::now();
    let (nombre_h, html_h) = generar_historico(&archivos, ahora, eur, None, None, None, None);
    let h = agregar_historico(&archivos, None, None, None);
    println!(
        "histórico: {nombre_h}: {} bytes ({:.1?}), {} proyectos, {} equipos, {} semanas",
        html_h.len(),
        t2.elapsed(),
        h.grupos[0].len(),
        h.grupos[1].len(),
        h.semanas.len()
    );
    assert!(bien_formado(&html_h, false));
    println!("tiempo total: {:.1?}", t0.elapsed());
}
