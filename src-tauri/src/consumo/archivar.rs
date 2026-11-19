//! Archivar tras cada ciclo del detalle (port de `archivar` de `consumo.pyw`): guarda el archivo
//! de la semana actual y el de la anterior solo si cambiaron, y reparte una vez las semanas
//! cerradas que aún se pueden archivar (ver `archivo.rs` para el formato y la lectura).

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use serde_json::Value;

use super::archivo::{
    a_json, construir_archivo, debe_guardar, escribir_json, firma_semana, nombre_archivo, puentear_archivos, resets_archivables, CacheArchivos,
    FirmaSemana,
};
use super::limites::{Muestra, SEMANA};
use super::reparto::{repartir, CuboEq};
use super::transcripciones::{Clave, Cubo, RETENER};

/// Como mucho cada cuánto se archiva (s).
const CADA_ARCHIVO: f64 = 300.0;


/// Lo que cuesta calcular y solo se calcula si hay algo que archivar (`Lector::costes/cache/tiempos`).
#[derive(Default)]
pub struct Medidas {
    pub costes: HashMap<Clave, f64>,
    pub cache: HashMap<Clave, (f64, f64)>,
    pub tiempos: HashMap<Clave, f64>,
}

/// Lo que se sabe tras un ciclo del detalle.
pub struct Entrada<'a> {
    pub yo: &'a str,
    /// Fin de la semana vigente y % semanal de ahora.
    pub reset: f64,
    pub pct: Option<f64>,
    /// Los cubos de este equipo.
    pub propios: &'a [Cubo],
    /// Los de todos los equipos, unidos y ordenados (`unir_cubos`).
    pub todos: &'a [&'a CuboEq],
    /// El historial del % semanal fusionado de todos los equipos.
    pub todas: &'a [Muestra],
}

/// Recuerda qué se archivó ya: solo se rehace si cambió algo.
#[derive(Default)]
pub struct EstadoArchivo {
    t_archivo: f64,
    firma_entrada: Option<u64>,
    /// Firma con que se quedó bloqueado (Consumo.exe abierto): antes de recalcular se mira solo si ya se puede.
    firma_bloqueada: Option<u64>,
    /// Firma de cada semana cerrada ya repartida (clave: el reset).
    firmas_viejas: HashMap<i64, FirmaSemana>,
    firma_puente: Option<u64>,
}

/// Qué pasó al archivar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResultadoArchivo {
    /// Ficheros escritos (archivos semanales).
    pub escritos: usize,
    /// Ficheros copiados de una carpeta a otra (puente).
    pub puenteados: usize,
    /// Había algo que escribir y no se pudo (Consumo.exe abierto o un fallo): se reintenta.
    pub pendiente: bool,
}

fn hash_entrada(e: &Entrada, carpetas: &[PathBuf]) -> u64 {
    let mut h = DefaultHasher::new();
    e.yo.hash(&mut h);
    e.reset.to_bits().hash(&mut h);
    e.pct.map(f64::to_bits).hash(&mut h);
    for c in e.propios {
        (c.inicio, &c.proyecto, &c.sesion, c.modelo, c.peso, c.tokens, c.tarea).hash(&mut h);
    }
    for c in e.todos {
        (c.inicio.to_bits(), &c.proyecto, &c.sesion, &c.modelo, c.peso.to_bits(), c.tokens.to_bits(), &c.tarea, &c.equipo).hash(&mut h);
    }
    for m in e.todas {
        (m.0.to_bits(), m.1.to_bits(), m.2.to_bits()).hash(&mut h);
    }
    carpetas.hash(&mut h);
    h.finish()
}

struct Escritura {
    nombre: String,
    json: Value,
    proteger: bool,
}

