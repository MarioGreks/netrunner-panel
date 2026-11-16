//! Módulo CUENTAS ATRÁS: eventos de la agenda marcados con ★ para contar el tiempo que
//! falta. Se guardan en `app_data_dir()/cuentas.json` (escritura atómica) y se ponen al
//! día en cada sincronización del calendario.
//!
//! Reconciliación (ver `reconciliar_lista`): se casa por `id_evento`; si no aparece, por
//! `uid` (solo si ese uid tiene un único evento: un evento suelto cuya hora cambió). Las
//! ocurrencias movidas de una serie conservan su id, así que no necesitan el `uid`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::calendario::Evento;

const ARCHIVO: &str = "cuentas.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cuenta {
    pub id_evento: String,
    pub uid: String,
    pub titulo: String,
    pub inicio: DateTime<FixedOffset>,
    #[serde(default)]
    pub categorias: Vec<String>,
    /// Milisegundos UTC del momento en que se marcó.
    pub creada: i64,
}

pub struct Cuentas {
    lista: Mutex<Vec<Cuenta>>,
}

// ---------------------------------------------------------------------------
// Lógica pura
// ---------------------------------------------------------------------------

fn copiar_datos(c: &mut Cuenta, e: &Evento) {
    c.id_evento = e.id.clone();
    c.titulo = e.titulo.clone();
    c.inicio = e.inicio;
    c.categorias = e.categorias.clone();
}

/// Pone al día las cuentas con los eventos recién leídos. Devuelve `true` si algo cambió.
///
/// - Se actualizan título, inicio y categorías del evento con el mismo `id_evento`.
/// - Si no aparece y su inicio aún es futuro, se busca por `uid`; solo se acepta si hay un
///   único evento con ese uid que no sea ya de otra cuenta (en una serie, varias
///   ocurrencias comparten uid y saltar a otra sería un error).
/// - Si no aparece y es futura, se conserva con sus últimos datos.
/// - Todas las cuentas cuyo inicio ya pasó se borran (la cuenta atrás terminó).
pub fn reconciliar_lista(cuentas: &mut Vec<Cuenta>, eventos: &[Evento], ahora: DateTime<Utc>) -> bool {
    let antes = cuentas.clone();
    let marcados: HashSet<String> = cuentas.iter().map(|c| c.id_evento.clone()).collect();
    let mut reclamados: HashSet<String> = HashSet::new();
    for c in cuentas.iter_mut() {
        let futura = c.inicio.with_timezone(&Utc) > ahora;
        let por_id = eventos.iter().find(|e| !e.id.is_empty() && e.id == c.id_evento);
        let hallado = por_id.or_else(|| {
            if !futura || c.uid.is_empty() {
                return None;
            }
            let mut candidatos = eventos
                .iter()
                .filter(|e| e.uid == c.uid && !marcados.contains(&e.id) && !reclamados.contains(&e.id));
            match (candidatos.next(), candidatos.next()) {
                (Some(e), None) => Some(e),
                _ => None,
            }
        });
        if let Some(e) = hallado {
            reclamados.insert(e.id.clone());
            copiar_datos(c, e);
        }
    }
    cuentas.retain(|c| c.inicio.with_timezone(&Utc) > ahora);
    *cuentas != antes
}

/// Añade (o refresca) la cuenta de un evento futuro.
fn anadir(cuentas: &mut Vec<Cuenta>, e: &Evento, ahora: DateTime<Utc>) -> Result<(), String> {
    if e.id.is_empty() {
        return Err("El evento no tiene identificador; sincroniza la agenda".into());
    }
    if e.inicio.with_timezone(&Utc) <= ahora {
        return Err("El evento ya ha empezado".into());
    }
    match cuentas.iter_mut().find(|c| c.id_evento == e.id) {
        Some(c) => copiar_datos(c, e),
        None => cuentas.push(Cuenta {
            id_evento: e.id.clone(),
            uid: e.uid.clone(),
            titulo: e.titulo.clone(),
            inicio: e.inicio,
            categorias: e.categorias.clone(),
            creada: ahora.timestamp_millis(),
        }),
    }
    Ok(())
}

/// Las futuras, ordenadas por inicio: lo que ve la interfaz.
fn futuras(cuentas: &[Cuenta], ahora: DateTime<Utc>) -> Vec<Cuenta> {
    let mut v: Vec<Cuenta> = cuentas
        .iter()
        .filter(|c| c.inicio.with_timezone(&Utc) > ahora)
        .cloned()
        .collect();
    v.sort_by(|a, b| a.inicio.cmp(&b.inicio).then(a.titulo.cmp(&b.titulo)));
    v
}

