//! COLA DE PENDIENTES con Outlook: creaciones y borrados de eventos hechos desde el panel.
//!
//! La agenda sale de un ICS de solo lectura; el único calendario que el panel puede modificar
//! es el del Outlook clásico abierto (COM). Para que el panel responda al instante aunque
//! Outlook esté cerrado, cada acción se apunta aquí y se superpone a lo que dice el ICS:
//!
//! - **Crear**: el evento aparece en la agenda al momento (marcado `pendiente`) y se crea en
//!   Outlook en cuanto está abierto.
//! - **Borrar**: el evento desaparece de la agenda al momento y se borra de Outlook en cuanto
//!   está abierto.
//!
//! Una operación enviada a Outlook se queda en la cola (con `enviada`) hasta que el ICS
//! publicado la refleja, o hasta 30 minutos: así un borrado no «resucita» ni una creación
//! desaparece mientras el ICS se pone al día. La cola se guarda en
//! `app_data_dir()/pendientes_outlook.json` y sobrevive a reinicios.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::calendario::Evento;
use crate::outlook::{validar_borrado, validar_evento, NuevoEvento};

/// Prefijo del `id` de los eventos que solo existen en la cola (aún no los publica el ICS).
pub const PREFIJO_ID: &str = "pendiente|";
const PREFIJO_UID: &str = "pendiente-";
/// Cuánto se sigue superponiendo una operación ya enviada si el ICS no la refleja.
const MINUTOS_CONFIRMACION: i64 = 30;
/// Fallos seguidos (con Outlook abierto) tras los cuales se descarta una operación.
pub const MAX_INTENTOS: u32 = 5;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Operacion {
    Crear {
        id: String,
        evento: NuevoEvento,
        #[serde(default)]
        enviada: Option<DateTime<Utc>>,
        #[serde(default)]
        intentos: u32,
    },
    Borrar {
        evento: Evento,
        #[serde(default)]
        enviada: Option<DateTime<Utc>>,
        #[serde(default)]
        intentos: u32,
    },
}

impl Operacion {
    /// Identifica la operación dentro de la cola.
    pub fn clave(&self) -> String {
        match self {
            Operacion::Crear { id, .. } => format!("c:{id}"),
            Operacion::Borrar { evento, .. } => format!("b:{}", evento.id),
        }
    }

    pub fn enviada(&self) -> Option<DateTime<Utc>> {
        match self {
            Operacion::Crear { enviada, .. } | Operacion::Borrar { enviada, .. } => *enviada,
        }
    }

    pub fn titulo(&self) -> &str {
        match self {
            Operacion::Crear { evento, .. } => evento.titulo.trim(),
            Operacion::Borrar { evento, .. } => evento.titulo.trim(),
        }
    }

    fn campos(&mut self) -> (&mut Option<DateTime<Utc>>, &mut u32) {
        match self {
            Operacion::Crear { enviada, intentos, .. } | Operacion::Borrar { enviada, intentos, .. } => {
                (enviada, intentos)
            }
        }
    }
}

pub struct Pendientes {
    cola: Mutex<Vec<Operacion>>,
}

/// Se emite como `pendientes://fallo` cuando una operación no se pudo llevar a Outlook.
#[derive(Serialize, Clone, Debug)]
pub struct Fallo {
    pub titulo: String,
    pub mensaje: String,
}

// ---------------------------------------------------------------------------
// Lógica pura
// ---------------------------------------------------------------------------

/// Mismo evento por sus datos: título, tipo de día y comienzo y fin (con 1 min de margen).
fn mismo_evento(a: &Evento, b: &Evento) -> bool {
    let cerca = |x: NaiveDateTime, y: NaiveDateTime| (x - y).num_seconds().abs() < 60;
    a.titulo.trim().to_lowercase() == b.titulo.trim().to_lowercase()
        && a.todo_el_dia == b.todo_el_dia
        && cerca(a.inicio.naive_local(), b.inicio.naive_local())
        && cerca(a.fin.naive_local(), b.fin.naive_local())
}

