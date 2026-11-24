// Barra de edición de widgets (widgets-barra.html): catálogo para añadir y quitar, opacidad y
// LISTO. La crea y la destruye el núcleo (widgets.rs) mientras dura el modo edición.
import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/hud.css";
import "./styles/widgets-barra.css";
import {
  alEstadoWidgets,
  anadirWidget,
  fijarEdicionWidgets,
  fijarOpacidadWidgets,
  quitarWidget,
  widgetsEstado,
  type EstadoWidgets,
  type TipoWidget,
} from "./lib/ipc";
import { iniciarTema } from "./lib/tema";

iniciarTema();

const CATALOGO: Array<[TipoWidget, string]> = [
  ["reloj", "RELOJ"],
  ["sistema", "SISTEMA"],
  ["consumo", "CONSUMO"],
  ["tareas", "TAREAS"],
  ["cuentas", "CUENTAS ATRÁS"],
];
/** Pausa tras el último movimiento del deslizador antes de guardar en disco. */
const ESPERA_GUARDADO = 250;
/** Como mucho una orden de aplicación en vivo cada tanto mientras se arrastra. */
const ESPERA_VIVO = 50;

const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const deslizador = q<HTMLInputElement>("opacidad");
const valor = q<HTMLOutputElement>("valor");
const botones = new Map<TipoWidget, HTMLButtonElement>();
let activos = new Set<string>();

for (const [tipo, titulo] of CATALOGO) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "wb__tipo";
  b.addEventListener("click", () => {
    const accion = activos.has(tipo) ? quitarWidget(tipo) : anadirWidget(tipo);
    void accion.catch(() => {});
  });
  b.dataset.titulo = titulo;
  botones.set(tipo, b);
  q("catalogo").append(b);
}

function pintarBoton(tipo: TipoWidget, b: HTMLButtonElement) {
  const activo = activos.has(tipo);
  b.classList.toggle("wb__tipo--activo", activo);
  b.setAttribute("aria-pressed", String(activo));
  b.textContent = `${activo ? "●" : "+"} ${b.dataset.titulo}`;
  b.title = activo ? "Quitar este widget" : "Añadir este widget";
}

// Mientras el usuario mueve el deslizador, los ecos del estado no lo pisan
let editando = false;

function aplicar(e: EstadoWidgets) {
  activos = new Set(e.activos);
  for (const [tipo, b] of botones) pintarBoton(tipo, b);
  if (!editando) mostrarOpacidad(e.opacidad);
}

function mostrarOpacidad(o: number) {
  const pct = Math.round(o * 100);
  deslizador.value = String(pct);
  valor.textContent = `${pct}%`;
}

let temporizadorVivo: number | undefined;
let temporizadorGuardado: number | undefined;

deslizador.addEventListener("input", () => {
  const o = Number(deslizador.value) / 100;
  valor.textContent = `${deslizador.value}%`;
  editando = true;
  if (temporizadorVivo === undefined) {
    temporizadorVivo = window.setTimeout(() => {
      temporizadorVivo = undefined;
      void fijarOpacidadWidgets(Number(deslizador.value) / 100, false).catch(() => {});
    }, ESPERA_VIVO);
  }
  clearTimeout(temporizadorGuardado);
  temporizadorGuardado = window.setTimeout(() => {
    temporizadorGuardado = undefined;
    editando = false;
    void fijarOpacidadWidgets(o, true).catch(() => {});
  }, ESPERA_GUARDADO);
});

const salir = () => void fijarEdicionWidgets(false).catch(() => {});
q("listo").addEventListener("click", salir);
window.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape") salir();
});

void widgetsEstado().then(aplicar).catch(() => {});
void alEstadoWidgets(aplicar);
