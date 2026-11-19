//! Serie del historial para la gráfica «Ritmo de la semana» de la ventana: las muestras (t, %)
//! de la semana (y de la ventana de 5 h) vigentes, sacadas del historial fusionado que ya está en
//! memoria. No lee ficheros.

use serde::Serialize;

use super::limites::{Muestra, Ventana, CINCO_HORAS, SEMANA};

/// Tolerancia (s) al emparejar el `resetsAt` de una muestra con el de la ventana (como Consumo).
const TOLERANCIA_RESET: f64 = 600.0;

/// Una ventana con sus muestras: `muestras` son `[t, %]` (t en s desde la época), solo donde el %
/// sube, de menor a mayor t.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerieVentana {
    pub inicio: f64,
    pub reset: f64,
    pub pct: f64,
    pub muestras: Vec<(f64, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerieConsumo {
    pub semanal: Option<SerieVentana>,
    pub cinco_horas: Option<SerieVentana>,
}

/// Muestras de la ventana que acaba en `reset` (de `largo` s): solo las de esa ventana, hasta
/// `ahora`, y solo cuando el % sube (como `serie_semana` de Consumo; el (inicio, 0) lo pone la interfaz).
pub fn serie_ventana(hist: &[Muestra], v: &Ventana, largo: f64, ahora: f64) -> Option<SerieVentana> {
    let reset = v.resets_at?;
    let inicio = reset - largo;
    let mut ordenadas: Vec<&Muestra> = hist
        .iter()
        .filter(|m| (m.1 - reset).abs() < TOLERANCIA_RESET && m.0 >= inicio && m.0 <= ahora)
        .collect();
    ordenadas.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut previo = 0.0;
    let mut muestras = Vec::new();
    for m in ordenadas {
        if m.2 > previo {
            muestras.push((m.0, m.2));
            previo = m.2;
        }
    }
    Some(SerieVentana { inicio, reset, pct: v.pct, muestras })
}

/// La serie de las dos ventanas con los límites ya renovados a `ahora`.
pub fn calcular_serie(
    semanal: Option<&Ventana>,
    cinco: Option<&Ventana>,
    hist: &[Muestra],
    hist5: &[Muestra],
    ahora: f64,
) -> SerieConsumo {
    SerieConsumo {
        semanal: semanal.and_then(|v| serie_ventana(hist, v, SEMANA, ahora)),
        cinco_horas: cinco.and_then(|v| serie_ventana(hist5, v, CINCO_HORAS, ahora)),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn ven(pct: f64, reset: f64) -> Ventana {
        Ventana { pct, resets_at: Some(reset) }
    }

    #[test]
    fn solo_la_semana_vigente_y_solo_cuando_sube() {
        let reset = 10_000_000.0;
        let ini = reset - SEMANA;
        let hist: Vec<Muestra> = vec![
            (ini - 100.0, reset, 3.0),          // antes del inicio: fuera
            (ini + 3000.0, reset + 100.0, 5.0), // reset dentro de la tolerancia: vale
            (ini + 1000.0, reset, 2.0),
            (ini + 4000.0, reset, 4.0), // baja respecto a la anterior: se descarta
            (ini + 5000.0, reset, 9.0),
            (ini + 6000.0, reset - SEMANA, 50.0), // otra semana: fuera
            (reset + 500.0, reset, 99.0),         // del futuro: fuera
        ];
        let s = serie_ventana(&hist, &ven(9.0, reset), SEMANA, reset - 1.0).unwrap();
        assert_eq!(s.inicio, ini);
        assert_eq!(s.muestras, vec![(ini + 1000.0, 2.0), (ini + 3000.0, 5.0), (ini + 5000.0, 9.0)]);
    }

    #[test]
    fn sin_reset_no_hay_serie_y_la_de_cinco_horas_usa_su_largo() {
        assert_eq!(serie_ventana(&[], &Ventana { pct: 1.0, resets_at: None }, SEMANA, 0.0), None);
        let reset = 1_000_000.0;
        let h5: Vec<Muestra> = vec![(reset - CINCO_HORAS + 60.0, reset, 7.0)];
        let s = calcular_serie(None, Some(&ven(7.0, reset)), &[], &h5, reset - 10.0);
        assert!(s.semanal.is_none());
        let c = s.cinco_horas.unwrap();
        assert_eq!((c.inicio, c.muestras.len()), (reset - CINCO_HORAS, 1));
    }
}