/// ¿El borrado pendiente `borrado` afecta a `e`? Por id; y, si el borrado es de un evento que
/// solo existía en la cola, también por datos (el ICS le pondrá otro id al publicarlo).
fn oculta(borrado: &Evento, e: &Evento) -> bool {
    e.id == borrado.id || (borrado.pendiente && mismo_evento(e, borrado))
}

/// El evento que se ve en la agenda mientras una creación espera a Outlook o al ICS.
pub fn evento_de_creacion(id: &str, n: &NuevoEvento) -> Option<Evento> {
    let (ini, fin) = validar_evento(n).ok()?;
    let local = |x: NaiveDateTime| Local.from_local_datetime(&x).earliest().map(|d| d.fixed_offset());
    Some(Evento {
        id: format!("{PREFIJO_ID}{id}"),
        uid: format!("{PREFIJO_UID}{id}"),
        titulo: n.titulo.trim().to_string(),
        inicio: local(ini)?,
        fin: local(fin)?,
        todo_el_dia: n.todo_el_dia,
        ubicacion: n.ubicacion.as_deref().map(str::trim).filter(|u| !u.is_empty()).map(String::from),
        categorias: n.categorias.clone(),
        descripcion: None,
        recurrente: false,
        pendiente: true,
    })
}

/// Lo que ve el usuario: los eventos del ICS sin los que se están borrando y con los que se
/// están creando (si el ICS aún no los trae), ordenados como en `calendario::interpretar`.
pub fn superponer(ics: &[Evento], cola: &[Operacion]) -> Vec<Evento> {
    let borrados: Vec<&Evento> = cola
        .iter()
        .filter_map(|o| match o {
            Operacion::Borrar { evento, .. } => Some(evento),
            _ => None,
        })
        .collect();
    let oculto = |e: &Evento| borrados.iter().any(|b| oculta(b, e));
    let mut v: Vec<Evento> = ics.iter().filter(|e| !oculto(e)).cloned().collect();
    for op in cola {
        if let Operacion::Crear { id, evento, .. } = op {
            if let Some(e) = evento_de_creacion(id, evento) {
                if !ics.iter().any(|x| mismo_evento(x, &e)) && !oculto(&e) {
                    v.push(e);
                }
            }
        }
    }
    v.sort_by(|a, b| {
        a.inicio
            .cmp(&b.inicio)
            .then(b.todo_el_dia.cmp(&a.todo_el_dia))
            .then(a.titulo.cmp(&b.titulo))
    });
    v
}

/// Quita de la cola las operaciones ya enviadas que el ICS refleja (la creación aparece, el
/// borrado ya no aparece) o que llevan más de 30 minutos. Devuelve `true` si cambió algo.
pub fn limpiar(cola: &mut Vec<Operacion>, ics: &[Evento], ahora: DateTime<Utc>) -> bool {
    let antes = cola.len();
    let vencida = |t: &DateTime<Utc>| ahora - *t >= Duration::minutes(MINUTOS_CONFIRMACION);
    cola.retain(|op| match op {
        Operacion::Crear { id, evento, enviada: Some(t), .. } => {
            let publicada = evento_de_creacion(id, evento).is_some_and(|e| ics.iter().any(|x| mismo_evento(x, &e)));
            !(publicada || vencida(t))
        }
        Operacion::Borrar { evento, enviada: Some(t), .. } => {
            ics.iter().any(|e| oculta(evento, e)) && !vencida(t)
        }
        _ => true,
    });
    cola.len() != antes
}

#[derive(Debug, PartialEq, Eq)]
pub enum AccionBorrado {
    /// Era un evento creado desde el panel que aún no se había enviado: se cancela la creación.
    CreacionCancelada,
    Encolado,
    YaEstabaEncolado,
}

