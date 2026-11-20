// Ventana de una tarjeta desacoplada (tarjeta.html?tipo=<tipo>): barra de título HUD propia y la
// vista ampliada de la tarjeta. El núcleo (tarjetas.rs) la crea y la destruye.
import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/hud.css";
import "./styles/consumo-ventana.css";
import "./styles/tarjeta.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { iniciarTema } from "./lib/tema";
import { montarVentanaMeteo } from "./modulos/tiempo/ventana";

iniciarTema();

type Vista = (contenedor: HTMLElement) => void | (() => void);

const TITULOS: Record<string, string> = { meteo: "METEO", notas: "NOTAS", sistema: "SISTEMA" };

/** Vista provisional: los encargos siguientes sustituyen la de cada tipo por la real. */
const provisional =
  (titulo: string): Vista =>
  (cont) => {
    cont.innerHTML = `
      <section class="marco tv__marco">
        <div class="marco__cabecera"><span></span><span class="marco__etiqueta">VENTANA</span></div>
        <div class="marco__cuerpo tv__vacio">VISTA AMPLIADA EN CONSTRUCCIÓN</div>
      </section>`;
    cont.querySelector(".marco__cabecera span")!.textContent = titulo;
  };

const VISTAS: Record<string, Vista> = {
  meteo: montarVentanaMeteo,
  notas: provisional("NOTAS"),
  sistema: provisional("SISTEMA"),
};

const tipo = new URLSearchParams(location.search).get("tipo") ?? "";
const titulo = TITULOS[tipo];
const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

if (titulo) {
  q("sistema").textContent = `NR // ${titulo}`;
  q("titulo").textContent = titulo;
  document.title = `${titulo} // NETRUNNER`;
  const limpiar = VISTAS[tipo](q("cuerpo"));
  if (limpiar) window.addEventListener("pagehide", limpiar);
} else {
  q("titulo").textContent = "TARJETA DESCONOCIDA";
}

// Entrada con un breve glitch/escaneo (sin él con movimiento reducido)
const raiz = q("raiz");
if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
  raiz.classList.add("tv--entrada");
  setTimeout(() => raiz.classList.remove("tv--entrada"), 400);
}

const ventana = getCurrentWindow();
q("minimizar").addEventListener("click", () => void ventana.minimize().catch(() => {}));
q("cerrar").addEventListener("click", () => void ventana.close().catch(() => {}));