// ---------------------------------------------------------------------------
// Archivo y estado
// ---------------------------------------------------------------------------

/// Escribe en un archivo temporal contiguo y lo renombra sobre el destino.
fn escribir_atomico(ruta: &Path, cuentas: &[Cuenta]) -> Result<(), String> {
    let dir = ruta.parent().ok_or("ruta sin carpeta")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("no se pudo crear la carpeta: {e}"))?;
    let json = serde_json::to_string_pretty(cuentas).map_err(|e| e.to_string())?;
    let tmp = ruta.with_extension(format!("tmp{}", std::process::id()));
    let res = std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, ruta));
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("no se pudo escribir: {e}"));
    }
    Ok(())
}

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(ARCHIVO))
}

/// Carga el archivo; si está corrupto, lo aparta como `cuentas.corrupto.json`.
fn cargar(app: &AppHandle) -> Vec<Cuenta> {
    let Some(r) = ruta(app) else { return vec![] };
    let Ok(texto) = std::fs::read_to_string(&r) else { return vec![] };
    match serde_json::from_str::<Vec<Cuenta>>(&texto) {
        Ok(v) => v,
        Err(_) => {
            let _ = std::fs::rename(&r, r.with_file_name("cuentas.corrupto.json"));
            vec![]
        }
    }
}

fn guardar(app: &AppHandle, cuentas: &[Cuenta]) -> Result<(), String> {
    let r = ruta(app).ok_or("sin directorio de datos")?;
    escribir_atomico(&r, cuentas)
}

fn instantanea(app: &AppHandle) -> Vec<Cuenta> {
    futuras(&app.state::<Cuentas>().lista.lock().unwrap(), Utc::now())
}

fn emitir(app: &AppHandle) {
    let _ = app.emit("cuentas://estado", instantanea(app));
}

/// Aplica un cambio, lo guarda y emite el estado. Si falla al guardar, no cambia nada.
fn modificar(app: &AppHandle, cambio: impl FnOnce(&mut Vec<Cuenta>) -> Result<(), String>) -> Result<(), String> {
    {
        let c = app.state::<Cuentas>();
        let mut lista = c.lista.lock().unwrap();
        let mut nueva = lista.clone();
        cambio(&mut nueva)?;
        guardar(app, &nueva)?;
        *lista = nueva;
    }
    emitir(app);
    Ok(())
}