/// Apunta el borrado de un evento de la agenda.
pub fn encolar_borrado(cola: &mut Vec<Operacion>, ev: &Evento) -> AccionBorrado {
    if let Some(id) = ev.id.strip_prefix(PREFIJO_ID) {
        let pos = cola
            .iter()
            .position(|o| matches!(o, Operacion::Crear { id: i, enviada: None, .. } if i == id));
        if let Some(pos) = pos {
            cola.remove(pos);
            return AccionBorrado::CreacionCancelada;
        }
    }
    if cola.iter().any(|o| matches!(o, Operacion::Borrar { evento, .. } if evento.id == ev.id)) {
        return AccionBorrado::YaEstabaEncolado;
    }
    cola.push(Operacion::Borrar { evento: ev.clone(), enviada: None, intentos: 0 });
    AccionBorrado::Encolado
}

/// Marca una operación como enviada a Outlook.
pub fn marcar_enviada(cola: &mut [Operacion], clave: &str, ahora: DateTime<Utc>) -> bool {
    match cola.iter_mut().find(|o| o.clave() == clave) {
        Some(op) => {
            *op.campos().0 = Some(ahora);
            true
        }
        None => false,
    }
}

/// Cuenta un fallo con Outlook abierto. Si se agotan los intentos, saca la operación de la
/// cola y la devuelve.
pub fn sumar_intento(cola: &mut Vec<Operacion>, clave: &str) -> Option<Operacion> {
    let pos = cola.iter().position(|o| o.clave() == clave)?;
    let n = cola[pos].campos().1;
    *n += 1;
    if *n >= MAX_INTENTOS {
        Some(cola.remove(pos))
    } else {
        None
    }
}

pub fn sin_enviar(cola: &[Operacion]) -> usize {
    cola.iter().filter(|o| o.enviada().is_none()).count()
}

// ---------------------------------------------------------------------------
// Estado, disco y comandos
// ---------------------------------------------------------------------------

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("pendientes_outlook.json"))
}

fn cargar(app: &AppHandle) -> Vec<Operacion> {
    ruta(app)
        .and_then(|r| std::fs::read_to_string(r).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn guardar(app: &AppHandle, cola: &[Operacion]) {
    let Some(r) = ruta(app) else { return };
    if let Some(dir) = r.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(json) = serde_json::to_string(cola) else { return };
    // Se escribe a un temporal y se renombra: un corte no deja el archivo a medias
    let tmp = r.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &r);
    }
}

pub fn iniciar(app: &AppHandle) {
    app.manage(Pendientes { cola: Mutex::new(cargar(app)) });
}

/// Copia de la cola (vacía si el módulo aún no arrancó).
pub fn instantanea(app: &AppHandle) -> Vec<Operacion> {
    app.try_state::<Pendientes>().map(|p| p.cola.lock().unwrap().clone()).unwrap_or_default()
}

/// Cambia la cola, la guarda y refresca la agenda con el resultado.
fn modificar<T>(app: &AppHandle, cambio: impl FnOnce(&mut Vec<Operacion>) -> T) -> Option<T> {
    let p = app.try_state::<Pendientes>()?;
    let r = {
        let mut cola = p.cola.lock().unwrap();
        let r = cambio(&mut cola);
        guardar(app, &cola);
        r
    };
    crate::calendario::refrescar_vista(app);
    Some(r)
}

static CONTADOR: AtomicU64 = AtomicU64::new(0);

fn id_nuevo() -> String {
    format!("{}-{}", Utc::now().timestamp_millis(), CONTADOR.fetch_add(1, Ordering::Relaxed))
}

/// Apunta la creación de un evento: aparece en la agenda al instante.
pub fn anadir_creacion(app: &AppHandle, evento: NuevoEvento) -> Result<(), String> {
    validar_evento(&evento)?;
    let id = id_nuevo();
    modificar(app, |c| c.push(Operacion::Crear { id, evento, enviada: None, intentos: 0 }))
        .ok_or_else(|| "La cola de Outlook no está activa".to_string())
}

/// Apunta el borrado de un evento: desaparece de la agenda al instante y su cuenta atrás se retira.
pub fn anadir_borrado(app: &AppHandle, ev: &Evento) -> Result<AccionBorrado, String> {
    validar_borrado(ev)?;
    let accion = modificar(app, |c| encolar_borrado(c, ev)).ok_or_else(|| "La cola de Outlook no está activa".to_string())?;
    crate::cuentas::quitar_por_evento(app, &ev.id);
    Ok(accion)
}

