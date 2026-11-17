//! Detección de pantalla completa en el monitor principal (juegos, vídeos, presentaciones).

/// Rectángulo en píxeles físicos: (izquierda, arriba, derecha, abajo).
pub type Caja = (i32, i32, i32, i32);

/// ¿La ventana cubre por completo el monitor? Tolera 1 px por los bordes negativos.
pub fn cubre_monitor(ventana: Caja, monitor: Caja) -> bool {
    ventana.0 <= monitor.0 + 1
        && ventana.1 <= monitor.1 + 1
        && ventana.2 >= monitor.2 - 1
        && ventana.3 >= monitor.3 - 1
}

/// Traduce el código de `SHQueryUserNotificationState`: true para QUNS_BUSY (2),
/// QUNS_RUNNING_D3D_FULL_SCREEN (3) y QUNS_PRESENTATION_MODE (4).
pub fn quns_es_pantalla_completa(codigo: i32) -> bool {
    matches!(codigo, 2..=4)
}

/// ¿Hay algo a pantalla completa en el monitor principal?
#[cfg(windows)]
pub fn detectar() -> bool {
    quns() || ventana_cubre_principal()
}

#[cfg(not(windows))]
pub fn detectar() -> bool {
    false
}

#[cfg(windows)]
fn quns() -> bool {
    use windows::Win32::UI::Shell::SHQueryUserNotificationState;
    unsafe { SHQueryUserNotificationState() }
        .map(|e| quns_es_pantalla_completa(e.0))
        .unwrap_or(false)
}

#[cfg(windows)]
fn ventana_cubre_principal() -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetShellWindow, GetWindowRect,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };
    // MONITORINFOF_PRIMARY
    const PRINCIPAL: u32 = 1;

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() || !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        if hwnd == GetShellWindow() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return false;
        }
        let mut buf = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut buf).max(0) as usize;
        let clase = String::from_utf16_lossy(&buf[..n]);
        if matches!(clase.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
        if hmon.is_invalid() {
            return false;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut info).as_bool() || info.dwFlags & PRINCIPAL == 0 {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return false;
        }
        let m = info.rcMonitor;
        cubre_monitor((r.left, r.top, r.right, r.bottom), (m.left, m.top, m.right, m.bottom))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Caja = (0, 0, 1920, 1080);

    #[test]
    fn cubre_exacto() {
        assert!(cubre_monitor(MON, MON));
    }

    #[test]
    fn mas_grande_por_bordes_negativos() {
        assert!(cubre_monitor((-8, -8, 1928, 1088), MON));
    }

    #[test]
    fn maximizada_con_barra_de_tareas_no() {
        assert!(!cubre_monitor((0, 0, 1920, 1040), MON));
    }

    #[test]
    fn ventana_pequena_no() {
        assert!(!cubre_monitor((100, 100, 900, 700), MON));
    }

    #[test]
    fn otro_desplazamiento_no() {
        assert!(!cubre_monitor((1920, 0, 3840, 1080), MON));
        assert!(!cubre_monitor((40, 0, 1960, 1080), MON));
    }

    #[test]
    fn codigos_quns() {
        assert!(!quns_es_pantalla_completa(1)); // NOT_PRESENT
        assert!(quns_es_pantalla_completa(2)); // BUSY
        assert!(quns_es_pantalla_completa(3)); // D3D
        assert!(quns_es_pantalla_completa(4)); // PRESENTATION
        assert!(!quns_es_pantalla_completa(5)); // ACCEPTS_NOTIFICATIONS
        assert!(!quns_es_pantalla_completa(6)); // QUIET_TIME
        assert!(!quns_es_pantalla_completa(7)); // APP
    }
}