/// Guarda en el historial el archivo de la semana actual y el de la anterior solo si cambiaron,
/// como mucho cada 5 min (salvo `forzar`). Además, cada semana cerrada más vieja que empiece dentro
/// de lo que retienen los cubos (35 días, `resets_archivables`) si este equipo tiene cubos de
/// ella: se reparte una vez (al arrancar) y solo otra vez si cambia su firma; nunca empeora un
/// archivo que ya lleva más (`proteger`). Con más de una carpeta, pasa de una a otra lo de los demás.
///
/// `permitido` se pregunta solo si hay algo que escribir (con él se comprueba que Consumo.exe no
/// esté abierto: él ya archiva).
#[allow(clippy::too_many_arguments)]
pub fn archivar(
    estado: &mut EstadoArchivo,
    cache: &mut CacheArchivos,
    e: &Entrada,
    carpetas: &[PathBuf],
    ahora: f64,
    forzar: bool,
    medidas: impl FnOnce() -> Medidas,
    permitido: &mut dyn FnMut() -> bool,
) -> ResultadoArchivo {
    let mut res = ResultadoArchivo::default();
    if carpetas.is_empty() {
        return res;
    }
    if forzar || ahora - estado.t_archivo >= CADA_ARCHIVO {
        let firma = hash_entrada(e, carpetas);
        let bloqueado_aun = estado.firma_bloqueada == Some(firma) && !permitido();
        if (forzar || estado.firma_entrada != Some(firma)) && !bloqueado_aun {
            estado.t_archivo = ahora;
            estado.firma_bloqueada = None;
            archivar_semanas(estado, e, carpetas, ahora, firma, medidas(), permitido, &mut res);
        } else if bloqueado_aun {
            res.pendiente = true;
        }
    }
    if carpetas.len() >= 2 {
        let f = cache.firma(carpetas);
        if estado.firma_puente != Some(f) {
            let (n, bloqueado) = puentear_archivos(cache, carpetas, e.yo, permitido);
            res.puenteados = n;
            res.pendiente |= bloqueado;
            estado.firma_puente = (n == 0 && !bloqueado).then_some(f);
        }
    }
    res
}

#[allow(clippy::too_many_arguments)]
fn archivar_semanas(
    estado: &mut EstadoArchivo,
    e: &Entrada,
    carpetas: &[PathBuf],
    ahora: f64,
    firma: u64,
    m: Medidas,
    permitido: &mut dyn FnMut() -> bool,
    res: &mut ResultadoArchivo,
) {
    let finales = |reset: f64| e.todas.iter().rfind(|x| (x.1 - reset).abs() < 600.0).map(|x| x.2);
    let mut pend: Vec<Escritura> = Vec::new();
    // la semana actual y la anterior
    for (reset, actual) in [(e.reset, true), (e.reset - SEMANA, false)] {
        let filas = repartir(e.todas, reset, e.todos);
        let pct = if actual { e.pct } else { finales(reset) };
        let arch = construir_archivo(e.yo, reset, &filas, e.propios, &m.costes, pct, Some(ahora), Some(&m.cache), Some(&m.tiempos));
        if !arch.dias.is_empty() {
            pend.push(Escritura { nombre: nombre_archivo(&arch), json: a_json(&arch), proteger: false });
        }
    }
    // las cerradas dentro de lo que retienen los cubos
    let mut hechas: Vec<(i64, FirmaSemana)> = Vec::new();
    for reset in resets_archivables(e.todas, e.reset, ahora, RETENER) {
        let f = firma_semana(reset, e.todos, e.todas);
        if estado.firmas_viejas.get(&(reset as i64)) == Some(&f) {
            continue;
        }
        // si este equipo no hizo nada esa semana, no hay nada suyo que archivar
        if e.propios.iter().any(|c| reset - SEMANA <= c.inicio as f64 && (c.inicio as f64) < reset) {
            let filas = repartir(e.todas, reset, e.todos);
            let arch = construir_archivo(e.yo, reset, &filas, e.propios, &m.costes, finales(reset), Some(ahora), Some(&m.cache), Some(&m.tiempos));
            if !arch.dias.is_empty() {
                pend.push(Escritura { nombre: nombre_archivo(&arch), json: a_json(&arch), proteger: true });
            }
        }
        hechas.push((reset as i64, f));
    }
    // qué hay que escribir de verdad (solo lo que cambió) y dónde
    let mut tareas: Vec<(&PathBuf, &Escritura)> = Vec::new();
    for w in &pend {
        for c in carpetas {
            if debe_guardar(c, &w.nombre, &w.json, w.proteger) {
                tareas.push((c, w));
            }
        }
    }
    if !tareas.is_empty() {
        if !permitido() {
            estado.firma_bloqueada = Some(firma);
            res.pendiente = true;
            return;
        }
        for (c, w) in tareas {
            match escribir_json(c, &w.nombre, &w.json) {
                Ok(()) => res.escritos += 1,
                Err(_) => res.pendiente = true, // OneDrive a medias: se reintenta
            }
        }
    }
    if !res.pendiente {
        estado.firma_entrada = Some(firma);
        estado.firmas_viejas.extend(hechas);
    }
}

#[cfg(test)]
impl EstadoArchivo {
    pub fn firmas_viejas(&self) -> usize {
        self.firmas_viejas.len()
    }
}