/// Operaciones que aún no se han enviado a Outlook.
pub fn por_enviar(app: &AppHandle) -> Vec<Operacion> {
    instantanea(app).into_iter().filter(|o| o.enviada().is_none()).collect()
}

pub fn hay_por_enviar(app: &AppHandle) -> bool {
    instantanea(app).iter().any(|o| o.enviada().is_none())
}

pub fn operacion_enviada(app: &AppHandle, clave: &str) {
    modificar(app, |c| marcar_enviada(c, clave, Utc::now()));
}

/// Fallo con Outlook abierto: se reintentará; si se agotan los intentos, se descarta y se avisa.
pub fn fallo_transitorio(app: &AppHandle, clave: &str, mensaje: &str) {
    if let Some(Some(op)) = modificar(app, |c| sumar_intento(c, clave)) {
        avisar(app, &op, mensaje);
    }
}

/// Error definitivo (no se va a poder): se saca de la cola y se avisa. Un borrado fallido
/// devuelve el evento a la agenda.
pub fn descartar(app: &AppHandle, op: &Operacion, mensaje: &str) {
    let clave = op.clave();
    modificar(app, |c| c.retain(|o| o.clave() != clave));
    avisar(app, op, mensaje);
}

fn avisar(app: &AppHandle, op: &Operacion, mensaje: &str) {
    let _ = app.emit("pendientes://fallo", Fallo { titulo: op.titulo().to_string(), mensaje: mensaje.to_string() });
}

