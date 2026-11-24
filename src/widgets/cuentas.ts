// Widget CUENTAS ATRÁS: las cuentas atrás con su tiempo restante y una barra fina de progreso.
// Refresco cada 60 s (cada 1 s si alguna queda a menos de una hora) y solo con el widget a la vista.
import { alEvento, estadoCuentas, irAEventoPanel, type CuentaAtras } from "../lib/ipc";
import { colorCategoria } from "../lib/categorias";
import { dos, esc } from "../lib/texto";
import { MIN_ALERTA, restante } from "../lib/tiempo-texto";
import type { ContextoWidget } from "../widget";
import { escuchas, prepararCuerpo } from "./comun";

/** Alto de una fila y hueco entre filas (px); deben coincidir con widgets-contenido.css. */
const ALTO_FILA = 36;
const HUECO = 4;
const UNA_HORA = 3600000;

/** Con menos de una hora se enseñan los segundos: «12M 05S». */
function tiempo(ms: number): string {
  if (ms >= UNA_HORA) return restante(ms);
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${dos(Math.floor(s / 60))}M ${dos(s % 60)}S`;
}

export function montarCuentas(cuerpo: HTMLElement, ctx: ContextoWidget): () => void {
  prepararCuerpo(cuerpo, "wgc--cuentas");
  cuerpo.innerHTML = `<div class="cu" data-id="lista"></div>`;
  const lista = cuerpo.querySelector<HTMLElement>(".cu")!;

  let cuentas: CuentaAtras[] = [];
  let temporizador: number | undefined;

  const pintar = () => {
    clearTimeout(temporizador);
    temporizador = undefined;
    const ahora = Date.now();
    const futuras = cuentas
      .filter((c) => new Date(c.inicio).getTime() > ahora)
      .sort((a, b) => new Date(a.inicio).getTime() - new Date(b.inicio).getTime());

    let html: string;
    if (futuras.length === 0) {
      html = `<p class="cu__vacio">SIN CUENTAS ATRÁS · ★ en un evento</p>`;
    } else {
      const alto = lista.clientHeight;
      const caben = Math.max(1, Math.floor((alto + HUECO) / (ALTO_FILA + HUECO)));
      html = futuras
        .slice(0, caben)
        .map((c) => {
          const fin = new Date(c.inicio).getTime();
          const ms = fin - ahora;
          const alerta = ms < MIN_ALERTA * 60000;
          const color = c.categorias?.length ? colorCategoria(c.categorias[0]) : "";
          let barra = "";
          if (c.creada > 0 && fin > c.creada) {
            const p = Math.min(1, Math.max(0, (ahora - c.creada) / (fin - c.creada)));
            barra = `<div class="cu__barra"><i style="width:${(p * 100).toFixed(1)}%"></i></div>`;
          }
          return `<div class="cu__fila${alerta ? " cu__fila--alerta" : ""}" data-evid="${esc(c.id_evento)}" title="${esc(c.titulo)}">
            <div class="cu__cabeza">
              <i class="cu__cuadro"${color ? ` style="--cat:${color}"` : ""}></i>
              <span class="cu__titulo">${esc(c.titulo)}</span>
              <span class="cu__valor">${tiempo(ms)}</span>
            </div>${barra}
          </div>`;
        })
        .join("");
    }
    if (lista.dataset.html !== html) {
      lista.dataset.html = html;
      lista.innerHTML = html;
    }

    // Próximo refresco: 1 s si alguna queda a menos de una hora; si no, 60 s
    if (!ctx.visible() || futuras.length === 0) return;
    const cerca = futuras.some((c) => new Date(c.inicio).getTime() - ahora < UNA_HORA);
    temporizador = window.setTimeout(pintar, cerca ? 1000 : 60000);
  };

  const parar = () => {
    clearTimeout(temporizador);
    temporizador = undefined;
  };
  ctx.alVisibilidad((v) => (v ? pintar() : parar()));

  // Al cambiar el alto se recalcula cuántas filas caben (por evento, sin sondear)
  let medicion = 0;
  const observador = new ResizeObserver(() => {
    if (!ctx.visible()) return;
    cancelAnimationFrame(medicion);
    medicion = requestAnimationFrame(pintar);
  });
  observador.observe(lista);

  const escucha = escuchas();
  escucha.anadir(
    alEvento<CuentaAtras[]>("cuentas://estado", (c) => {
      cuentas = c;
      if (ctx.visible()) pintar();
    }),
  );
  estadoCuentas()
    .then((c) => {
      cuentas = c;
      if (ctx.visible()) pintar();
    })
    .catch(() => {});
  pintar();

  const alClic = (e: MouseEvent) => {
    const f = (e.target as HTMLElement).closest<HTMLElement>("[data-evid]");
    if (f?.dataset.evid) void irAEventoPanel(f.dataset.evid);
  };
  cuerpo.addEventListener("click", alClic);

  return () => {
    parar();
    cancelAnimationFrame(medicion);
    observador.disconnect();
    escucha.limpiar();
    cuerpo.removeEventListener("click", alClic);
  };
}
