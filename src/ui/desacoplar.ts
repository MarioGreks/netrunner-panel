// Desacoplar tarjetas: arrastrar la cabecera de una tarjeta fuera del panel la convierte en una
// ventana (núcleo: tarjetas.rs). Se usan eventos de puntero, no el arrastre HTML5, que choca con
// la captura de archivos de la ventana en Windows.
//
// Dentro del panel sigue al puntero un fantasma DOM; al cruzar el borde del panel el fantasma DOM
// se oculta y lo sustituye la ventana fantasma del sistema (la página no puede dibujar fuera de
// su ventana). Mientras dura el arrastre el núcleo no cierra el panel (arrastreTarjeta).
import {
  abrirTarjeta,
  abrirVentanaConsumo,
  alEvento,
  arrastreTarjeta,
  cerrarTarjeta,
  fantasmaMostrar,
  fantasmaMover,
  fantasmaOcultar,
  tarjetasAbiertas,
  type TipoTarjeta,
} from "../lib/ipc";

export interface OpcionesDesacoplable {
  tipo: TipoTarjeta | "consumo";
  titulo: string;
  /** Selector del asa dentro de la tarjeta (por defecto su cabecera). */
  asa?: string;
}

const UMBRAL = 6; // px que hay que mover antes de que sea un arrastre y no un clic
const IGNORAR = "button, input, textarea, select, a";
const MS_VUELTA = 180;
const reducido = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

let hayArrastre = false;

const fueraDelPanel = (x: number, y: number) => x < 0 || y < 0 || x >= window.innerWidth || y >= window.innerHeight;

/** Fantasma DOM: el marco con su cabecera y un cuerpo de líneas de escaneo. */
function crearFantasmaDom(titulo: string, r: DOMRect): HTMLElement {
  const f = document.createElement("div");
  f.className = "dsc-fantasma";
  f.style.width = `${r.width}px`;
  f.style.height = `${Math.min(r.height, 220)}px`;
  f.innerHTML = `<div class="dsc-fantasma__cab"><span></span><span>⧉</span></div><div class="dsc-fantasma__cuerpo"></div>`;
  f.querySelector("span")!.textContent = titulo;
  document.body.appendChild(f);
  return f;
}

/** Hace que la cabecera de `tarjeta` sea un asa para sacarla del panel. */
export function hacerDesacoplable(tarjeta: HTMLElement, opciones: OpcionesDesacoplable): void {
  const asa = opciones.asa ?? ".marco__cabecera";
  tarjeta.classList.add("dsc-desacoplable");
  const titulo = opciones.titulo;

  // Delegado en la tarjeta: algunas (la tira de METEO) repintan su contenido y perderían el oyente
  tarjeta.addEventListener("pointerdown", (e) => {
    const destino = e.target as Element;
    if (e.button !== 0 || hayArrastre || destino.closest(IGNORAR) || !destino.closest(asa)) return;
    const x0 = e.clientX;
    const y0 = e.clientY;
    const id = e.pointerId;
    const r0 = tarjeta.getBoundingClientRect();
    const agarre = { x: x0 - r0.left, y: y0 - r0.top };
    let empezado = false;
    let fuera = false;
    let ventanaVisible = false;
    let dom: HTMLElement | null = null;
    let ultimo = { x: x0, y: y0 };
    let cuadro = 0;

    const pintarCuadro = () => {
      cuadro = 0;
      if (!dom) return;
      dom.style.transform = `translate(${ultimo.x - agarre.x}px, ${ultimo.y - agarre.y}px)`;
      if (fuera && !ventanaVisible) {
        ventanaVisible = true;
        void fantasmaMostrar(titulo, r0.width, r0.height).catch(() => {});
      }
      if (!fuera && ventanaVisible) {
        ventanaVisible = false;
        void fantasmaOcultar().catch(() => {});
      }
      // Un mensaje por fotograma como mucho
      if (fuera) void fantasmaMover().catch(() => {});
    };

    const empezar = () => {
      empezado = true;
      hayArrastre = true;
      void arrastreTarjeta(true).catch(() => {});
      try {
        document.documentElement.setPointerCapture(id);
      } catch {
        /* el puntero ya no existe: pointercancel limpiará */
      }
      window.getSelection()?.removeAllRanges();
      document.body.classList.add("dsc-arrastrando");
      tarjeta.classList.add("dsc-origen");
      dom = crearFantasmaDom(titulo, r0);
    };

    const mover = (ev: PointerEvent) => {
      if (ev.pointerId !== id) return;
      if (!empezado) {
        if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < UMBRAL) return;
        empezar();
      }
      ultimo = { x: ev.clientX, y: ev.clientY };
      fuera = fueraDelPanel(ev.clientX, ev.clientY);
      dom?.classList.toggle("dsc-fantasma--fuera", fuera);
      if (!cuadro) cuadro = requestAnimationFrame(pintarCuadro);
    };

    /** Termina el arrastre. `soltarEn`: dónde se soltó (undefined = cancelado). */
    const terminar = async (soltarEn?: PointerEvent) => {
      document.removeEventListener("pointermove", mover);
      document.removeEventListener("pointerup", arriba);
      document.removeEventListener("pointercancel", cancelar);
      window.removeEventListener("keydown", tecla, true);
      cancelAnimationFrame(cuadro);
      if (!empezado) return;
      // El clic que sigue a soltar no debe activar nada de la cabecera
      const tragar = (c: Event) => c.stopPropagation();
      window.addEventListener("click", tragar, true);
      setTimeout(() => window.removeEventListener("click", tragar, true), 0);
      const quitarDom = () => {
        dom?.remove();
        dom = null;
      };
      try {
        const suelta = !!soltarEn && fueraDelPanel(soltarEn.clientX, soltarEn.clientY);
        if (suelta) {
          await fantasmaOcultar().catch(() => {});
          quitarDom();
          if (opciones.tipo === "consumo") await abrirVentanaConsumo();
          else {
            const d = window.devicePixelRatio || 1;
            await abrirTarjeta(opciones.tipo, soltarEn!.screenX * d, soltarEn!.screenY * d);
          }
        } else if (dom && !reducido()) {
          // Vuelve a su sitio y se desvanece
          const f = dom;
          f.classList.remove("dsc-fantasma--fuera");
          f.classList.add("dsc-fantasma--vuelta");
          f.style.transform = `translate(${r0.left}px, ${r0.top}px)`;
          setTimeout(() => f.remove(), MS_VUELTA);
          dom = null;
        }
      } catch {
        /* la ventana no se pudo abrir: la tarjeta sigue en el panel */
      } finally {
        quitarDom();
        void fantasmaOcultar().catch(() => {});
        void arrastreTarjeta(false).catch(() => {});
        document.body.classList.remove("dsc-arrastrando");
        tarjeta.classList.remove("dsc-origen");
        try {
          document.documentElement.releasePointerCapture(id);
        } catch {
          /* ya liberado */
        }
        hayArrastre = false;
      }
    };

    const arriba = (ev: PointerEvent) => ev.pointerId === id && void terminar(ev);
    const cancelar = (ev: PointerEvent) => ev.pointerId === id && void terminar();
    const tecla = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape" || !empezado) return;
      ev.stopPropagation();
      void terminar();
    };
    document.addEventListener("pointermove", mover);
    document.addEventListener("pointerup", arriba);
    document.addEventListener("pointercancel", cancelar);
    window.addEventListener("keydown", tecla, true);
  });
}

