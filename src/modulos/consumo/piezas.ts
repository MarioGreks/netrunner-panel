// Piezas de CONSUMO compartidas por la pestaña (resumen) y la ventana flotante: los bloques de
// límite con su anillo y las pastillas de contexto por sesión. Sin temporizadores propios.
import { abrirChatConsumo, type EstadoConsumo, type NivelConsumo, type NivelContexto, type SesionContexto, type VentanaConsumo } from "../../lib/ipc";
import { crearAnillo, type Anillo } from "../../ui/grafica";

export const COLOR_NIVEL: Record<NivelConsumo, string> = {
  bien: "var(--cian)",
  encima: "var(--amarillo)",
  riesgo: "var(--rojo)",
};

/** Mismo formato que `falta` del núcleo: «1d 1h», «1h 01m», «10m». */
export function formatearFalta(s: number): string {
  const t = Math.max(0, Math.floor(s));
  const d = Math.floor(t / 86400);
  const h = Math.floor((t % 86400) / 3600);
  const m = Math.floor((t % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
  return `${m}m`;
}

export const tokensCorto = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 100000 ? 0 : 1)}K` : String(n));

// --- Bloques de límite (anillo + textos) ---
export interface BloqueLimite {
  anillo: Anillo;
  ritmo: HTMLElement;
  semana: HTMLElement;
  exceso: HTMLElement;
  falta: HTMLElement;
}

export const plantillaLimite = (clave: string, titulo: string) => `
  <div class="con__limite" data-id="${clave}">
    <span class="con__sub">${titulo}</span>
    <div class="con__anillo" data-id="${clave}-anillo"></div>
    <span class="con__exceso" data-id="${clave}-exceso" hidden></span>
    <span class="con__ritmo" data-id="${clave}-ritmo"></span>
    <span class="con__semana" data-id="${clave}-semana"></span>
    <span class="con__falta">RESET <b data-id="${clave}-falta">--</b></span>
  </div>`;

/** Crea el anillo y localiza los textos de un bloque pintado con `plantillaLimite(clave, …)`. */
export function crearBloqueLimite(raiz: HTMLElement, clave: string, tamano: number, segmentos: number): BloqueLimite {
  const q = (id: string) => raiz.querySelector<HTMLElement>(`[data-id="${clave}-${id}"]`)!;
  const anillo = crearAnillo(q("anillo"), { color: "var(--cian)", tamano, segmentos });
  anillo.fijar(null, true);
  return { anillo, ritmo: q("ritmo"), semana: q("semana"), exceso: q("exceso"), falta: q("falta") };
}

/** Pinta una ventana de límite en su bloque. `animar`: el anillo arranca desde 0. */
export function pintarLimite(b: BloqueLimite, v: VentanaConsumo, animar: boolean, reducido: boolean): void {
  const color = COLOR_NIVEL[v.nivel];
  const sinDato = !v.hayDatos;
  b.anillo.colorAnillo(color);
  b.anillo.colorValor(color);
  if (animar) b.anillo.fijar(0, true);
  b.anillo.fijar(sinDato ? null : v.pct, reducido);
  b.anillo.marca(sinDato ? null : v.esperado);
  b.anillo.tramo(sinDato || v.fin === null ? null : v.fin);
  b.ritmo.textContent = sinDato ? "SIN DATOS" : (v.textoRitmo ?? "");
  b.ritmo.classList.toggle("con__ritmo--encima", v.nivel === "encima");
  b.ritmo.classList.toggle("con__ritmo--riesgo", v.nivel === "riesgo");
  b.semana.textContent = sinDato ? "" : (v.textoSemana ?? v.textoSinLimite ?? "");
  b.exceso.hidden = sinDato || v.exceso <= 0;
  if (!b.exceso.hidden) b.exceso.textContent = `▲ EXCESO +${Math.round(v.exceso)}%`;
  b.falta.textContent = v.textoFalta;
}

/**
 * Cuenta atrás de los dos bloques (solo toca textContent). `pasado`: segundos desde que llegó el
 * estado. True si alguna ventana ya llegó a su reset (hay que releer el estado).
 */
export function pintarCuentaAtras(
  bloques: [BloqueLimite, VentanaConsumo][],
  pasado: number,
): boolean {
  let vencido = false;
  for (const [b, v] of bloques) {
    if (v.segundosHastaReset === null) continue;
    const falta = v.segundosHastaReset - pasado;
    if (falta <= 0) vencido = true;
    const t = formatearFalta(falta);
    if (b.falta.textContent !== t) b.falta.textContent = t;
  }
  return vencido;
}

/** Los dos bloques de un estado, en el orden que usa `pintarCuentaAtras`. */
export const ventanasDe = (e: EstadoConsumo, sem: BloqueLimite, cinco: BloqueLimite): [BloqueLimite, VentanaConsumo][] => [
  [sem, e.semanal],
  [cinco, e.cincoHoras],
];

// --- Pastillas de contexto por sesión ---
interface Pastilla {
  boton: HTMLButtonElement;
  relleno: HTMLElement;
  nombre: HTMLElement;
  tokens: HTMLElement;
}

export interface ListaContexto {
  /** Pinta (en sitio) las `max` primeras sesiones. */
  pintar(sesiones: SesionContexto[]): void;
}

const CLASES_NIVEL: NivelContexto[] = ["bien", "medio", "alto"];

/**
 * Lista de pastillas dentro de `contenedor`. Las nuevas se crean, las que ya no están se quitan y
 * las demás se actualizan en sitio. `cuenta` y `vacio` son opcionales.
 */
export function crearListaContexto(
  contenedor: HTMLElement,
  vacio: HTMLElement | null,
  cuenta: HTMLElement | null,
  max: number,
): ListaContexto {
  const pastillas = new Map<string, Pastilla>();
  return {
    pintar(sesiones) {
      const lista = sesiones.slice(0, max);
      if (cuenta) cuenta.textContent = String(lista.length);
      if (vacio) vacio.hidden = lista.length > 0;
      contenedor.hidden = lista.length === 0;
      const ids = lista.map((s) => s.id);
      for (const [id, p] of pastillas) {
        if (!ids.includes(id)) {
          p.boton.remove();
          pastillas.delete(id);
        }
      }
      lista.forEach((s, i) => {
        let p = pastillas.get(s.id);
        if (!p) {
          const boton = document.createElement("button");
          boton.className = "ctx";
          boton.innerHTML = `
            <span class="ctx__barra"><i class="ctx__relleno"></i></span>
            <span class="ctx__nombre"></span>
            <span class="ctx__tokens"></span>`;
          const id = s.id;
          boton.addEventListener("click", () => {
            if (boton.classList.contains("ctx--chat")) void abrirChatConsumo(id).catch(() => {});
          });
          p = {
            boton,
            relleno: boton.querySelector<HTMLElement>(".ctx__relleno")!,
            nombre: boton.querySelector<HTMLElement>(".ctx__nombre")!,
            tokens: boton.querySelector<HTMLElement>(".ctx__tokens")!,
          };
          pastillas.set(s.id, p);
        }
        const pastilla = p;
        if (contenedor.children[i] !== pastilla.boton) contenedor.insertBefore(pastilla.boton, contenedor.children[i] ?? null);
        pastilla.relleno.style.height = `${Math.round(Math.min(1, Math.max(0, s.fraccion)) * 100)}%`;
        CLASES_NIVEL.forEach((n) => pastilla.boton.classList.toggle(`ctx--${n}`, s.nivel === n));
        pastilla.boton.classList.toggle("ctx--chat", s.tieneChat);
        const titulo = `${s.nombre}\n${s.proyecto} · ${s.tokens.toLocaleString("es-ES")} tokens · ${Math.round(s.fraccion * 100)} %`;
        if (pastilla.boton.title !== titulo) pastilla.boton.title = titulo;
        if (pastilla.nombre.textContent !== s.nombre) pastilla.nombre.textContent = s.nombre;
        pastilla.tokens.textContent = tokensCorto(s.tokens);
      });
    },
  };
}