/// Llamada por el calendario tras cada sincronización correcta.
pub fn reconciliar(app: &AppHandle, eventos: &[Evento]) {
    {
        let c = app.state::<Cuentas>();
        let mut lista = c.lista.lock().unwrap();
        let mut nueva = lista.clone();
        if reconciliar_lista(&mut nueva, eventos, Utc::now()) && guardar(app, &nueva).is_ok() {
            *lista = nueva;
        }
    }
    emitir(app);
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn estado_cuentas(app: AppHandle) -> Vec<Cuenta> {
    instantanea(&app)
}

#[tauri::command]
pub fn marcar_cuenta(app: AppHandle, evento: Evento) -> Result<(), String> {
    modificar(&app, |l| anadir(l, &evento, Utc::now()))
}

#[tauri::command]
pub fn desmarcar_cuenta(app: AppHandle, id_evento: String) -> Result<(), String> {
    modificar(&app, |l| {
        l.retain(|c| c.id_evento != id_evento);
        Ok(())
    })
}

pub fn iniciar(app: &AppHandle) {
    app.manage(Cuentas { lista: Mutex::new(cargar(app)) });
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use chrono::TimeZone;

    fn ahora() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 16, 10, 0, 0).unwrap()
    }

    fn en(dias: i64, hora: u32) -> DateTime<FixedOffset> {
        (Utc.with_ymd_and_hms(2026, 11, 16, hora, 0, 0).unwrap() + chrono::Duration::days(dias)).fixed_offset()
    }

    fn evento(id: &str, uid: &str, titulo: &str, inicio: DateTime<FixedOffset>) -> Evento {
        Evento {
            id: id.into(),
            uid: uid.into(),
            titulo: titulo.into(),
            inicio,
            fin: inicio + chrono::Duration::hours(1),
            todo_el_dia: false,
            ubicacion: None,
            categorias: vec![],
            descripcion: None,
        }
    }

    fn cuenta(id: &str, uid: &str, titulo: &str, inicio: DateTime<FixedOffset>) -> Cuenta {
        Cuenta { id_evento: id.into(), uid: uid.into(), titulo: titulo.into(), inicio, categorias: vec![], creada: 1 }
    }

    #[test]
    fn actualiza_por_id() {
        let mut v = vec![cuenta("a|1", "a", "Viejo", en(10, 9))];
        let mut e = evento("a|1", "a", "Nuevo título", en(10, 9));
        e.categorias = vec!["TFM".into()];
        assert!(reconciliar_lista(&mut v, &[e], ahora()));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].titulo, "Nuevo título");
        assert_eq!(v[0].categorias, vec!["TFM"]);
        // Sin cambios: no hay nada que guardar
        let e2 = evento("a|1", "a", "Nuevo título", en(10, 9));
        let mut e2 = e2;
        e2.categorias = vec!["TFM".into()];
        assert!(!reconciliar_lista(&mut v, &[e2], ahora()));
    }

    #[test]
    fn actualiza_por_uid_si_el_evento_se_movio() {
        let mut v = vec![cuenta("b|1", "b", "Examen", en(10, 9))];
        let movido = evento("b|2", "b", "Examen", en(12, 15));
        assert!(reconciliar_lista(&mut v, &[movido, evento("z|1", "z", "Otro", en(3, 9))], ahora()));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id_evento, "b|2");
        assert_eq!(v[0].inicio, en(12, 15));
    }

    #[test]
    fn no_salta_entre_ocurrencias_de_una_serie() {
        // Dos ocurrencias comparten uid y ninguna tiene el id guardado: se conserva tal cual
        let mut v = vec![cuenta("s|1", "s", "Clase", en(10, 9))];
        let eventos = [evento("s|2", "s", "Clase", en(17, 9)), evento("s|3", "s", "Clase", en(24, 9))];
        assert!(!reconciliar_lista(&mut v, &eventos, ahora()));
        assert_eq!(v[0].id_evento, "s|1");
        assert_eq!(v[0].inicio, en(10, 9));
    }

    #[test]
    fn borra_las_pasadas() {
        // Pasada y ausente; pasada y presente; y una futura que se queda
        let mut v = vec![
            cuenta("p|1", "p", "Pasada ausente", en(-2, 9)),
            cuenta("q|1", "q", "Pasada presente", en(0, 8)),
            cuenta("f|1", "f", "Futura", en(5, 9)),
        ];
        let eventos = [evento("q|1", "q", "Pasada presente", en(0, 8)), evento("f|1", "f", "Futura", en(5, 9))];
        assert!(reconciliar_lista(&mut v, &eventos, ahora()));
        let ids: Vec<_> = v.iter().map(|c| c.id_evento.as_str()).collect();
        assert_eq!(ids, vec!["f|1"]);
    }

    #[test]
    fn pasada_no_resucita_con_otra_ocurrencia_por_uid() {
        let mut v = vec![cuenta("s|1", "s", "Clase", en(-1, 9))];
        assert!(reconciliar_lista(&mut v, &[evento("s|2", "s", "Clase", en(6, 9))], ahora()));
        assert!(v.is_empty());
    }

    #[test]
    fn conserva_futuras_ausentes_con_sus_ultimos_datos() {
        let mut v = vec![cuenta("g|1", "g", "Lejano", en(100, 9))];
        assert!(!reconciliar_lista(&mut v, &[], ahora()));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].titulo, "Lejano");
    }

    #[test]
    fn marcar_valida_y_no_duplica() {
        let mut v = vec![];
        assert!(anadir(&mut v, &evento("", "u", "Sin id", en(2, 9)), ahora()).is_err());
        assert!(anadir(&mut v, &evento("u|1", "u", "Pasado", en(-1, 9)), ahora()).is_err());
        anadir(&mut v, &evento("u|2", "u", "Bien", en(2, 9)), ahora()).unwrap();
        anadir(&mut v, &evento("u|2", "u", "Bien renombrado", en(2, 9)), ahora()).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].titulo, "Bien renombrado");
        assert_eq!(v[0].uid, "u");
    }

    #[test]
    fn futuras_ordenadas_y_escritura_atomica() {
        let v = vec![
            cuenta("b|1", "b", "Segunda", en(9, 9)),
            cuenta("a|1", "a", "Primera", en(2, 9)),
            cuenta("c|1", "c", "Pasada", en(-1, 9)),
        ];
        let f = futuras(&v, ahora());
        assert_eq!(f.iter().map(|c| c.titulo.as_str()).collect::<Vec<_>>(), vec!["Primera", "Segunda"]);

        let dir = std::env::temp_dir().join(format!("netrunner-cuentas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let r = dir.join("sub").join(ARCHIVO);
        escribir_atomico(&r, &v).unwrap();
        escribir_atomico(&r, &v).unwrap();
        let leidas: Vec<Cuenta> = serde_json::from_str(&std::fs::read_to_string(&r).unwrap()).unwrap();
        assert_eq!(leidas, v);
        assert_eq!(std::fs::read_dir(r.parent().unwrap()).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
