// Evita la ventana de consola extra en Windows en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    netrunner_panel_lib::run()
}
