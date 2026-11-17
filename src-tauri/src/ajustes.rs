//! Ajustes compartidos entre equipos: un subconjunto de la configuración (`correo_vip`,
//! `correo_palabras`, `etiquetas_cuentas_correo`, `ciudad` e `ics_url`) que se sincroniza
//! con `<carpeta>/netrunner-panel/ajustes.json`.
//!
//! Cada campo lleva su **propia marca de tiempo** y la fusión va campo a campo: gana el más
//! reciente. Un campo que nunca se ha tocado en un equipo tiene `modificada = 0`, así que
//! no pisa el valor del otro. Las marcas locales viven aparte, en
//! `app_data_dir()/ajustes_marcas.json` (no en config.json).
//!
//! Los valores remotos más nuevos se aplican a la configuración local por el mismo camino
//! que `set_config` (`config::aplicar`, con origen remoto): se refiltra el correo, se
//! sincroniza el calendario si cambió la URL y se refresca el tiempo si cambió la ciudad.
//! Ese camino no anota marcas nuevas ni programa otra sincronización, así que no hay
//! bucles. La URL del calendario es privada: nunca se escribe en logs ni en errores.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{limpiar_etiquetas, limpiar_lista, Ciudad, Config, Origen};
use crate::estado::Estado;
use crate::sync::{self, ahora_ms, escribir_atomico};

const ARCHIVO: &str = "ajustes.json";
const ARCHIVO_MARCAS: &str = "ajustes_marcas.json";

/// Un valor con su marca de tiempo (ms UTC). 0 = nunca modificado en ese equipo.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Campo<T: Default> {
    pub valor: T,
    pub modificada: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Ajustes {
    pub correo_vip: Campo<Vec<String>>,
    pub correo_palabras: Campo<Vec<String>>,
    pub etiquetas_cuentas_correo: Campo<HashMap<String, String>>,
    pub ciudad: Campo<Option<Ciudad>>,
    pub ics_url: Campo<Option<String>>,
}

/// Marcas de tiempo locales de cada campo compartido.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Marcas {
    pub correo_vip: i64,
    pub correo_palabras: i64,
    pub etiquetas_cuentas_correo: i64,
    pub ciudad: i64,
    pub ics_url: i64,
    /// La migración inicial (marcar los campos con valor) ya se hizo.
    pub migrado: bool,
}

pub struct EstadoAjustes {
    marcas: Mutex<Marcas>,
}

// ---------------------------------------------------------------------------
// Lógica pura
// ---------------------------------------------------------------------------

/// Elige entre dos versiones de un campo: gana la `modificada` más reciente. Si empatan
/// con marca real (> 0) y valores distintos, un criterio determinista (la clave mayor)
/// hace que los dos equipos converjan al mismo valor. Con marca 0 gana el local.
fn fusionar_campo<T: Default + Clone>(l: &Campo<T>, r: &Campo<T>, clave: impl Fn(&T) -> String) -> Campo<T> {
    if r.modificada > l.modificada {
        r.clone()
    } else if r.modificada == l.modificada && r.modificada > 0 && clave(&r.valor) > clave(&l.valor) {
        r.clone()
    } else {
        l.clone()
    }
}

fn clave_lista(v: &Vec<String>) -> String {
    v.join("\u{1}")
}

fn clave_etiquetas(m: &HashMap<String, String>) -> String {
    let mut pares: Vec<_> = m.iter().collect();
    pares.sort();
    format!("{pares:?}")
}

fn clave_debug<T: std::fmt::Debug>(v: &T) -> String {
    format!("{v:?}")
}

/// Fusión campo a campo.
pub fn fusionar(local: &Ajustes, remoto: &Ajustes) -> Ajustes {
    Ajustes {
        correo_vip: fusionar_campo(&local.correo_vip, &remoto.correo_vip, clave_lista),
        correo_palabras: fusionar_campo(&local.correo_palabras, &remoto.correo_palabras, clave_lista),
        etiquetas_cuentas_correo: fusionar_campo(&local.etiquetas_cuentas_correo, &remoto.etiquetas_cuentas_correo, clave_etiquetas),
        ciudad: fusionar_campo(&local.ciudad, &remoto.ciudad, clave_debug),
        ics_url: fusionar_campo(&local.ics_url, &remoto.ics_url, clave_debug),
    }
}

impl Ajustes {
    /// Deja los valores como los dejaría `Config::normalizar` (un remoto editado a mano o
    /// por otra versión no debe romper la comparación).
    fn normalizar(&mut self) {
        limpiar_lista(&mut self.correo_vip.valor);
        limpiar_lista(&mut self.correo_palabras.valor);
        limpiar_etiquetas(&mut self.etiquetas_cuentas_correo.valor);
    }
}

