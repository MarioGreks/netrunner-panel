//! Lectura del registro: versión y carpeta de la instalación existente.
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

const CLAVE: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Netrunner Panel";

pub struct Instalada {
    pub version: Option<String>,
    pub carpeta: Option<String>,
}

pub fn leer_instalacion() -> Option<Instalada> {
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(CLAVE).ok()?;
    let version = k.get_value::<String, _>("DisplayVersion").ok().filter(|v| !v.trim().is_empty());
    // NSIS guarda InstallLocation entre comillas.
    let carpeta = k
        .get_value::<String, _>("InstallLocation")
        .ok()
        .map(|c| c.trim().trim_matches('"').to_string())
        .filter(|c| !c.is_empty());
    Some(Instalada { version, carpeta })
}

/// Carpeta del escritorio del usuario (resuelta, aunque esté redirigida a OneDrive).
pub fn carpeta_escritorio() -> Option<std::path::PathBuf> {
    let k = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        .ok()?;
    k.get_value::<String, _>("Desktop").ok().map(Into::into)
}
