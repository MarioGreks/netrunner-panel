// Prepara los ficheros de una release en release/: el instalador y latest.json.
// Uso: npm run release:preparar -- --notas "Texto de las notas"
// No publica nada: ver «Publicar una versión» en el README.
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fallo = (msg) => {
  console.error(`ERROR: ${msg}`);
  process.exit(1);
};

const i = process.argv.indexOf("--notas");
const notasArg = i !== -1 ? process.argv[i + 1] : undefined;
if (i !== -1 && !notasArg) fallo("--notas necesita un texto.");

const { version } = JSON.parse(readFileSync(join(raiz, "src-tauri", "tauri.conf.json"), "utf8"));
const nsis = join(raiz, "src-tauri", "target", "release", "bundle", "nsis");
const exe = join(nsis, `Netrunner Panel_${version}_x64-setup.exe`);
const sig = `${exe}.sig`;
if (!existsSync(exe)) fallo(`no existe ${exe}\nCompila antes con la clave de firma: npm run tauri build`);
if (!existsSync(sig)) fallo(`no existe ${sig}\nCompila con TAURI_SIGNING_PRIVATE_KEY definida para generar la firma.`);

const salida = join(raiz, "release");
rmSync(salida, { recursive: true, force: true });
mkdirSync(salida);
copyFileSync(exe, join(salida, "Netrunner-Panel-Setup.exe"));

const latest = {
  version,
  notes: notasArg ?? `Netrunner Panel v${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(sig, "utf8").trim(),
      url: `https://github.com/MarioGreks/netrunner-panel/releases/download/v${version}/Netrunner-Panel-Setup.exe`,
    },
  },
};
writeFileSync(join(salida, "latest.json"), JSON.stringify(latest, null, 2) + "\n");
console.log(`Release v${version} preparada en ${salida}`);
