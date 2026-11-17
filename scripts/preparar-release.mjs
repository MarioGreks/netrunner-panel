// Prepara los ficheros de una release en release/: el instalador HUD y latest.json.
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
// El instalador de la release es el HUD propio (lleva el NSIS de la app incrustado).
const exe = join(raiz, "instalador", "src-tauri", "target", "release", "netrunner-panel-setup.exe");
const sig = `${exe}.sig`;
if (!existsSync(exe)) fallo(`no existe ${exe}
Ejecuta antes: npm run instalador:construir`);
if (!existsSync(sig)) fallo(`no existe ${sig}
Ejecuta antes: npm run instalador:construir (firma el instalador con la clave del actualizador).`);

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