/// Los ajustes compartidos tal como están en la configuración, con sus marcas locales.
pub fn extraer(config: &Config, m: &Marcas) -> Ajustes {
    Ajustes {
        correo_vip: Campo { valor: config.correo_vip.clone(), modificada: m.correo_vip },
        correo_palabras: Campo { valor: config.correo_palabras.clone(), modificada: m.correo_palabras },
        etiquetas_cuentas_correo: Campo {
            valor: config.etiquetas_cuentas_correo.clone(),
            modificada: m.etiquetas_cuentas_correo,
        },
        ciudad: Campo { valor: config.ciudad.clone(), modificada: m.ciudad },
        ics_url: Campo { valor: config.ics_url.clone(), modificada: m.ics_url },
    }
}

/// Primera activación: los campos con valor distinto del de por defecto y sin marca se
/// marcan con la hora actual, para que el equipo que ya los tenía configurados los
/// comparta. Devuelve `true` si algo cambió.
pub fn migrar_marcas(config: &Config, m: &mut Marcas, ahora: i64) -> bool {
    if m.migrado {
        return false;
    }
    let def = Config::default();
    if m.correo_vip == 0 && config.correo_vip != def.correo_vip {
        m.correo_vip = ahora;
    }
    if m.correo_palabras == 0 && config.correo_palabras != def.correo_palabras {
        m.correo_palabras = ahora;
    }
    if m.etiquetas_cuentas_correo == 0 && config.etiquetas_cuentas_correo != def.etiquetas_cuentas_correo {
        m.etiquetas_cuentas_correo = ahora;
    }
    if m.ciudad == 0 && config.ciudad != def.ciudad {
        m.ciudad = ahora;
    }
    if m.ics_url == 0 && config.ics_url != def.ics_url {
        m.ics_url = ahora;
    }
    m.migrado = true;
    true
}

/// Anota en `m` la marca de los campos que cambian entre `anterior` y `nueva`.
/// Devuelve `true` si cambió alguno. La marca nunca retrocede.
pub fn marcar_cambios(anterior: &Config, nueva: &Config, m: &mut Marcas, ahora: i64) -> bool {
    let mut cambio = false;
    let mut marca = |cambia: bool, campo: &mut i64| {
        if cambia {
            *campo = ahora.max(*campo + 1);
            cambio = true;
        }
    };
    marca(anterior.correo_vip != nueva.correo_vip, &mut m.correo_vip);
    marca(anterior.correo_palabras != nueva.correo_palabras, &mut m.correo_palabras);
    marca(anterior.etiquetas_cuentas_correo != nueva.etiquetas_cuentas_correo, &mut m.etiquetas_cuentas_correo);
    marca(anterior.ciudad != nueva.ciudad, &mut m.ciudad);
    marca(anterior.ics_url != nueva.ics_url, &mut m.ics_url);
    cambio
}

/// Aplica a `config` los campos de `fusion` que son más nuevos que los locales y
/// actualiza las marcas de `m`. Un campo cuya marca local es más reciente que la de la
/// fusión (el usuario lo cambió durante la sincronización) se conserva. Devuelve `true`
/// si cambió algún valor de la configuración.
pub fn aplicar_remotos(config: &mut Config, fusion: &Ajustes, m: &mut Marcas) -> bool {
    let mut cambio = false;
    macro_rules! campo {
        ($f:ident) => {{
            let c = &fusion.$f;
            if c.modificada > 0
                && (c.modificada > m.$f || (c.modificada == m.$f && c.valor != config.$f))
            {
                if c.valor != config.$f {
                    config.$f = c.valor.clone();
                    cambio = true;
                }
                m.$f = c.modificada;
            }
        }};
    }
    campo!(correo_vip);
    campo!(correo_palabras);
    campo!(etiquetas_cuentas_correo);
    campo!(ciudad);
    campo!(ics_url);
    cambio
}

// ---------------------------------------------------------------------------
// Archivo y estado
// ---------------------------------------------------------------------------

fn ruta_marcas(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(ARCHIVO_MARCAS))
}

fn cargar_marcas(app: &AppHandle) -> Marcas {
    let Some(r) = ruta_marcas(app) else { return Marcas::default() };
    let Ok(texto) = std::fs::read_to_string(&r) else { return Marcas::default() };
    serde_json::from_str(&texto).unwrap_or_default()
}