// --- Hueco «DESACOPLADO» ---

/** Avisa de si la tarjeta `tipo` tiene ventana: ahora (al montar) y en cada cambio. Devuelve la limpieza. */
export function alCambiarTarjeta(tipo: TipoTarjeta, cb: (abierta: boolean) => void): () => void {
  let vivo = true;
  let llegoEvento = false; // un evento es más reciente que la respuesta inicial
  tarjetasAbiertas()
    .then((l) => vivo && !llegoEvento && cb(l.includes(tipo)))
    .catch(() => vivo && !llegoEvento && cb(false));
  const baja = alEvento<string[]>("tarjetas://cambio", (l) => {
    llegoEvento = true;
    if (vivo) cb(l.includes(tipo));
  });
  return () => {
    vivo = false;
    void baja.then((f) => f()).catch(() => {});
  };
}

/** El marco que ocupa el sitio de una tarjeta que vive en su propia ventana. */
export function crearHueco(tipo: TipoTarjeta, nombre: string): HTMLElement {
  const h = document.createElement("section");
  h.className = "marco dsc-hueco";
  h.innerHTML = `
    <div class="banda"></div>
    <div class="dsc-hueco__cuerpo">
      <span class="dsc-hueco__estado">DESACOPLADO</span>
      <span class="dsc-hueco__nombre"></span>
      <button class="fm__btn" type="button" title="Cerrar la ventana y volver a poner la tarjeta aquí">TRAER</button>
    </div>
    <div class="banda"></div>`;
  h.querySelector(".dsc-hueco__nombre")!.textContent = nombre;
  h.querySelector("button")!.addEventListener("click", () => void cerrarTarjeta(tipo).catch(() => {}));
  return h;
}

/** Una tarjeta suelta (METEO): mientras su ventana esté abierta se oculta y el hueco ocupa su sitio. */
export function sustituirPorHueco(tarjeta: HTMLElement, tipo: TipoTarjeta, nombre: string): () => void {
  const hueco = crearHueco(tipo, nombre);
  hueco.hidden = true;
  tarjeta.before(hueco);
  const baja = alCambiarTarjeta(tipo, (abierta) => {
    hueco.hidden = !abierta;
    tarjeta.style.display = abierta ? "none" : "";
  });
  return () => {
    baja();
    hueco.remove();
  };
}

/** Una pestaña entera (NOTAS, SISTEMA): con la ventana abierta solo hay un hueco y el módulo ni se monta
 *  (así no se edita lo mismo en dos sitios ni se muestrea para nada). */
export function montarConHueco(
  contenedor: HTMLElement,
  tipo: TipoTarjeta,
  nombre: string,
  montarReal: (c: HTMLElement) => void | (() => void),
): () => void {
  let abierta: boolean | null = null;
  let limpieza: void | (() => void);
  const baja = alCambiarTarjeta(tipo, (a) => {
    if (a === abierta) return;
    abierta = a;
    limpieza?.();
    limpieza = undefined;
    contenedor.replaceChildren();
    if (a) contenedor.appendChild(crearHueco(tipo, nombre));
    else limpieza = montarReal(contenedor);
  });
  return () => {
    baja();
    limpieza?.();
  };
}
