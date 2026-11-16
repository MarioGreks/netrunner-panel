//! Módulo ACCESOS: cuadrícula de accesos directos (apps, carpetas, archivos y URLs).
//!
//! Se guardan en `app_data_dir()/accesos.json` con escritura atómica. El icono se
//! extrae una sola vez, al añadir el acceso, y se guarda como data URL PNG.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{Interface, HSTRING, PCWSTR};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IBindCtx, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, IShellLinkW, SHCreateItemFromParsingName, ShellExecuteW, ShellLink,
    SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, SLGP_RAWPATH,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::ventanas;

const ARCHIVO: &str = "accesos.json";
const TAM_ICONO: i32 = 64;
const TIPOS: [&str; 4] = ["app", "carpeta", "archivo", "url"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Acceso {
    pub id: String,
    pub nombre: String,
    pub destino: String,
    /// "app" | "carpeta" | "archivo" | "url"
    pub tipo: String,
    /// Data URL PNG; las URLs no llevan icono.
    pub icono: Option<String>,
    pub orden: i64,
}

pub struct Accesos {
    lista: Mutex<Vec<Acceso>>,
}

// ---------------------------------------------------------------------------
// Lógica pura
// ---------------------------------------------------------------------------

/// Normaliza y valida una URL: solo `http` y `https`. Sin esquema se asume `https://`.
pub fn validar_url(entrada: &str) -> Result<String, String> {
    let t = entrada.trim();
    if t.is_empty() {
        return Err("La URL está vacía".into());
    }
    if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("La URL no puede contener espacios".into());
    }
    let bajo = t.to_lowercase();
    let url = if bajo.starts_with("http://") || bajo.starts_with("https://") {
        t.to_string()
    } else if !t.contains(':') {
        format!("https://{t}")
    } else {
        return Err("Solo se admiten URLs http y https".into());
    };
    if host_de_url(&url).is_empty() {
        return Err("La URL no tiene dominio".into());
    }
    Ok(url)
}

/// Dominio de una URL ya validada, sin `www.`, puerto ni credenciales.
fn host_de_url(url: &str) -> String {
    let sin_esq = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let autoridad = sin_esq.split(['/', '?', '#']).next().unwrap_or("");
    let host = autoridad.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    let bajo = host.to_lowercase();
    bajo.strip_prefix("www.").unwrap_or(&bajo).to_string()
}

fn ultimo_componente(ruta: &str) -> &str {
    let t = ruta.trim_end_matches(['\\', '/']);
    t.rsplit(['\\', '/']).next().unwrap_or(t)
}

/// Nombre por defecto de un acceso.
pub fn deducir_nombre(destino: &str, tipo: &str) -> String {
    match tipo {
        "url" => host_de_url(destino),
        "carpeta" => {
            let t = ultimo_componente(destino);
            // Raíz de unidad («D:\»): se usa tal cual
            if t.is_empty() || (t.len() == 2 && t.ends_with(':')) {
                destino.trim().trim_end_matches(['\\', '/']).to_string() + "\\"
            } else {
                t.to_string()
            }
        }
        _ => {
            let f = ultimo_componente(destino);
            match f.rfind('.') {
                Some(i) if i > 0 => f[..i].to_string(),
                _ => f.to_string(),
            }
        }
    }
}

/// Reordena según `ids`; los no mencionados conservan su orden relativo al final.
pub fn reordenar(lista: &mut Vec<Acceso>, ids: &[String]) {
    let mut nueva: Vec<Acceso> = Vec::with_capacity(lista.len());
    for id in ids {
        if let Some(p) = lista.iter().position(|a| &a.id == id) {
            nueva.push(lista.remove(p));
        }
    }
    nueva.append(lista);
    for (i, a) in nueva.iter_mut().enumerate() {
        a.orden = i as i64;
    }
    *lista = nueva;
}