fn guardar_marcas(app: &AppHandle, m: &Marcas) -> Result<(), String> {
    let r = ruta_marcas(app).ok_or("sin directorio de datos")?;
    escribir_atomico(&r, m)
}

fn config_actual(app: &AppHandle) -> Config {
    app.state::<Estado>().config.lock().unwrap().clone()
}

/// Llamada por `config::aplicar` tras un cambio hecho por el usuario: anota la marca de
/// los campos compartidos que cambiaron y programa una sincronización.
pub fn cambios_locales(app: &AppHandle, anterior: &Config, nueva: &Config) {
    let Some(e) = app.try_state::<EstadoAjustes>() else { return };
    let cambio = {
        let mut m = e.marcas.lock().unwrap();
        let cambio = marcar_cambios(anterior, nueva, &mut m, ahora_ms());
        if cambio {
            let _ = guardar_marcas(app, &m);
        }
        cambio
    };
    if cambio {
        crate::notas::programar_sync(app);
    }
}

/// Paso de la sincronización entre equipos (lo llama el planificador de `notas`).
pub fn sincronizar(app: &AppHandle, carpeta: &str) -> Result<(), String> {
    let e = app.state::<EstadoAjustes>();
    let config = config_actual(app); // sin bloqueos de marcas a la vez: primero config
    let local = {
        let mut m = e.marcas.lock().unwrap();
        if migrar_marcas(&config, &mut m, ahora_ms()) {
            let _ = guardar_marcas(app, &m);
        }
        extraer(&config, &m)
    };
    let fusion = sync::sincronizar_con(
        &local,
        &sync::ruta_remota(carpeta, ARCHIVO),
        ahora_ms(),
        |l, r| {
            let mut r = r.clone();
            r.normalizar();
            fusionar(l, &r)
        },
        |_, _| {},
    )?;
    aplicar_en_local(app, &fusion)
}

/// Aplica la fusión a la configuración local si trae valores más nuevos.
fn aplicar_en_local(app: &AppHandle, fusion: &Ajustes) -> Result<(), String> {
    let e = app.state::<EstadoAjustes>();
    // Comprobación previa sobre copias: sin novedades no se reescribe config.json
    {
        let mut cfg = config_actual(app);
        let antes = e.marcas.lock().unwrap().clone();
        let mut m = antes.clone();
        if !aplicar_remotos(&mut cfg, fusion, &mut m) && m == antes {
            return Ok(());
        }
    }
    let mut hay_cambio = false;
    // Orden de bloqueos: configuración y después marcas (aquí, dentro de `aplicar`)
    crate::config::aplicar(app, Origen::Remoto, |actual| {
        let mut nueva = actual.clone();
        let mut m = e.marcas.lock().unwrap();
        let marcas_antes = m.clone();
        if aplicar_remotos(&mut nueva, fusion, &mut m) {
            hay_cambio = true;
        }
        if *m != marcas_antes {
            let _ = guardar_marcas(app, &m);
        }
        nueva
    })?;
    if hay_cambio {
        // Para que AJUSTES, si está abierto, recargue los formularios
        let _ = app.emit("ajustes://cambio", ());
    }
    Ok(())
}

