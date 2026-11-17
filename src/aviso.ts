import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/aviso.css";
import { iniciarTema } from "./lib/tema";
import {
  alEvento,
  infoActualizacion,
  instalarActualizacion,
  posponerActualizacion,
  type ProgresoActualizacion,
} from "./lib/ipc";

iniciarTema();

const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const elVersiones = q("versiones");
const elNotas = q("notas");
const elProgreso = q("progreso");
const elBarra = q("barra");
const elPct = q("pct");
const elError = q("error");
const btnActualizar = q<HTMLButtonElement>("actualizar");
const btnLuego = q<HTMLButtonElement>("luego");

/** Segmentos de la barra de progreso. */
const SEGMENTOS = 30;
elBarra.innerHTML = '<i class="aviso__seg"></i>'.repeat(SEGMENTOS);
const segmentos = Array.from(elBarra.children) as HTMLElement[];

const pintarBarra = (fraccion: number | null) => {
  const llenos = fraccion === null ? 0 : Math.round(fraccion * SEGMENTOS);
  segmentos.forEach((s, i) => s.classList.toggle("aviso__seg--on", i < llenos));
  elBarra.classList.toggle("aviso__barra--indeterminada", fraccion === null);
};

const mostrarError = (msg: string) => {
  elError.textContent = msg;
  elError.hidden = false;
};

let ocupado = false;

const posponer = () => {
  if (ocupado) return;
  void posponerActualizacion().catch(() => {});
};

// Las notas van como texto plano (textContent escapa el HTML)
infoActualizacion()
  .then((info) => {
    if (!info) {
      elVersiones.textContent = "SIN DATOS DE ACTUALIZACIÓN";
      btnActualizar.disabled = true;
      return;
    }
    elVersiones.textContent = `v${info.version_actual} → v${info.version}`;
    elNotas.textContent = info.notas ?? "Sin notas de la versión.";
  })
  .catch((e) => mostrarError(String(e)));

void alEvento<ProgresoActualizacion>("actualizador://progreso", ({ descargado, total }) => {
  if (total && total > 0) {
    const f = Math.min(1, descargado / total);
    pintarBarra(f);
    elPct.textContent = `${Math.floor(f * 100)} %`;
  } else {
    pintarBarra(null);
    elPct.textContent = "DESCARGANDO…";
  }
});
void alEvento("actualizador://instalando", () => {
  pintarBarra(1);
  elPct.textContent = "INSTALANDO…";
});

btnActualizar.addEventListener("click", () => {
  if (ocupado) return;
  ocupado = true;
  btnActualizar.disabled = true;
  btnLuego.disabled = true;
  elError.hidden = true;
  elProgreso.hidden = false;
  pintarBarra(null);
  elPct.textContent = "DESCARGANDO…";
  instalarActualizacion().catch((e) => {
    // Si todo va bien la app se cierra y se relanza; si no, se muestra el fallo
    ocupado = false;
    btnActualizar.disabled = false;
    btnLuego.disabled = false;
    elProgreso.hidden = true;
    mostrarError(String(e));
  });
});
btnLuego.addEventListener("click", posponer);
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    e.preventDefault();
    posponer();
  }
});
