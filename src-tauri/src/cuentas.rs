//! Módulo CUENTAS ATRÁS: eventos de la agenda marcados con ★ para contar el tiempo que
//! falta. Se guardan en `app_data_dir()/cuentas.json` (escritura atómica) y se ponen al
//! día en cada sincronización del calendario.
//!
//! Se sincronizan entre equipos con `<carpeta>/netrunner-panel/cuentas.json`. La fusión
//! va por `id_evento`: gana la `modificada` más reciente y, si empatan, el borrado.
//! Desmarcar deja una marca de borrado (`borrada`) durante 30 días para que se propague;
//! las cuentas cuyo inicio ya pasó se purgan siempre.
//!
//! Reconciliación (ver `reconciliar_lista`): se casa por `id_evento`; si no aparece, por
//! `uid` (solo si ese uid tiene un único evento: un evento suelto cuya hora cambió). Las
//! ocurrencias movidas de una serie conservan su id, así que no necesitan el `uid`.
//! Solo se toca `modificada` si cambia algo de verdad, para no provocar sincronizaciones
//! en bucle entre equipos.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::calendario::Evento;
use crate::sync::{self, ahora_ms, escribir_atomico, fusionar_lista, VIDA_MARCAS_MS};

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
    /// Milisegundos UTC del último cambio (marcar, desmarcar o datos del evento).
    /// 0 en los archivos antiguos: al cargar se sustituye por `creada`.
    #[serde(default)]
    pub modificada: i64,
    /// Marca de borrado: se conserva para propagar el desmarcado.
    #[serde(default)]
    pub borrada: bool,
}

pub struct Cuentas {
    lista: Mutex<Vec<Cuenta>>,
}

// ---------------------------------------------------------------------------
// Lógica pura
// ---------------------------------------------------------------------------

/// Copia los datos del evento; devuelve `true` si algo cambió de verdad.
fn copiar_datos(c: &mut Cuenta, e: &Evento) -> bool {
    let cambia = c.id_evento != e.id || c.titulo != e.titulo || c.inicio != e.inicio || c.categorias != e.categorias;
    if cambia {
        c.id_evento = e.id.clone();
        c.titulo = e.titulo.clone();
        c.inicio = e.inicio;
        c.categorias = e.categorias.clone();
    }
    cambia
}

/// Pone al día las cuentas con los eventos recién leídos. Devuelve `true` si algo cambió.
///
/// - Se actualizan título, inicio y categorías del evento con el mismo `id_evento`
///   (y `modificada` solo si algún dato es distinto).
/// - Si no aparece y su inicio aún es futuro, se busca por `uid`; solo se acepta si hay un
///   único evento con ese uid que no sea ya de otra cuenta (en una serie, varias
///   ocurrencias comparten uid y saltar a otra sería un error).
/// - Si no aparece y es futura, se conserva con sus últimos datos.
/// - Las marcas de borrado no se tocan.
/// - Todas las cuentas cuyo inicio ya pasó se borran (la cuenta atrás terminó).
pub fn reconciliar_lista(cuentas: &mut Vec<Cuenta>, eventos: &[Evento], ahora: DateTime<Utc>) -> bool {
    let antes = cuentas.len();
    let mut cambio = false;
    let marcados: HashSet<String> = cuentas.iter().map(|c| c.id_evento.clone()).collect();
    let mut reclamados: HashSet<String> = HashSet::new();
    for c in cuentas.iter_mut().filter(|c| !c.borrada) {
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
            if copiar_datos(c, e) {
                c.modificada = ahora.timestamp_millis().max(c.modificada + 1);
                cambio = true;
            }
        }
    }
    cuentas.retain(|c| c.inicio.with_timezone(&Utc) > ahora);
    cambio || cuentas.len() != antes
}