pub fn iniciar(app: &AppHandle) {
    app.manage(EstadoAjustes { marcas: Mutex::new(cargar_marcas(app)) });
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn dir_temporal(nombre: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("netrunner-ajustes-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn lista(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cambios_distintos_en_a_y_en_b_se_conservan() {
        // A cambió el VIP (t=10) y B cambió la URL (t=20)
        let mut a = Ajustes::default();
        a.correo_vip = Campo { valor: lista(&["@a.com"]), modificada: 10 };
        let mut b = Ajustes::default();
        b.ics_url = Campo { valor: Some("https://example.com/b.ics".into()), modificada: 20 };
        for f in [fusionar(&a, &b), fusionar(&b, &a)] {
            assert_eq!(f.correo_vip.valor, lista(&["@a.com"]));
            assert_eq!(f.ics_url.valor.as_deref(), Some("https://example.com/b.ics"));
        }
        // El mismo campo en los dos: gana la marca más reciente
        b.correo_vip = Campo { valor: lista(&["@b.com"]), modificada: 30 };
        assert_eq!(fusionar(&a, &b).correo_vip.valor, lista(&["@b.com"]));
        assert_eq!(fusionar(&b, &a).correo_vip.valor, lista(&["@b.com"]));
    }

    #[test]
    fn campo_con_marca_cero_no_pisa() {
        let mut a = Ajustes::default();
        a.correo_palabras = Campo { valor: lista(&["tfm"]), modificada: 5 };
        // B nunca lo tocó: valor vacío y marca 0
        let b = Ajustes::default();
        assert_eq!(fusionar(&a, &b).correo_palabras.valor, lista(&["tfm"]));
        assert_eq!(fusionar(&b, &a).correo_palabras.valor, lista(&["tfm"]));
        // Con dos marcas 0 y valores distintos, cada uno conserva el suyo
        let mut c = Ajustes::default();
        c.correo_palabras.valor = lista(&["x"]);
        assert_eq!(fusionar(&b, &c).correo_palabras.valor, Vec::<String>::new());
        assert_eq!(fusionar(&c, &b).correo_palabras.valor, lista(&["x"]));
    }

    #[test]
    fn empate_con_valores_distintos_converge() {
        let mut a = Ajustes::default();
        a.correo_vip = Campo { valor: lista(&["a"]), modificada: 7 };
        let mut b = Ajustes::default();
        b.correo_vip = Campo { valor: lista(&["b"]), modificada: 7 };
        assert_eq!(fusionar(&a, &b).correo_vip, fusionar(&b, &a).correo_vip);
    }

    #[test]
    fn migracion_inicial_marca_los_campos_con_valor() {
        let mut cfg = Config::default();
        cfg.correo_vip = lista(&["@uni.es"]);
        cfg.ics_url = Some("https://example.com/a.ics".into());
        let mut m = Marcas::default();
        assert!(migrar_marcas(&cfg, &mut m, 1000));
        assert_eq!(m.correo_vip, 1000);
        assert_eq!(m.ics_url, 1000);
        assert_eq!((m.correo_palabras, m.etiquetas_cuentas_correo, m.ciudad), (0, 0, 0));
        assert!(m.migrado);
        // Solo una vez: después ya no marca nada
        cfg.ciudad = Some(Ciudad { nombre: "X".into(), pais: "Y".into(), latitud: 1.0, longitud: 2.0 });
        assert!(!migrar_marcas(&cfg, &mut m, 2000));
        assert_eq!(m.ciudad, 0);
        // Un campo ya marcado conserva su marca
        let mut m2 = Marcas { ics_url: 55, ..Default::default() };
        migrar_marcas(&cfg, &mut m2, 3000);
        assert_eq!(m2.ics_url, 55);
        assert_eq!(m2.ciudad, 3000);
    }

    #[test]
    fn config_antigua_sin_campos_compartidos_ni_marcas() {
        let cfg: Config = serde_json::from_str(r#"{"edge":"left","tema":"netrunner"}"#).unwrap();
        let m: Marcas = serde_json::from_str("{}").unwrap();
        assert_eq!(m, Marcas::default());
        let a = extraer(&cfg, &m);
        assert_eq!(a, Ajustes::default());
        // Un remoto parcial o vacío también es válido
        let r: Ajustes = serde_json::from_str(r#"{"ics_url":{"valor":"https://example.com/x.ics","modificada":9}}"#).unwrap();
        assert_eq!(r.correo_vip.modificada, 0);
        assert_eq!(r.ics_url.modificada, 9);
        let vacio: Ajustes = serde_json::from_str("{}").unwrap();
        assert_eq!(vacio, Ajustes::default());
    }

    #[test]
    fn marcar_cambios_solo_toca_lo_que_cambia() {
        let a = Config::default();
        let mut b = a.clone();
        b.correo_palabras = lista(&["urgente"]);
        b.tema = "otro".into(); // no compartido
        b.carpeta_sync = Some("C:/x".into()); // no compartido
        let mut m = Marcas { correo_palabras: 500, ..Default::default() };
        assert!(marcar_cambios(&a, &b, &mut m, 100));
        assert_eq!(m.correo_palabras, 501); // la marca nunca retrocede
        assert_eq!((m.correo_vip, m.ciudad, m.ics_url, m.etiquetas_cuentas_correo), (0, 0, 0, 0));
        // Cambiar solo campos no compartidos no marca nada
        let mut c = b.clone();
        c.opacidad = 0.7;
        c.edge = crate::config::Borde::Left;
        assert!(!marcar_cambios(&b, &c, &mut m, 200));
    }

    #[test]
    fn aplicar_remotos_actualiza_config_y_marcas_sin_bucle() {
        let mut cfg = Config::default();
        cfg.tema = "mio".into();
        cfg.ics_url = Some("https://example.com/vieja.ics".into());
        let mut m = Marcas { ics_url: 10, migrado: true, ..Default::default() };
        let mut fusion = extraer(&cfg, &m);
        fusion.ics_url = Campo { valor: Some("https://example.com/nueva.ics".into()), modificada: 20 };
        fusion.correo_vip = Campo { valor: lista(&["@uni.es"]), modificada: 15 };
        fusion.ciudad = Campo { valor: None, modificada: 0 }; // sin marca: no cuenta
        assert!(aplicar_remotos(&mut cfg, &fusion, &mut m));
        assert_eq!(cfg.ics_url.as_deref(), Some("https://example.com/nueva.ics"));
        assert_eq!(cfg.correo_vip, lista(&["@uni.es"]));
        assert_eq!((m.ics_url, m.correo_vip), (20, 15));
        assert_eq!(cfg.tema, "mio"); // lo no compartido no se toca
        // Aplicar otra vez lo mismo no cambia nada (idempotente)
        let antes = (cfg.clone(), m.clone());
        assert!(!aplicar_remotos(&mut cfg, &fusion, &mut m));
        assert_eq!((cfg, m), antes);
        // Un cambio local posterior (marca más reciente) no se pisa
        let mut local = Config::default();
        local.correo_vip = lista(&["@local.es"]);
        let mut ml = Marcas { correo_vip: 99, ..Default::default() };
        aplicar_remotos(&mut local, &fusion, &mut ml);
        assert_eq!(local.correo_vip, lista(&["@local.es"]));
    }

    #[test]
    fn sincronizar_con_archivo_remoto_y_remoto_corrupto() {
        let dir = dir_temporal("sync");
        let r = dir.join(ARCHIVO);
        let mut remoto = Ajustes::default();
        remoto.correo_vip = Campo { valor: lista(&["@r.com"]), modificada: 50 };
        sync::escribir_atomico(&r, &remoto).unwrap();
        let mut local = Ajustes::default();
        local.ics_url = Campo { valor: Some("https://example.com/l.ics".into()), modificada: 10 };
        let f = sync::sincronizar_con(&local, &r, 100, |l, r| fusionar(l, r), |_, _| {}).unwrap();
        assert_eq!(f.correo_vip.valor, lista(&["@r.com"]));
        assert_eq!(f.ics_url.modificada, 10);
        assert_eq!(sync::leer_remoto::<Ajustes>(&r).unwrap().unwrap(), f);

        // Corrupto: error sin el contenido y archivo intacto
        std::fs::write(&r, "{ https://example.com/secreta.ics").unwrap();
        let e = sync::sincronizar_con(&local, &r, 100, |l, r| fusionar(l, r), |_, _| {}).unwrap_err();
        assert!(!e.contains("secreta"));
        assert_eq!(std::fs::read_to_string(&r).unwrap(), "{ https://example.com/secreta.ics");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn el_planificador_sigue_si_un_archivo_esta_corrupto() {
        let dir = dir_temporal("planificador");
        let r_ajustes = dir.join("ajustes.json");
        let r_interfaz = dir.join("interfaz.json");
        std::fs::write(&r_ajustes, "{ corrupto").unwrap();
        crate::sync::escribir_atomico(&r_interfaz, &crate::interfaz::Interfaz::default()).unwrap();
        let ejecutado = std::cell::Cell::new(false);
        let pasos: Vec<sync::Paso> = vec![
            (
                "ajustes",
                Box::new(|| {
                    sync::sincronizar_con(&Ajustes::default(), &r_ajustes, 1, |l, r| fusionar(l, r), |_, _| {}).map(|_| ())
                }),
            ),
            (
                "interfaz",
                Box::new(|| {
                    ejecutado.set(true);
                    let l = crate::interfaz::Interfaz { tira_meteo_abierta: true, ultima_pestana: "notas".into(), modificada: 9 };
                    sync::sincronizar_con(&l, &r_interfaz, 1, |l, r| crate::interfaz::fusionar(l, r), |_, _| {}).map(|_| ())
                }),
            ),
        ];
        let (bien, errores) = sync::ejecutar_pasos(pasos);
        assert_eq!(bien, 1);
        assert!(ejecutado.get());
        assert_eq!(errores.len(), 1);
        assert!(errores[0].starts_with("ajustes: "));
        // El archivo sano sí se sincronizó y el corrupto no se tocó
        let i: crate::interfaz::Interfaz = sync::leer_remoto(&r_interfaz).unwrap().unwrap();
        assert_eq!(i.ultima_pestana, "notas");
        assert_eq!(std::fs::read_to_string(&r_ajustes).unwrap(), "{ corrupto");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
