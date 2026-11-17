//! Estado de interfaz compartido entre equipos: si la tira METEO está desplegada y la
//! última pestaña abierta. Se guarda en `app_data_dir()/interfaz.json` y se sincroniza con
//! `<carpeta>/netrunner-panel/interfaz.json`. Gana el estado entero más reciente.
//!
//! La interfaz lo aplica al arrancar (pestaña inicial y tira); un cambio remoto solo
//! mueve la tira, nunca cambia de pestaña mientras el usuario usa el panel.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::sync::{self, ahora_ms, escribir_atomico};

const ARCHIVO: &str = "interfaz.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Interfaz {
    pub tira_meteo_abierta: bool,
    pub ultima_pestana: String,
    /// Milisegundos UTC del último cambio hecho por el usuario.
    pub modificada: i64,
}

pub struct EstadoInterfaz {
    actual: Mutex<Interfaz>,
}

/// Gana el estado más reciente (entero); si empatan, se conserva el local.
pub fn fusionar(local: &Interfaz, remoto: &Interfaz) -> Interfaz {
    if remoto.modificada > local.modificada {
        remoto.clone()
    } else {
        local.clone()
    }
}

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(ARCHIVO))
}

fn cargar(app: &AppHandle) -> Interfaz {
    let Some(r) = ruta(app) else { return Interfaz::default() };
    let Ok(texto) = std::fs::read_to_string(&r) else { return Interfaz::default() };
    match serde_json::from_str::<Interfaz>(&texto) {
        Ok(i) => i,
        Err(_) => {
            let _ = std::fs::rename(&r, r.with_file_name("interfaz.corrupto.json"));
            Interfaz::default()
        }
    }
}

/// Paso de la sincronización entre equipos (lo llama el planificador de `notas`).
pub fn sincronizar(app: &AppHandle, carpeta: &str) -> Result<(), String> {
    let e = app.state::<EstadoInterfaz>();
    let local = e.actual.lock().unwrap().clone();
    let fusionado = sync::sincronizar_con(
        &local,
        &sync::ruta_remota(carpeta, ARCHIVO),
        ahora_ms(),
        |l, r| fusionar(l, r),
        |_, _| {},
    )?;
    let mut actual = e.actual.lock().unwrap();
    // Si el usuario cambió algo durante la sincronización, su cambio es más reciente y gana
    let nuevo = fusionar(&actual, &fusionado);
    if nuevo != *actual {
        if let Some(r) = ruta(app) {
            escribir_atomico(&r, &nuevo)?;
        }
        *actual = nuevo.clone();
        drop(actual);
        // Sin programar sincronización: el cambio ya viene del remoto
        let _ = app.emit("interfaz://estado", nuevo);
    }
    Ok(())
}

#[tauri::command]
pub fn estado_interfaz(app: AppHandle) -> Interfaz {
    app.state::<EstadoInterfaz>().actual.lock().unwrap().clone()
}

/// Guarda el estado de interfaz. Si no cambia nada no toca la marca de tiempo (así un
/// guardado repetido no provoca sincronizaciones).
#[tauri::command]
pub fn guardar_interfaz(app: AppHandle, tira_meteo_abierta: bool, ultima_pestana: String) -> Result<(), String> {
    let e = app.state::<EstadoInterfaz>();
    {
        let mut actual = e.actual.lock().unwrap();
        if actual.tira_meteo_abierta == tira_meteo_abierta && actual.ultima_pestana == ultima_pestana {
            return Ok(());
        }
        let nuevo = Interfaz {
            tira_meteo_abierta,
            ultima_pestana,
            modificada: ahora_ms().max(actual.modificada + 1),
        };
        if let Some(r) = ruta(&app) {
            escribir_atomico(&r, &nuevo)?;
        }
        *actual = nuevo;
    }
    crate::notas::programar_sync(&app);
    Ok(())
}

pub fn iniciar(app: &AppHandle) {
    app.manage(EstadoInterfaz { actual: Mutex::new(cargar(app)) });
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn i(tira: bool, pestana: &str, modificada: i64) -> Interfaz {
        Interfaz { tira_meteo_abierta: tira, ultima_pestana: pestana.into(), modificada }
    }

    #[test]
    fn gana_el_mas_reciente_entero() {
        let a = i(true, "notas", 10);
        let b = i(false, "correo", 20);
        // Se toma el estado entero, no campo a campo
        assert_eq!(fusionar(&a, &b), b);
        assert_eq!(fusionar(&b, &a), b);
        // Empate: se conserva el local
        let c = i(true, "tiempo", 20);
        assert_eq!(fusionar(&b, &c), b);
    }

    #[test]
    fn archivo_antiguo_o_vacio_da_valores_por_defecto() {
        let x: Interfaz = serde_json::from_str("{}").unwrap();
        assert_eq!(x, Interfaz::default());
        assert_eq!(x.modificada, 0);
        // El remoto sin marca no pisa lo local
        assert_eq!(fusionar(&i(true, "notas", 5), &x).ultima_pestana, "notas");
    }

    #[test]
    fn sincronizar_con_archivo_remoto() {
        let dir = std::env::temp_dir().join(format!("netrunner-interfaz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let r = dir.join(ARCHIVO);
        sync::escribir_atomico(&r, &i(true, "correo", 50)).unwrap();
        let res = sync::sincronizar_con(&i(false, "tiempo", 10), &r, 100, |l, r| fusionar(l, r), |_, _| {}).unwrap();
        assert_eq!(res, i(true, "correo", 50));
        // Local más nuevo: se escribe en el remoto
        let res = sync::sincronizar_con(&i(false, "notas", 90), &r, 100, |l, r| fusionar(l, r), |_, _| {}).unwrap();
        assert_eq!(res.ultima_pestana, "notas");
        assert_eq!(sync::leer_remoto::<Interfaz>(&r).unwrap().unwrap().ultima_pestana, "notas");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
