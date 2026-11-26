// Widget TAREAS: las tareas pendientes por orden, con un cuadro HUD para marcarlas y una línea
// «+ TAREA» para crear otras. Los datos llegan por `notas://estado` (sin sondeos). Marcar una
// tarea la tacha ~400 ms (transición única) y después desaparece de la lista.
import { abrirTarjeta, alEvento, estadoNotas, guardarTarea, type EstadoNotas, type Tarea } from "../lib/ipc";
import { esc } from "../lib/texto";
import type { ContextoWidget } from "../widget";
import { escuchas, prepararCuerpo } from "./comun";

/** Alto de una fila y hueco (px); deben coincidir con widgets-contenido.css. */
const ALTO_FILA = 22;
const HUECO = 2;
const MS_TACHADO = 400;

export function montarTareas(cuerpo: HTMLElement, ctx: ContextoWidget): () => void {
  prepararCuerpo(cuerpo, "wgc--tareas");
  cuerpo.innerHTML = `
    <div class="ta">
      <button type="button" class="ta__cab" data-id="cab"><span class="ta__n" data-id="n">0 PENDIENTES</span></button>
      <div class="ta__lista" data-id="lista"></div>
      <div class="ta__nueva" data-id="nueva"></div>
    </div>`;
  const q = (id: string) => cuerpo.querySelector<HTMLElement>(`[data-id="${id}"]`)!;
  const elN = q("n");
  const lista = q("lista");
  const nueva = q("nueva");

  let tareas: Tarea[] = [];
  /** Tareas en pleno tachado: siguen en la lista hasta que acaba la transición. */
  const marcando = new Set<string>();
  const plazos = new Set<number>();
  let editando = false;
  let pendientePintar = false;
  let medicion = 0;

  const pendientes = () => tareas.filter((t) => !t.hecha && !t.borrada).sort((a, b) => a.orden - b.orden);
  const siguienteOrden = () => tareas.reduce((m, t) => Math.max(m, t.orden), 0) + 1;

  const pintarNueva = () => {
    nueva.innerHTML = `<button type="button" class="ta__mas" data-id="mas">+ TAREA</button>`;
  };

  const pintar = () => {
    pendientePintar = false;
    const lis = pendientes();
    const n = lis.filter((t) => !marcando.has(t.id)).length;
    elN.textContent = n === 1 ? "1 PENDIENTE" : `${n} PENDIENTES`;
    let html: string;
    if (lis.length === 0) {
      html = `<p class="ta__vacio">SIN TAREAS PENDIENTES</p>`;
    } else {
      let caben = Math.max(1, Math.floor((lista.clientHeight + HUECO) / (ALTO_FILA + HUECO)));
      let resto = 0;
      if (lis.length > caben) {
        // Hace falta la línea «+N MÁS»: ocupa una fila
        caben = Math.max(1, caben - 1);
        resto = lis.length - caben;
      }
      html =
        lis
          .slice(0, caben)
          .map(
            (t) => `<div class="ta__fila${marcando.has(t.id) ? " ta__fila--hecha" : ""}" data-tarea="${esc(t.id)}" title="${esc(t.texto)}">
              <i class="ta__cuadro"></i><span class="ta__texto">${esc(t.texto)}</span>
            </div>`,
          )
          .join("") + (resto > 0 ? `<button type="button" class="ta__resto" data-id="resto">+${resto} MÁS</button>` : "");
    }
    // No se repinta si nada cambió (no corta el tachado ni pierde el clic)
    if (lista.dataset.html !== html) {
      lista.dataset.html = html;
      lista.innerHTML = html;
    }
  };

  const pedirPintar = () => {
    if (!ctx.visible()) {
      pendientePintar = true;
      return;
    }
    pintar();
  };
  ctx.alVisibilidad((v) => {
    if (v && pendientePintar) pintar();
  });

  // Al cambiar el alto se recalcula cuántas filas caben (por evento, sin sondear)
  const observador = new ResizeObserver(() => {
    if (!ctx.visible()) return;
    cancelAnimationFrame(medicion);
    medicion = requestAnimationFrame(pintar);
  });
  observador.observe(lista);

  const recibir = (e: EstadoNotas) => {
    tareas = e.tareas;
    pedirPintar();
  };
  const escucha = escuchas();
  escucha.anadir(alEvento<EstadoNotas>("notas://estado", recibir));
  estadoNotas().then(recibir).catch(() => {});

  const marcar = (id: string) => {
    const t = tareas.find((x) => x.id === id);
    if (!t || marcando.has(id)) return;
    marcando.add(id);
    // El tachado es una transición CSS de la clase; al acabar se guarda y sale de la lista
    lista.querySelector(`[data-tarea="${CSS.escape(id)}"]`)?.classList.add("ta__fila--hecha");
    const p = window.setTimeout(() => {
      plazos.delete(p);
      void guardarTarea({ ...t, hecha: true })
        .catch(() => {})
        .finally(() => {
          marcando.delete(id);
          t.hecha = true;
          pedirPintar();
        });
    }, MS_TACHADO);
    plazos.add(p);
  };

  // --- Línea «+ TAREA» → campo de texto ---
  const cerrarCampo = () => {
    if (!editando) return;
    editando = false;
    pintarNueva();
  };
  const abrirCampo = () => {
    editando = true;
    nueva.innerHTML = `<input type="text" class="ta__campo" maxlength="200" placeholder="NUEVA TAREA" spellcheck="false" autocomplete="off" />`;
    const campo = nueva.querySelector<HTMLInputElement>("input")!;
    campo.addEventListener("keydown", (ev) => {
      // Las teclas son del campo: no llegan al manejador de widget.ts (Esc sale del modo edición)
      ev.stopPropagation();
      if (ev.key === "Enter") {
        const texto = campo.value.trim();
        if (!texto) return;
        campo.value = "";
        const t: Tarea = {
          id: crypto.randomUUID(),
          texto,
          hecha: false,
          orden: siguienteOrden(),
          creada: 0,
          modificada: 0,
          borrada: false,
        };
        tareas = [...tareas, t];
        pedirPintar();
        void guardarTarea(t).catch(() => {});
      } else if (ev.key === "Escape") {
        cerrarCampo();
      }
    });
    campo.addEventListener("blur", () => {
      if (campo.value.trim() === "") cerrarCampo();
    });
    campo.focus();
  };

  const alClic = (e: MouseEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest('[data-id="mas"]')) return abrirCampo();
    if (el.closest("input")) return;
    if (el.closest('[data-id="cab"], [data-id="resto"]')) return void abrirTarjeta("notas").catch(() => {});
    const fila = el.closest<HTMLElement>("[data-tarea]");
    if (fila?.dataset.tarea) marcar(fila.dataset.tarea);
  };
  cuerpo.addEventListener("click", alClic);

  pintarNueva();
  pintar();

  return () => {
    cancelAnimationFrame(medicion);
    observador.disconnect();
    escucha.limpiar();
    plazos.forEach((p) => clearTimeout(p));
    cuerpo.removeEventListener("click", alClic);
  };
}
