# Instalador HUD de Netrunner Panel

Exe con ventana sin bordes (Tauri 2). La interfaz está en `instalador/src` (Vite, puerto 1430) y el motor en `instalador/src-tauri` (Rust). Lleva incrustado el setup NSIS normal de la app y lo ejecuta en silencio (`/S`); el NSIS copia archivos, registra el desinstalador y crea el acceso del menú Inicio.

## Desarrollo

```
npm run instalador:tauri -- dev
```

(equivale a `cd instalador/src-tauri && tauri dev`; arranca Vite del instalador solo). Sin paquete incrustado, `instalar` devuelve «Este instalador no contiene el paquete de la aplicación.». Para probar con paquete en dev, define antes `NETRUNNER_NSIS_SETUP`. Ojo: instalar de verdad pisa la app instalada.

## Compilar

Lo normal es el flujo completo, que encadena todos los pasos y firma el resultado:

```
npm run instalador:construir
```

1. `npm run tauri build` compila la app y su setup NSIS (`src-tauri/target/release/bundle/nsis/`). Se omite con `--sin-app`.
2. `npm run instalador:build` compila el frontend del instalador (`instalador/dist`).
3. `cargo build --release` en `instalador/src-tauri`, con `NETRUNNER_NSIS_SETUP` apuntando al setup NSIS; `build.rs` lo incrusta en el exe. Con `--ligero` añade `-j 1` y desactiva LTO e incremental (poca memoria).
4. `npx tauri signer sign` firma el exe con la clave del actualizador (`~/.tauri/netrunner-panel.key` o `TAURI_SIGNING_PRIVATE_KEY`) y deja el `.sig` al lado.

Resultado: `instalador/src-tauri/target/release/netrunner-panel-setup.exe` (+ `.sig`; sin empaquetar, `bundle.active` es false). La versión mostrada sale de `src-tauri/tauri.conf.json` de la app. Después, `npm run release:preparar` lo copia como `release/Netrunner-Panel-Setup.exe` y genera `latest.json` con su firma.

## Comportamiento

El instalador detecta la situación al arrancar y elige uno de tres modos:

- **nueva**: no hay instalación previa; se ofrecen los ajustes iniciales.
- **detectada**: ya hay una versión instalada; se reinstala o repara conservando la configuración.
- **actualizacion**: lo lanza el actualizador de la propia app con el argumento `/UPDATE`; se ejecuta el NSIS con `/S /UPDATE`, que conserva los accesos existentes.

- Resto de casos (nueva y detectada): `/S`. El NSIS en silencio crea también el acceso del escritorio, así que el instalador lo borra si no existía antes.
- Ajustes: solo `tema`, `edge` y `autostart` de `%APPDATA%\com.netrunner.panel\config.json`; el resto se conserva.
