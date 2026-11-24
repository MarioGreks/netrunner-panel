// Ventana de un widget de escritorio (widget.html?tipo=<tipo>): marco HUD y, dentro, el contenido
// del tipo. El núcleo (widgets.rs) la crea, la pega al escritorio y la destruye.
import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/hud.css";
import "./styles/widget.css";
import "./styles/widgets-contenido.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  alEstadoWidgets,
  alVisibilidadWidget,
  fijarEdicionWidgets,
  quitarWidget,
  widgetALaVista,
  widgetsEstado,
  type EstadoWidgets,
  type TipoWidget,
} from "./lib/ipc";
import { iniciarTema } from "./lib/tema";
import { montarConsumo } from "./widgets/consumo";
import { montarCuentas } from "./widgets/cuentas";
import { montarReloj } from "./widgets/reloj";
import { montarSistema } from "./widgets/sistema";
import { montarTareas } from "./widgets/tareas";

iniciarTema();

/** Lo que recibe cada contenido para no trabajar mientras el widget no se ve (tapado u oculto). */
export interface ContextoWidget {
  tipo: TipoWidget;
  visible: () => boolean;
  /** Avisa solo cuando cambia (no al registrarse): consultar `visible()` para el estado inicial. */
  alVisibilidad: (cb: (v: boolean) => void) => void;
}

/** Pinta el contenido en `cuerpo`; puede devolver la función que lo limpia al cerrar la ventana. */
type Contenido = (cuerpo: HTMLElement, ctx: ContextoWidget) => (() => void) | void;

const TITULOS: Record<TipoWidget, string> = {
  reloj: "RELOJ",
  sistema: "SISTEMA",
  consumo: "CONSUMO",
  tareas: "TAREAS",
  cuentas: "CUENTAS ATRÁS",
};

export const CONTENIDOS: Record<TipoWidget, Contenido> = {
  reloj: montarReloj,
  sistema: montarSistema,
  consumo: montarConsumo,
  tareas: montarTareas,
  cuentas: montarCuentas,
};

const tipo = (new URLSearchParams(location.search).get("tipo") ?? "") as TipoWidget;
const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const raiz = q("raiz");
const ventana = getCurrentWindow();

// Visibilidad: se pregunta una vez al cargar y después solo llegan los cambios de este tipo
let vistoAhora = true;
const oyentes: Array<(v: boolean) => void> = [];
function fijarVisible(v: boolean) {
  if (v === vistoAhora) return;
  vistoAhora = v;
  for (const cb of oyentes) cb(v);
}
const ctx: ContextoWidget = {
  tipo,
  visible: () => vistoAhora,
  alVisibilidad: (cb) => void oyentes.push(cb),
};

function aplicar(e: EstadoWidgets) {
  // En la raíz del widget, no en :root: `.wg` declara su propio valor por defecto y ganaría
  raiz.style.setProperty("--wg-opacidad", String(e.opacidad));
  raiz.classList.toggle("wg--edicion", e.edicion);
}

if (tipo in TITULOS) {
  q("titulo").textContent = TITULOS[tipo];
  document.title = `${TITULOS[tipo]} // NETRUNNER`;
  const limpiar = CONTENIDOS[tipo](q("cuerpo"), ctx);
  if (limpiar) window.addEventListener("pagehide", limpiar);
} else {
  q("titulo").textContent = "WIDGET DESCONOCIDO";
}

void widgetsEstado().then(aplicar).catch(() => {});
void alEstadoWidgets(aplicar);
if (tipo in TITULOS) {
  void alVisibilidadWidget((e) => {
    if (e.tipo === tipo) fijarVisible(e.visible);
  });
  void widgetALaVista(tipo).then(fijarVisible).catch(() => {});
}

// En modo edición, Esc sale de la edición (igual que LISTO en la barra)
window.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape" && raiz.classList.contains("wg--edicion")) void fijarEdicionWidgets(false).catch(() => {});
});

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
