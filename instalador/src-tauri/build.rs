//! Incrusta el setup NSIS de la app (variable NETRUNNER_NSIS_SETUP) y expone la versión.
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=NETRUNNER_NSIS_SETUP");
    let conf = "../../src-tauri/tauri.conf.json";
    println!("cargo:rerun-if-changed={conf}");

    // Versión de la app: la de su tauri.conf.json.
    let texto = fs::read_to_string(conf).expect("no se pudo leer ../../src-tauri/tauri.conf.json");
    let json: serde_json::Value = serde_json::from_str(&texto).expect("tauri.conf.json de la app no es JSON válido");
    let version = json["version"].as_str().expect("tauri.conf.json de la app no tiene «version»");
    println!("cargo:rustc-env=NETRUNNER_VERSION={version}");

    // Paquete NSIS: se copia a OUT_DIR; si falta, queda un fichero vacío.
    let destino = PathBuf::from(env::var("OUT_DIR").unwrap()).join("paquete_nsis.exe");
    match env::var("NETRUNNER_NSIS_SETUP") {
        Ok(ruta) if !ruta.trim().is_empty() => {
            println!("cargo:rerun-if-changed={ruta}");
            fs::copy(&ruta, &destino).unwrap_or_else(|e| panic!("no se pudo copiar el setup NSIS «{ruta}»: {e}"));
        }
        _ => {
            println!("cargo:warning=NETRUNNER_NSIS_SETUP no definida: el instalador no llevará el paquete de la aplicación");
            fs::write(&destino, []).unwrap();
        }
    }

    tauri_build::build()
}
