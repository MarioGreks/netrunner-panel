// Ventana de un widget de escritorio (widget.html?tipo=<tipo>): marco HUD y, dentro, el contenido
// del tipo. El núcleo (widgets.rs) la crea, la pega al escritorio y la destruye.
import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/hud.css";
import "./styles/widget.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { alEstadoWidgets, quitarWidget, widgetsEstado, type EstadoWidgets, type TipoWidget } from "./lib/ipc";
import { iniciarTema } from "./lib/tema";

iniciarTema();

/** Pinta el contenido en `cuerpo`; puede devolver la función que lo limpia al cerrar la ventana. */
type Contenido = (cuerpo: HTMLElement) => (() => void) | void;

const TITULOS: Record<TipoWidget, string> = {
  reloj: "RELOJ",
  sistema: "SISTEMA",
  consumo: "CONSUMO",
  tareas: "TAREAS",
  cuentas: "CUENTAS ATRÁS",
};

/** Marcador provisional: los encargos siguientes sustituyen cada entrada por el widget real. */
const enConstruccion: Contenido = (cuerpo) => {
  const p = document.createElement("p");
  p.className = "wg__marcador";
  p.textContent = "EN CONSTRUCCIÓN";
  cuerpo.replaceChildren(p);
};

export const CONTENIDOS: Record<TipoWidget, Contenido> = {
  reloj: enConstruccion,
  sistema: enConstruccion,
  consumo: enConstruccion,
  tareas: enConstruccion,
  cuentas: enConstruccion,
};

const tipo = (new URLSearchParams(location.search).get("tipo") ?? "") as TipoWidget;
const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const raiz = q("raiz");
const ventana = getCurrentWindow();

function aplicar(e: EstadoWidgets) {
  document.documentElement.style.setProperty("--wg-opacidad", String(e.opacidad));
  raiz.classList.toggle("wg--edicion", e.edicion);
}

if (tipo in TITULOS) {
  q("titulo").textContent = TITULOS[tipo];
  document.title = `${TITULOS[tipo]} // NETRUNNER`;
  const limpiar = CONTENIDOS[tipo](q("cuerpo"));
  if (limpiar) window.addEventListener("pagehide", limpiar);
} else {
  q("titulo").textContent = "WIDGET DESCONOCIDO";
}

void widgetsEstado().then(aplicar).catch(() => {});
void alEstadoWidgets(aplicar);

// Solo actúan en modo edición: fuera de él las asas están ocultas (display: none)
q("barra").addEventListener("pointerdown", (ev) => {
  if ((ev.target as HTMLElement).closest("button") || ev.button !== 0) return;
  void ventana.startDragging().catch(() => {});
});
q("quitar").addEventListener("click", () => void quitarWidget(tipo).catch(() => {}));
q("asa").addEventListener("pointerdown", (ev) => {
  if (ev.button !== 0) return;
  void ventana.startResizeDragging("SouthEast").catch(() => {});
});

// Entrada única con un parpadeo breve (sin él con movimiento reducido)
if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
  raiz.classList.add("wg--entrada");
  setTimeout(() => raiz.classList.remove("wg--entrada"), 400);
}
