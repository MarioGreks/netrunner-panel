// Construye el instalador HUD completo y firmado: app (NSIS) -> frontend del instalador -> exe del instalador -> firma.
// Uso: npm run instalador:construir [-- --sin-app] [-- --ligero]
//   --sin-app  reutiliza el setup NSIS ya compilado de la app (no ejecuta `npm run tauri build`)
//   --ligero   compila con poca memoria: -j 1, sin LTO y sin compilación incremental
// No publica nada: ver «Publicar una versión» en el README.
import { existsSync, readFileSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const sinApp = args.includes("--sin-app");
const ligero = args.includes("--ligero");
const desconocidos = args.filter((a) => a !== "--sin-app" && a !== "--ligero");

const fallo = (msg, codigo = 1) => {
  console.error(`\nERROR: ${msg}`);
  process.exit(codigo);
};
const paso = (n, texto) => console.log(`\n=== [${n}/5] ${texto} ===`);

// Ejecuta un comando con la salida heredada; si falla, para con su código de salida.
function ejecutar(comando, argumentos, opciones = {}) {
  const r = spawnSync(comando, argumentos, {
    cwd: raiz,
    stdio: "inherit",
    shell: process.platform === "win32",
    ...opciones,
  });
  if (r.error) fallo(`no se pudo ejecutar «${comando}»: ${r.error.message}`);
  if (r.status !== 0) fallo(`«${comando} ${argumentos.join(" ")}» terminó con código ${r.status}.`, r.status ?? 1);
}

if (desconocidos.length) fallo(`opciones desconocidas: ${desconocidos.join(" ")}\nUso: npm run instalador:construir [-- --sin-app] [-- --ligero]`);

// Clave de firma del actualizador (solo se maneja su ruta, nunca su contenido).
const env = { ...process.env };
if (!env.TAURI_SIGNING_PRIVATE_KEY) {
  const clave = join(homedir(), ".tauri", "netrunner-panel.key");
  if (!existsSync(clave)) {
    fallo(`falta la clave de firma. Define TAURI_SIGNING_PRIVATE_KEY (ruta de la clave) o crea ${clave}.`);
  }
  env.TAURI_SIGNING_PRIVATE_KEY = clave;
  env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "";
}
env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= "";

const { version } = JSON.parse(readFileSync(join(raiz, "src-tauri", "tauri.conf.json"), "utf8"));
const nsis = join(raiz, "src-tauri", "target", "release", "bundle", "nsis", `Netrunner Panel_${version}_x64-setup.exe`);
console.log(`Netrunner Panel v${version}${sinApp ? " (sin recompilar la app)" : ""}${ligero ? " (modo ligero)" : ""}`);

paso(1, "Compilando la app y su setup NSIS (npm run tauri build)");
if (sinApp) {
  if (!existsSync(nsis)) fallo(`con --sin-app hace falta el setup ya compilado y no existe: ${nsis}\nQuita --sin-app para compilar la app.`);
  console.log(`Se reutiliza ${nsis}`);
} else {
  ejecutar("npm", ["run", "tauri", "build"], { env });
  if (!existsSync(nsis)) fallo(`la compilación terminó pero no existe ${nsis}`);
}

paso(2, "Compilando el frontend del instalador (npm run instalador:build)");
ejecutar("npm", ["run", "instalador:build"], { env });

paso(3, "Compilando el exe del instalador con el NSIS incrustado (cargo build --release)");
const crate = join(raiz, "instalador", "src-tauri");
const envCargo = { ...env, NETRUNNER_NSIS_SETUP: nsis };
// custom-protocol: la interfaz va dentro del exe (sin ella cargaría localhost:1430)
const argsCargo = ["build", "--release", "--features", "custom-protocol"];
if (ligero) {
  argsCargo.push("-j", "1");
  envCargo.CARGO_PROFILE_RELEASE_LTO = "false";
  envCargo.CARGO_INCREMENTAL = "0";
}
ejecutar("cargo", argsCargo, { cwd: crate, env: envCargo });
const exe = join(crate, "target", "release", "netrunner-panel-setup.exe");
if (!existsSync(exe)) fallo(`la compilación terminó pero no existe ${exe}`);

paso(4, "Firmando el instalador (tauri signer sign)");
const firma = ["tauri", "signer", "sign", "--app-version", version];
// `signer sign` lee la clave como texto en la variable; si es una ruta a un fichero, se pasa con -f.
if (existsSync(env.TAURI_SIGNING_PRIVATE_KEY)) firma.push("-f", `"${env.TAURI_SIGNING_PRIVATE_KEY}"`);
firma.push(`"${exe}"`);
const envFirma = { ...env };
if (existsSync(env.TAURI_SIGNING_PRIVATE_KEY)) delete envFirma.TAURI_SIGNING_PRIVATE_KEY;
ejecutar("npx", firma, { env: envFirma });
const sig = `${exe}.sig`;
if (!existsSync(sig)) fallo(`la firma terminó pero no existe ${sig}`);

paso(5, "Listo");
const mb = (f) => `${(statSync(f).size / 1048576).toFixed(1)} MB`;
console.log(`Instalador: ${exe} (${mb(exe)})`);
console.log(`Firma:      ${sig}`);
console.log(`Siguiente:  npm run release:preparar -- --notas "..."`);
