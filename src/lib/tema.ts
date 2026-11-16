// Temas de color: aplica el tema y la opacidad del panel en el documento.
// El panel y la pestaña del borde son ventanas distintas; cada una llama a `iniciarTema`.
import { alEvento, obtenerConfig } from "./ipc";

export interface InfoTema {
  id: string;
  nombre: string;
}

export const TEMAS: InfoTema[] = [
  { id: "netrunner", nombre: "NETRUNNER" },
  { id: "neon", nombre: "NEÓN" },
  { id: "terminal", nombre: "TERMINAL" },
  { id: "sangre", nombre: "SANGRE" },
  { id: "ambar", nombre: "ÁMBAR" },
  { id: "hielo", nombre: "HIELO" },
];

export const TEMA_DEFECTO = "netrunner";
export const OPACIDAD_MIN = 0.55;
export const OPACIDAD_MAX = 1;
export const OPACIDAD_DEFECTO = 0.94;

/** Evento de ventana que avisa a los canvas de que deben releer los colores. */
export const EVENTO_TEMA = "netrunner:tema";

const CLAVE = "netrunner.tema";

export const temaValido = (t: unknown): string =>
  typeof t === "string" && TEMAS.some((x) => x.id === t) ? t : TEMA_DEFECTO;

export const limitarOpacidad = (o: unknown): number => {
  const n = typeof o === "number" && Number.isFinite(o) ? o : OPACIDAD_DEFECTO;
  return Math.min(OPACIDAD_MAX, Math.max(OPACIDAD_MIN, n));
};

export function aplicarTema(tema: string, opacidad: number): void {
  const raiz = document.documentElement;
  const t = temaValido(tema);
  const o = limitarOpacidad(opacidad);
  const cambio = raiz.dataset.tema !== t;
  raiz.dataset.tema = t;
  raiz.style.setProperty("--opacidad-panel", String(o));
  try {
    localStorage.setItem(CLAVE, JSON.stringify({ tema: t, opacidad: o }));
  } catch {
    // sin almacenamiento: solo se pierde la aplicación inmediata al arrancar
  }
  if (cambio) window.dispatchEvent(new Event(EVENTO_TEMA));
}

/** Aplica el último tema conocido (síncrono, antes del primer pintado). */
function aplicarGuardado(): void {
  try {
    const g = JSON.parse(localStorage.getItem(CLAVE) ?? "null");
    if (g) aplicarTema(g.tema, g.opacidad);
  } catch {
    // se queda el tema por defecto hasta leer la configuración
  }
}

/** Aplica el tema al arrancar y lo mantiene al día con `tema://cambio`. */
export function iniciarTema(): void {
  aplicarGuardado();
  void alEvento<{ tema: string; opacidad: number }>("tema://cambio", (c) => aplicarTema(c.tema, c.opacidad));
  obtenerConfig()
    .then((c) => aplicarTema(c.tema, c.opacidad))
    .catch(() => {});
}