// ---------------------------------------------------------------------------
// Iconos (Windows)
// ---------------------------------------------------------------------------

/// Ejecuta `f` en un hilo propio con COM inicializado (apartamento de un solo hilo).
fn en_hilo_com<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> Option<R> {
    thread::spawn(move || {
        let inicializado = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
        let r = f();
        if inicializado {
            unsafe { CoUninitialize() };
        }
        r
    })
    .join()
    .ok()
}

/// Destino real de un `.lnk`, o `None` si no se puede resolver.
fn destino_de_lnk(ruta: &str) -> Option<String> {
    unsafe {
        let enlace: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let fichero: IPersistFile = enlace.cast().ok()?;
        fichero.Load(&HSTRING::from(ruta), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        enlace.GetPath(&mut buf, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).ok()?;
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]);
        // Variables de entorno sin expandir o destino ya inexistente: no sirve
        if s.is_empty() || s.contains('%') || !Path::new(&s).exists() {
            None
        } else {
            Some(s)
        }
    }
}

/// Icono grande de `ruta` como PNG (RGBA). Usa `IShellItemImageFactory` (64 px), que da
/// mejor calidad que `SHGetFileInfoW` (32 px). Debe llamarse con COM inicializado.
fn icono_png(ruta: &str) -> Option<Vec<u8>> {
    unsafe {
        let item: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(ruta), None::<&IBindCtx>).ok()?;
        let hbm = item
            .GetImage(SIZE { cx: TAM_ICONO, cy: TAM_ICONO }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
            .ok()?;
        // El bitmap se libera siempre, salga como salga la conversión
        let png = bitmap_a_png(hbm);
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        png
    }
}

unsafe fn bitmap_a_png(hbm: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<Vec<u8>> {
    let mut bmp = BITMAP::default();
    let leido = GetObjectW(
        HGDIOBJ(hbm.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bmp as *mut BITMAP as *mut c_void),
    );
    if leido == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 || bmp.bmWidth > 512 || bmp.bmHeight > 512 {
        return None;
    }
    let (w, h) = (bmp.bmWidth as usize, bmp.bmHeight as usize);
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w as i32,
        biHeight: -(h as i32), // de arriba abajo
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut px = vec![0u8; w * h * 4];
    let hdc = GetDC(None);
    let filas = GetDIBits(hdc, hbm, 0, h as u32, Some(px.as_mut_ptr() as *mut c_void), &mut info, DIB_RGB_COLORS);
    ReleaseDC(None, hdc);
    if filas == 0 {
        return None;
    }
    // BGRA -> RGBA; si el bitmap no trae alfa (todo 0), se hace opaco
    let sin_alfa = px.chunks_exact(4).all(|p| p[3] == 0);
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
        if sin_alfa {
            p[3] = 255;
        }
    }
    let mut salida = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut salida, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut esc = enc.write_header().ok()?;
        esc.write_image_data(&px).ok()?;
    }
    Some(salida)
}

/// Icono como data URL PNG. Para `.lnk` se prefiere el del destino real.
fn extraer_icono(destino: &str) -> Option<String> {
    let destino = destino.to_string();
    en_hilo_com(move || {
        let mut origen = destino.clone();
        if destino.to_lowercase().ends_with(".lnk") {
            if let Some(real) = destino_de_lnk(&destino) {
                origen = real;
            }
        }
        let png = icono_png(&origen).or_else(|| if origen != destino { icono_png(&destino) } else { None })?;
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ))
    })
    .flatten()
}

// ---------------------------------------------------------------------------
// Almacén
// ---------------------------------------------------------------------------

fn ruta(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(ARCHIVO))
}

fn escribir_atomico(ruta: &Path, lista: &[Acceso]) -> Result<(), String> {
    let dir = ruta.parent().ok_or("ruta sin carpeta")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("no se pudo crear la carpeta: {e}"))?;
    let json = serde_json::to_string_pretty(lista).map_err(|e| e.to_string())?;
    let tmp = ruta.with_extension(format!("tmp{}", std::process::id()));
    let res = std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, ruta));
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("no se pudo escribir: {e}"));
    }
    Ok(())
}

