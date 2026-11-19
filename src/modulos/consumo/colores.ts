// Colores de las gráficas de CONSUMO para <canvas>, que no entiende var(--x) ni se entera de los
// temas: se resuelven desde los tokens de tokens.css/temas.css y se vuelven a leer al cambiar de
// tema. Nada de hex sueltos: todo sale de la paleta del tema vigente.
import { EVENTO_TEMA } from "../../lib/tema";

/** Colores ya resueltos (cadenas CSS que el canvas entiende). */
export interface TemaGrafica {
  cian: string;
  amarillo: string;
  rojo: string;
  texto: string;
  tenue: string;
  fondo: string;
  /** Rejilla tenue y rejilla marcada. */
  rejilla: string;
  rejillaFuerte: string;
  /** Proyectos: el color de uno es siempre el mismo (hash de su nombre). */
  proyectos: string[];
  resto: string;
  otros: string;
}

/** Nombres especiales del núcleo: proyectos menores juntos y consumo no atribuido. */
const RESTO = "(resto)";
const OTROS = "(otros)";

// Variantes de los dos acentos del tema y mezclas entre ellos: distinguibles entre sí y cambian con el tema.
const PROYECTOS = [
  "var(--cian)",
  "var(--amarillo)",
  "color-mix(in srgb, var(--cian) 55%, var(--fondo))",
  "color-mix(in srgb, var(--amarillo) 55%, var(--fondo))",
  "color-mix(in srgb, var(--cian) 50%, var(--amarillo))",
  "color-mix(in srgb, var(--cian) 45%, var(--texto))",
  "color-mix(in srgb, var(--amarillo) 45%, var(--texto))",
  "color-mix(in srgb, var(--cian) 30%, var(--amarillo))",
];

let sonda: HTMLElement | null = null;
let cache: TemaGrafica | null = null;

/** Resuelve una expresión de color CSS (con var() y color-mix()) al color real que usa el canvas. */
function resolver(expr: string): string {
  if (!sonda) {
    sonda = document.createElement("span");
    sonda.style.display = "none";
    document.body.appendChild(sonda);
  }
  sonda.style.color = "";
  sonda.style.color = expr;
  return getComputedStyle(sonda).color;
}

export function temaGrafica(): TemaGrafica {
  if (cache) return cache;
  cache = {
    cian: resolver("var(--cian)"),
    amarillo: resolver("var(--amarillo)"),
    rojo: resolver("var(--rojo)"),
    texto: resolver("var(--texto)"),
    tenue: resolver("var(--texto-tenue)"),
    fondo: resolver("var(--fondo)"),
    rejilla: resolver("color-mix(in srgb, var(--texto) 9%, transparent)"),
    rejillaFuerte: resolver("color-mix(in srgb, var(--texto) 20%, transparent)"),
    proyectos: PROYECTOS.map(resolver),
    resto: resolver("var(--texto-tenue)"),
    otros: resolver("color-mix(in srgb, var(--texto-tenue) 55%, var(--fondo))"),
  };
  return cache;
}

// Los colores se releen solo cuando cambia el tema
window.addEventListener(EVENTO_TEMA, () => {
  cache = null;
});

/** Color con alfa (0..1) de un color ya resuelto. */
export const conAlfa = (color: string, alfa: number) => `color-mix(in srgb, ${color} ${Math.round(alfa * 100)}%, transparent)`;

function hash(nombre: string): number {
  let h = 2166136261;
  for (let i = 0; i < nombre.length; i++) h = Math.imul(h ^ nombre.charCodeAt(i), 16777619);
  return h >>> 0;
}

/** Color estable de un proyecto: el mismo en todas las gráficas, apartados y ventanas. */
export function colorProyecto(nombre: string, t: TemaGrafica = temaGrafica()): string {
  if (nombre === RESTO) return t.resto;
  if (nombre === OTROS) return t.otros;
  return t.proyectos[hash(nombre) % t.proyectos.length];
}
