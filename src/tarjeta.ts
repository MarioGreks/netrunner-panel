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
import { montarVentanaNotas } from "./modulos/notas/ventana";
import { montarVentanaSistema } from "./modulos/sistema/ventana";

iniciarTema();

/** La limpieza puede devolver una promesa (p. ej. el último guardado del cuaderno). */
type Vista = (contenedor: HTMLElement) => void | (() => void | Promise<void>);

const TITULOS: Record<string, string> = { meteo: "METEO", notas: "NOTAS", sistema: "SISTEMA" };

const VISTAS: Record<string, Vista> = {
  meteo: montarVentanaMeteo,
  notas: montarVentanaNotas,
  sistema: montarVentanaSistema,
};

const tipo = new URLSearchParams(location.search).get("tipo") ?? "";
const titulo = TITULOS[tipo];
const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let limpiar: (() => void | Promise<void>) | null = null;

if (titulo) {
  q("sistema").textContent = `NR // ${titulo}`;
  q("titulo").textContent = titulo;
  document.title = `${titulo} // NETRUNNER`;
  limpiar = VISTAS[tipo](q("cuerpo")) || null;
  if (limpiar) window.addEventListener("pagehide", () => void limpiar?.());
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
// Antes de cerrar se espera al último guardado (como mucho 1,5 s, para no bloquear la ✕)
q("cerrar").addEventListener("click", async () => {
  const espera = new Promise<void>((r) => setTimeout(r, 1500));
  await Promise.race([Promise.resolve(limpiar?.()).catch(() => {}), espera]);
  await ventana.close().catch(() => {});
});