fn cargar(app: &AppHandle) -> Vec<Acceso> {
    let Some(r) = ruta(app) else { return vec![] };
    let Ok(texto) = std::fs::read_to_string(&r) else { return vec![] };
    match serde_json::from_str::<Vec<Acceso>>(&texto) {
        Ok(mut l) => {
            l.sort_by_key(|a| a.orden);
            l
        }
        Err(_) => {
            let _ = std::fs::rename(&r, r.with_file_name("accesos.corrupto.json"));
            vec![]
        }
    }
}

/// Aplica un cambio, lo guarda y emite el estado. Si no se puede guardar, no se aplica.
fn modificar<R>(app: &AppHandle, cambio: impl FnOnce(&mut Vec<Acceso>) -> Result<R, String>) -> Result<R, String> {
    let estado = app.state::<Accesos>();
    let mut l = estado.lista.lock().map_err(|_| "estado bloqueado".to_string())?;
    let mut copia = l.clone();
    let r = cambio(&mut copia)?;
    for (i, a) in copia.iter_mut().enumerate() {
        a.orden = i as i64;
    }
    if let Some(p) = ruta(app) {
        escribir_atomico(&p, &copia)?;
    }
    *l = copia.clone();
    drop(l);
    let _ = app.emit("accesos://estado", copia);
    Ok(r)
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn estado_accesos(app: AppHandle) -> Vec<Acceso> {
    app.state::<Accesos>().lista.lock().map(|l| l.clone()).unwrap_or_default()
}

#[tauri::command]
pub async fn anadir_acceso(app: AppHandle, destino: String, tipo: String) -> Result<(), String> {
    if !TIPOS.contains(&tipo.as_str()) {
        return Err("Tipo de acceso desconocido".into());
    }
    let destino = destino.trim().to_string();
    let (destino, icono) = if tipo == "url" {
        (validar_url(&destino)?, None)
    } else {
        let p = Path::new(&destino);
        if !p.exists() {
            return Err("La ruta no existe".into());
        }
        if tipo == "carpeta" && !p.is_dir() {
            return Err("No es una carpeta".into());
        }
        let d = destino.clone();
        // Extraer el icono (COM, GDI) fuera del hilo principal
        let icono = tauri::async_runtime::spawn_blocking(move || extraer_icono(&d))
            .await
            .map_err(|e| e.to_string())?;
        (destino, icono)
    };
    let nombre = deducir_nombre(&destino, &tipo);
    modificar(&app, move |l| {
        l.push(Acceso {
            id: uuid::Uuid::new_v4().to_string(),
            nombre: if nombre.is_empty() { "SIN NOMBRE".into() } else { nombre },
            destino,
            tipo,
            icono,
            orden: l.len() as i64,
        });
        Ok(())
    })
}

#[tauri::command]
pub fn renombrar_acceso(app: AppHandle, id: String, nombre: String) -> Result<(), String> {
    let nombre = nombre.trim().chars().take(60).collect::<String>();
    if nombre.is_empty() {
        return Err("El nombre no puede estar vacío".into());
    }
    modificar(&app, |l| {
        let a = l.iter_mut().find(|a| a.id == id).ok_or("El acceso ya no existe")?;
        a.nombre = nombre;
        Ok(())
    })
}

#[tauri::command]
pub fn borrar_acceso(app: AppHandle, id: String) -> Result<(), String> {
    modificar(&app, |l| {
        l.retain(|a| a.id != id);
        Ok(())
    })
}

#[tauri::command]
pub fn reordenar_accesos(app: AppHandle, ids: Vec<String>) -> Result<(), String> {
    modificar(&app, |l| {
        reordenar(l, &ids);
        Ok(())
    })
}

fn abrir_shell(objetivo: String) -> Result<(), String> {
    let r = en_hilo_com(move || unsafe {
        let h = ShellExecuteW(
            None,
            windows::core::w!("open"),
            &HSTRING::from(objetivo),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        // Éxito si el valor devuelto es mayor que 32
        h.0 as isize > 32
    })
    .unwrap_or(false);
    if r {
        Ok(())
    } else {
        Err("No se pudo abrir".into())
    }
}

#[tauri::command]
pub async fn abrir_acceso(app: AppHandle, id: String) -> Result<(), String> {
    let acceso = {
        let estado = app.state::<Accesos>();
        let l = estado.lista.lock().map_err(|_| "estado bloqueado".to_string())?;
        l.iter().find(|a| a.id == id).cloned().ok_or("El acceso ya no existe")?
    };
    let objetivo = if acceso.tipo == "url" {
        // Se revalida al abrir: el archivo podría haberse editado a mano
        validar_url(&acceso.destino)?
    } else {
        if !Path::new(&acceso.destino).exists() {
            return Err("El destino ya no existe".into());
        }
        acceso.destino
    };
    tauri::async_runtime::spawn_blocking(move || abrir_shell(objetivo))
        .await
        .map_err(|e| e.to_string())??;
    ventanas::pedir_ocultar(&app);
    Ok(())
}

pub fn iniciar(app: &AppHandle) {
    app.manage(Accesos { lista: Mutex::new(cargar(app)) });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(id: &str) -> Acceso {
        Acceso { id: id.into(), nombre: id.into(), ..Default::default() }
    }

    #[test]
    fn nombres_deducidos() {
        assert_eq!(deducir_nombre(r"C:\Program Files\Mozilla Firefox\firefox.exe", "app"), "firefox");
        assert_eq!(deducir_nombre(r"C:\Users\x\Desktop\Mi Juego.lnk", "app"), "Mi Juego");
        assert_eq!(deducir_nombre(r"D:\Usuario\Proyectos\", "carpeta"), "Proyectos");
        assert_eq!(deducir_nombre(r"D:\Usuario\Proyectos", "carpeta"), "Proyectos");
        assert_eq!(deducir_nombre(r"D:\", "carpeta"), r"D:\");
        assert_eq!(deducir_nombre(r"D:\docs\informe.final.pdf", "archivo"), "informe.final");
        assert_eq!(deducir_nombre("https://www.ejemplo.com/ruta/a?x=1", "url"), "ejemplo.com");
        assert_eq!(deducir_nombre("http://user@Sub.Ejemplo.com:8080/", "url"), "sub.ejemplo.com");
    }

    #[test]
    fn urls_validas_e_invalidas() {
        assert_eq!(validar_url("https://ejemplo.com").unwrap(), "https://ejemplo.com");
        assert_eq!(validar_url("HTTP://ejemplo.com/a").unwrap(), "HTTP://ejemplo.com/a");
        assert_eq!(validar_url("ejemplo.com/ruta").unwrap(), "https://ejemplo.com/ruta");
        assert!(validar_url("file:///C:/Windows/System32/cmd.exe").is_err());
        assert!(validar_url("javascript:alert(1)").is_err());
        assert!(validar_url("ms-settings:network").is_err());
        assert!(validar_url("https://").is_err());
        assert!(validar_url("").is_err());
        assert!(validar_url("https://a.com/ b").is_err());
    }

    #[test]
    fn reordenado() {
        let mut l = vec![acc("a"), acc("b"), acc("c"), acc("d")];
        reordenar(&mut l, &["c".into(), "a".into(), "zzz".into()]);
        let ids: Vec<_> = l.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b", "d"]);
        assert_eq!(l.iter().map(|a| a.orden).collect::<Vec<_>>(), [0, 1, 2, 3]);
    }
}