/// Tras una sincronización del ICS: se retiran las operaciones que el ICS ya refleja.
pub fn limpiar_con_ics(app: &AppHandle, ics: &[Evento]) {
    let Some(p) = app.try_state::<Pendientes>() else { return };
    let mut cola = p.cola.lock().unwrap();
    if limpiar(&mut cola, ics, Utc::now()) {
        guardar(app, &cola);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use chrono::{FixedOffset, NaiveDate};

    fn naive(d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn ahora() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 16, 10, 0, 0).unwrap()
    }

    fn ev(id: &str, titulo: &str, ini: NaiveDateTime, fin: NaiveDateTime) -> Evento {
        let z = FixedOffset::east_opt(3600).unwrap();
        Evento {
            id: id.into(),
            uid: id.into(),
            titulo: titulo.into(),
            inicio: z.from_local_datetime(&ini).unwrap(),
            fin: z.from_local_datetime(&fin).unwrap(),
            todo_el_dia: false,
            ubicacion: None,
            categorias: vec![],
            descripcion: None,
            recurrente: false,
            pendiente: false,
        }
    }

    fn nuevo(titulo: &str, ini: &str, fin: &str) -> NuevoEvento {
        NuevoEvento {
            titulo: titulo.into(),
            inicio: ini.into(),
            fin: fin.into(),
            todo_el_dia: false,
            ubicacion: None,
            categorias: vec![],
            cuenta: None,
        }
    }

    fn crear(id: &str, n: NuevoEvento, enviada: Option<DateTime<Utc>>) -> Operacion {
        Operacion::Crear { id: id.into(), evento: n, enviada, intentos: 0 }
    }

    fn borrar(e: &Evento, enviada: Option<DateTime<Utc>>) -> Operacion {
        Operacion::Borrar { evento: e.clone(), enviada, intentos: 0 }
    }

    fn ids(v: &[Evento]) -> Vec<&str> {
        v.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn el_borrado_pendiente_oculta_el_evento_del_ics() {
        let a = ev("a|1", "A", naive(17, 9, 0), naive(17, 10, 0));
        let b = ev("b|1", "B", naive(17, 11, 0), naive(17, 12, 0));
        let v = superponer(&[a.clone(), b.clone()], &[borrar(&a, None)]);
        assert_eq!(ids(&v), vec!["b|1"]);
        // Sin cola, el ICS se ve tal cual
        assert_eq!(ids(&superponer(&[a, b], &[])), vec!["a|1", "b|1"]);
    }

    #[test]
    fn la_creacion_pendiente_aparece_marcada_y_ordenada() {
        let a = ev("a|1", "A", naive(17, 9, 0), naive(17, 10, 0));
        let c = crear("7", nuevo("Nuevo", "2026-11-17T08:00", "2026-11-17T08:30"), None);
        let v = superponer(&[a], &[c]);
        assert_eq!(ids(&v), vec!["pendiente|7", "a|1"]);
        assert!(v[0].pendiente);
        assert!(!v[1].pendiente);
        assert_eq!(v[0].titulo, "Nuevo");
        assert_eq!(v[0].uid, "pendiente-7");
        assert_eq!(v[0].inicio.naive_local(), naive(17, 8, 0));
        assert_eq!(v[0].fin.naive_local(), naive(17, 8, 30));
    }

    #[test]
    fn la_creacion_no_se_duplica_cuando_el_ics_ya_la_trae() {
        let publicado = ev("u|1", "  nuevo ", naive(17, 8, 0), naive(17, 8, 30));
        let c = crear("7", nuevo("Nuevo", "2026-11-17T08:00", "2026-11-17T08:30"), Some(ahora()));
        let v = superponer(&[publicado], &[c]);
        assert_eq!(ids(&v), vec!["u|1"]);
    }

    #[test]
    fn creacion_de_dia_completo_y_datos_invalidos() {
        let mut n = nuevo("Festivo", "2026-11-18", "2026-11-18");
        n.todo_el_dia = true;
        n.ubicacion = Some("  ".into());
        let e = evento_de_creacion("1", &n).unwrap();
        assert!(e.todo_el_dia);
        assert_eq!(e.inicio.naive_local(), naive(18, 0, 0));
        assert_eq!(e.fin.naive_local(), naive(19, 0, 0));
        assert_eq!(e.ubicacion, None);
        assert!(evento_de_creacion("2", &nuevo("", "2026-11-18T10:00", "2026-11-18T11:00")).is_none());
    }

    #[test]
    fn borrar_un_evento_pendiente_sin_enviar_cancela_la_creacion() {
        let mut cola = vec![crear("7", nuevo("Nuevo", "2026-11-17T08:00", "2026-11-17T08:30"), None)];
        let visto = superponer(&[], &cola).remove(0);
        assert_eq!(encolar_borrado(&mut cola, &visto), AccionBorrado::CreacionCancelada);
        assert!(cola.is_empty());
    }

    #[test]
    fn borrar_un_evento_pendiente_ya_enviado_encola_el_borrado_y_lo_oculta_por_datos() {
        let n = nuevo("Nuevo", "2026-11-17T08:00", "2026-11-17T08:30");
        let mut cola = vec![crear("7", n, Some(ahora()))];
        let visto = superponer(&[], &cola).remove(0);
        assert_eq!(encolar_borrado(&mut cola, &visto), AccionBorrado::Encolado);
        assert_eq!(cola.len(), 2);
        // Nada se ve mientras tanto…
        assert!(superponer(&[], &cola).is_empty());
        // …ni siquiera cuando el ICS publica el evento con otro id
        let publicado = ev("u|1", "Nuevo", naive(17, 8, 0), naive(17, 8, 30));
        assert!(superponer(&[publicado], &cola).is_empty());
    }

    #[test]
    fn el_borrado_no_se_apunta_dos_veces() {
        let a = ev("a|1", "A", naive(17, 9, 0), naive(17, 10, 0));
        let mut cola = vec![];
        assert_eq!(encolar_borrado(&mut cola, &a), AccionBorrado::Encolado);
        assert_eq!(encolar_borrado(&mut cola, &a), AccionBorrado::YaEstabaEncolado);
        assert_eq!(cola.len(), 1);
    }

    #[test]
    fn limpiar_borrado_enviado() {
        let a = ev("a|1", "A", naive(17, 9, 0), naive(17, 10, 0));
        let mut cola = vec![borrar(&a, Some(ahora()))];
        // El ICS aún lo lista: se sigue ocultando
        assert!(!limpiar(&mut cola, &[a.clone()], ahora() + Duration::minutes(5)));
        assert_eq!(cola.len(), 1);
        // Pasados 30 minutos se da por perdido y vuelve a verse
        assert!(limpiar(&mut cola, &[a.clone()], ahora() + Duration::minutes(31)));
        assert!(cola.is_empty());
        // El ICS ya no lo lista: se retira antes
        let mut cola = vec![borrar(&a, Some(ahora()))];
        assert!(limpiar(&mut cola, &[], ahora() + Duration::minutes(5)));
        assert!(cola.is_empty());
    }

    #[test]
    fn limpiar_creacion_enviada_y_no_tocar_lo_sin_enviar() {
        let n = nuevo("Nuevo", "2026-11-17T08:00", "2026-11-17T08:30");
        let publicado = ev("u|1", "Nuevo", naive(17, 8, 0), naive(17, 8, 30));
        let mut cola = vec![crear("7", n.clone(), Some(ahora()))];
        assert!(!limpiar(&mut cola, &[], ahora() + Duration::minutes(5))); // aún no publicado
        assert!(limpiar(&mut cola, &[publicado.clone()], ahora() + Duration::minutes(5)));
        assert!(cola.is_empty());
        let mut cola = vec![crear("7", n.clone(), Some(ahora()))];
        assert!(limpiar(&mut cola, &[], ahora() + Duration::minutes(40))); // caducada
        // Lo que no se ha enviado nunca se descarta por tiempo, pase el que pase
        let a = ev("a|1", "A", naive(17, 9, 0), naive(17, 10, 0));
        let mut cola = vec![crear("8", n, None), borrar(&a, None)];
        assert!(!limpiar(&mut cola, &[], ahora() + Duration::days(30)));
        assert_eq!(cola.len(), 2);
    }

    #[test]
    fn marcar_enviada_y_contar_sin_enviar() {
        let a = ev("a|1", "A", naive(17, 9, 0), naive(17, 10, 0));
        let mut cola = vec![crear("7", nuevo("N", "2026-11-17T08:00", "2026-11-17T09:00"), None), borrar(&a, None)];
        assert_eq!(sin_enviar(&cola), 2);
        assert!(marcar_enviada(&mut cola, "c:7", ahora()));
        assert!(!marcar_enviada(&mut cola, "c:otra", ahora()));
        assert_eq!(sin_enviar(&cola), 1);
        assert_eq!(cola[0].enviada(), Some(ahora()));
    }

    #[test]
    fn los_fallos_se_agotan_tras_cinco_intentos() {
        let a = ev("a|1", "A", naive(17, 9, 0), naive(17, 10, 0));
        let mut cola = vec![borrar(&a, None)];
        for _ in 0..MAX_INTENTOS - 1 {
            assert!(sumar_intento(&mut cola, "b:a|1").is_none());
        }
        let descartada = sumar_intento(&mut cola, "b:a|1").expect("se agotan los intentos");
        assert_eq!(descartada.titulo(), "A");
        assert!(cola.is_empty());
        assert!(sumar_intento(&mut cola, "b:a|1").is_none());
    }

    #[test]
    fn la_cola_se_guarda_y_se_lee_como_json() {
        let a = ev("a|1", "A", naive(17, 9, 0), naive(17, 10, 0));
        let cola = vec![
            crear("7", nuevo("Nuevo", "2026-11-17T08:00", "2026-11-17T08:30"), Some(ahora())),
            borrar(&a, None),
        ];
        let json = serde_json::to_string(&cola).unwrap();
        assert!(json.contains("\"tipo\":\"crear\"") && json.contains("\"tipo\":\"borrar\""));
        let leida: Vec<Operacion> = serde_json::from_str(&json).unwrap();
        assert_eq!(leida, cola);
        // Sin `enviada` ni `intentos` (archivo antiguo o a mano) se leen como «sin enviar»
        let minimo = r#"[{"tipo":"crear","id":"1","evento":{"titulo":"X","inicio":"2026-11-17T08:00","fin":"2026-11-17T09:00"}}]"#;
        let l: Vec<Operacion> = serde_json::from_str(minimo).unwrap();
        assert_eq!(sin_enviar(&l), 1);
    }
}