/// Añade (o refresca) la cuenta de un evento futuro. Una marca de borrado del mismo
/// evento se revive.
fn anadir(cuentas: &mut Vec<Cuenta>, e: &Evento, ahora: DateTime<Utc>) -> Result<(), String> {
    if e.id.is_empty() {
        return Err("El evento no tiene identificador; sincroniza la agenda".into());
    }
    if e.inicio.with_timezone(&Utc) <= ahora {
        return Err("El evento ya ha empezado".into());
    }
    let ms = ahora.timestamp_millis();
    match cuentas.iter_mut().find(|c| c.id_evento == e.id) {
        Some(c) => {
            let revive = c.borrada;
            let cambia = copiar_datos(c, e);
            if revive || cambia {
                c.borrada = false;
                c.uid = e.uid.clone();
                c.modificada = ms.max(c.modificada + 1);
            }
        }
        None => cuentas.push(Cuenta {
            id_evento: e.id.clone(),
            uid: e.uid.clone(),
            titulo: e.titulo.clone(),
            inicio: e.inicio,
            categorias: e.categorias.clone(),
            creada: ms,
            modificada: ms,
            borrada: false,
        }),
    }
    Ok(())
}

/// Convierte la cuenta en una marca de borrado (conserva id, uid e inicio; vacía el resto).
fn quitar(cuentas: &mut [Cuenta], id_evento: &str, ahora: i64) {
    if let Some(c) = cuentas.iter_mut().find(|c| c.id_evento == id_evento && !c.borrada) {
        c.borrada = true;
        c.titulo.clear();
        c.categorias.clear();
        c.modificada = ahora.max(c.modificada + 1);
    }
}

/// Las futuras y no borradas, ordenadas por inicio: lo que ve la interfaz.
fn futuras(cuentas: &[Cuenta], ahora: DateTime<Utc>) -> Vec<Cuenta> {
    let mut v: Vec<Cuenta> = cuentas
        .iter()
        .filter(|c| !c.borrada && c.inicio.with_timezone(&Utc) > ahora)
        .cloned()
        .collect();
    v.sort_by(|a, b| a.inicio.cmp(&b.inicio).then(a.titulo.cmp(&b.titulo)));
    v
}

/// Fusiona dos listas por `id_evento` (gana la `modificada` más reciente; en empate, el
/// borrado). El resultado va ordenado para que sea estable entre equipos.
pub fn fusionar(local: &[Cuenta], remoto: &[Cuenta]) -> Vec<Cuenta> {
    let mut r = fusionar_lista(local, remoto, |c| &c.id_evento, |c| (c.modificada, c.borrada));
    r.sort_by(|a, b| a.inicio.cmp(&b.inicio).then_with(|| a.id_evento.cmp(&b.id_evento)));
    r
}

/// Quita las cuentas cuyo inicio ya pasó y las marcas de borrado de más de 30 días.
pub fn purgar(cuentas: &mut Vec<Cuenta>, ahora: i64) {
    let limite = ahora - VIDA_MARCAS_MS;
    cuentas.retain(|c| c.inicio.timestamp_millis() > ahora && !(c.borrada && c.modificada < limite));
}

// ---------------------------------------------------------------------------
// Archivo y estado
// ---------------------------------------------------------------------------

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(ARCHIVO))
}

/// Carga el archivo; si está corrupto, lo aparta como `cuentas.corrupto.json`.
fn cargar(app: &AppHandle) -> Vec<Cuenta> {
    let Some(r) = ruta(app) else { return vec![] };
    let Ok(texto) = std::fs::read_to_string(&r) else { return vec![] };
    match serde_json::from_str::<Vec<Cuenta>>(&texto) {
        Ok(mut v) => {
            migrar(&mut v);
            v
        }
        Err(_) => {
            let _ = std::fs::rename(&r, r.with_file_name("cuentas.corrupto.json"));
            vec![]
        }
    }
}

/// Los archivos anteriores a la sincronización no tienen `modificada`: se usa `creada`.
fn migrar(v: &mut [Cuenta]) {
    for c in v.iter_mut().filter(|c| c.modificada == 0) {
        c.modificada = c.creada;
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

/// Aplica un cambio, lo guarda, emite el estado y programa la sincronización. Si falla
/// al guardar, no cambia nada.
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
    crate::notas::programar_sync(app);
    Ok(())
}

/// Llamada por el calendario tras cada sincronización correcta.
pub fn reconciliar(app: &AppHandle, eventos: &[Evento]) {
    let cambio = {
        let c = app.state::<Cuentas>();
        let mut lista = c.lista.lock().unwrap();
        let mut nueva = lista.clone();
        if reconciliar_lista(&mut nueva, eventos, Utc::now()) && guardar(app, &nueva).is_ok() {
            *lista = nueva;
            true
        } else {
            false
        }
    };
    emitir(app);
    if cambio {
        crate::notas::programar_sync(app);
    }
}

/// Paso de la sincronización entre equipos (lo llama el planificador de `notas`).
pub fn sincronizar(app: &AppHandle, carpeta: &str) -> Result<(), String> {
    let c = app.state::<Cuentas>();
    let local = c.lista.lock().unwrap().clone();
    let fusionado = sync::sincronizar_con(
        &local,
        &sync::ruta_remota(carpeta, ARCHIVO),
        ahora_ms(),
        |l, r| fusionar(l, r),
        |v, a| purgar(v, a),
    )?;
    let mut lista = c.lista.lock().unwrap();
    // Cambios locales durante la sincronización: se fusionan de nuevo (idempotente)
    let mut nueva = fusionar(&lista, &fusionado);
    purgar(&mut nueva, ahora_ms());
    if nueva != *lista {
        guardar(app, &nueva)?;
        *lista = nueva;
        drop(lista);
        emitir(app);
    }
    Ok(())
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

/// El evento se borró en Outlook: su cuenta atrás (si la tenía) ya no tiene sentido.
pub fn quitar_por_evento(app: &AppHandle, id_evento: &str) {
    let hay = app.state::<Cuentas>().lista.lock().unwrap().iter().any(|c| c.id_evento == id_evento && !c.borrada);
    if hay {
        let _ = modificar(app, |l| {
            quitar(l, id_evento, ahora_ms());
            Ok(())
        });
    }
}

#[tauri::command]
pub fn desmarcar_cuenta(app: AppHandle, id_evento: String) -> Result<(), String> {
    modificar(&app, |l| {
        quitar(l, &id_evento, ahora_ms());
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
            recurrente: false,
            pendiente: false,
        }
    }

    fn cuenta(id: &str, uid: &str, titulo: &str, inicio: DateTime<FixedOffset>) -> Cuenta {
        Cuenta { id_evento: id.into(), uid: uid.into(), titulo: titulo.into(), inicio, categorias: vec![], creada: 1, modificada: 1, borrada: false }
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

    fn con(mut c: Cuenta, modificada: i64, borrada: bool) -> Cuenta {
        c.modificada = modificada;
        c.borrada = borrada;
        c
    }

    fn base() -> Cuenta {
        cuenta("a|1", "a", "Examen", en(10, 9))
    }

    #[test]
    fn marcar_en_a_y_desmarcar_en_b_gana_lo_mas_reciente() {
        // A marca (modificada 10) y B, que la había recibido, la desmarca después (20)
        let a = vec![con(base(), 10, false)];
        let b = vec![con(base(), 20, true)];
        assert!(fusionar(&a, &b)[0].borrada);
        assert!(fusionar(&b, &a)[0].borrada);
        // Al revés: B desmarcó antes (10) y A la vuelve a marcar después (30)
        let a = vec![con(base(), 30, false)];
        let b = vec![con(base(), 10, true)];
        assert!(!fusionar(&a, &b)[0].borrada);
        assert!(!fusionar(&b, &a)[0].borrada);
    }

    #[test]
    fn empate_en_cuentas_gana_el_borrado() {
        let a = vec![con(base(), 10, false)];
        let b = vec![con(base(), 10, true)];
        assert!(fusionar(&a, &b)[0].borrada);
        assert!(fusionar(&b, &a)[0].borrada);
    }

    #[test]
    fn fusion_conserva_cuentas_distintas_de_cada_equipo() {
        let a = vec![cuenta("a|1", "a", "A", en(5, 9))];
        let b = vec![cuenta("b|1", "b", "B", en(3, 9))];
        let f = fusionar(&a, &b);
        assert_eq!(f.iter().map(|c| c.id_evento.as_str()).collect::<Vec<_>>(), vec!["b|1", "a|1"]);
    }

    #[test]
    fn desmarcar_deja_marca_y_marcar_la_revive() {
        let mut v = vec![];
        anadir(&mut v, &evento("a|1", "a", "Examen", en(10, 9)), ahora()).unwrap();
        quitar(&mut v, "a|1", 500);
        assert_eq!(v.len(), 1);
        assert!(v[0].borrada && v[0].titulo.is_empty() && v[0].modificada >= 500);
        assert!(futuras(&v, ahora()).is_empty());
        anadir(&mut v, &evento("a|1", "a", "Examen", en(10, 9)), ahora() + chrono::Duration::days(1)).unwrap();
        assert!(!v[0].borrada);
        assert_eq!(futuras(&v, ahora()).len(), 1);
        // La reconciliación no toca las marcas de borrado
        let mut w = vec![con(cuenta("z|1", "z", "", en(8, 9)), 5, true)];
        assert!(!reconciliar_lista(&mut w, &[evento("z|1", "z", "Otro", en(9, 9))], ahora()));
        assert!(w[0].titulo.is_empty());
    }

    #[test]
    fn purga_pasadas_y_marcas_viejas() {
        let ahora_ms = ahora().timestamp_millis();
        let dia = 24 * 3600 * 1000;
        let mut v = vec![
            con(cuenta("p|1", "p", "Pasada", en(-1, 9)), ahora_ms - dia, false),
            con(cuenta("v|1", "v", "", en(20, 9)), ahora_ms - 31 * dia, true),
            con(cuenta("r|1", "r", "", en(20, 9)), ahora_ms - 29 * dia, true),
            con(cuenta("f|1", "f", "Futura", en(20, 9)), ahora_ms - 90 * dia, false),
        ];
        purgar(&mut v, ahora_ms);
        assert_eq!(v.iter().map(|c| c.id_evento.as_str()).collect::<Vec<_>>(), vec!["r|1", "f|1"]);
    }

    #[test]
    fn reconciliar_no_cambia_modificada_sin_cambios_reales() {
        let mut v = vec![con(cuenta("a|1", "a", "Examen", en(10, 9)), 123, false)];
        let e = evento("a|1", "a", "Examen", en(10, 9));
        // Mismo instante con otro desfase horario: no es un cambio
        let mut e_otra_zona = e.clone();
        e_otra_zona.inicio = e.inicio.with_timezone(&FixedOffset::east_opt(2 * 3600).unwrap());
        for _ in 0..3 {
            assert!(!reconciliar_lista(&mut v, &[e.clone()], ahora()));
            assert!(!reconciliar_lista(&mut v, &[e_otra_zona.clone()], ahora()));
        }
        assert_eq!(v[0].modificada, 123);
        // Un cambio real sí la actualiza
        let mut e2 = e.clone();
        e2.titulo = "Examen final".into();
        assert!(reconciliar_lista(&mut v, &[e2], ahora()));
        assert!(v[0].modificada > 123);
    }

    #[test]
    fn archivo_antiguo_sin_modificada_ni_borrada() {
        let json = r#"[{"id_evento":"a|1","uid":"a","titulo":"X","inicio":"2026-12-01T09:00:00+01:00","creada":777}]"#;
        let mut v: Vec<Cuenta> = serde_json::from_str(json).unwrap();
        assert_eq!(v[0].modificada, 0);
        assert!(!v[0].borrada);
        migrar(&mut v);
        assert_eq!(v[0].modificada, 777);
    }

    #[test]
    fn sincronizar_cuentas_con_archivo_remoto() {
        let dir = std::env::temp_dir().join(format!("netrunner-cuentas-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let r = dir.join(ARCHIVO);
        let ahora_ms = ahora().timestamp_millis();
        sync::escribir_atomico(&r, &vec![con(cuenta("b|1", "b", "B", en(4, 9)), 10, false)]).unwrap();
        let local = vec![con(cuenta("a|1", "a", "A", en(3, 9)), 10, false)];
        let res = sync::sincronizar_con(&local, &r, ahora_ms, |l, r| fusionar(l, r), |v, a| purgar(v, a)).unwrap();
        assert_eq!(res.len(), 2);
        let leidas: Vec<Cuenta> = sync::leer_remoto(&r).unwrap().unwrap();
        assert_eq!(leidas, res);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
