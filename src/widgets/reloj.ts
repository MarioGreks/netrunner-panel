// Widget RELOJ: hora grande, fecha, tiempo actual y próximos eventos. Se adapta al tamaño con
// container queries (widgets-contenido.css) y solo repinta cada segundo mientras se ve.
import {
  abrirTarjeta,
  alEvento,
  estadoCalendario,
  estadoMeteo,
  irAEventoPanel,
  obtenerConfig,
  type EstadoCalendario,
  type EstadoMeteo,
  type EventoCalendario,
} from "../lib/ipc";
import { iconoMeteo } from "../lib/iconos-meteo";
import { colorCategoria } from "../lib/categorias";
import { proximo } from "../ui/resumen";
import { dos, esc, recortar } from "../lib/texto";
import { cuandoEvento, fechaCorta } from "../lib/tiempo-texto";
import type { ContextoWidget } from "../widget";
import { escuchas, prepararCuerpo } from "./comun";

/** Eventos con hora que siguen hoy después del próximo (como mucho `n`). */
function siguientes(eventos: EventoCalendario[], actual: EventoCalendario | null, ahora: Date, n: number) {
  const finHoy = new Date(ahora.getFullYear(), ahora.getMonth(), ahora.getDate() + 1).getTime();
  const desde = actual ? new Date(actual.inicio).getTime() : 0;
  return eventos
    .filter((e) => {
      if (e.todo_el_dia || (actual && e.id === actual.id)) return false;
      const ini = new Date(e.inicio).getTime();
      return new Date(e.fin).getTime() > ahora.getTime() && ini < finHoy && ini >= desde;
    })
    .sort((a, b) => new Date(a.inicio).getTime() - new Date(b.inicio).getTime())
    .slice(0, n);
}

const cuadro = (ev: EventoCalendario) => {
  const color = ev.categorias.length ? colorCategoria(ev.categorias[0]) : "";
  return `<i class="rl__cuadro"${color ? ` style="--cat:${color}"` : ""}></i>`;
};

const fila = (ev: EventoCalendario, ahora: Date, clase: string) => {
  const c = cuandoEvento(ev, ahora);
  return `<div class="rl__ev ${clase}${c.alerta ? " rl__ev--alerta" : ""}" data-evid="${esc(ev.id)}" title="${esc(ev.titulo)}">
    ${cuadro(ev)}<b class="rl__cuando">${esc(c.texto)}</b><span class="rl__titulo">${esc(recortar(ev.titulo, 40))}</span>
  </div>`;
};

export function montarReloj(cuerpo: HTMLElement, ctx: ContextoWidget): () => void {
  prepararCuerpo(cuerpo, "wgc--reloj");
  cuerpo.innerHTML = `
    <div class="rl">
      <div class="rl__hora"><span class="rl__hm" data-id="hm">--:--</span><span class="rl__seg" data-id="seg">--</span></div>
      <div class="rl__linea">
        <span class="rl__fecha" data-id="fecha">--</span>
        <span class="rl__meteo" data-id="meteo"></span>
      </div>
      <div class="rl__prox" data-id="prox"></div>
      <div class="rl__sig" data-id="sig"></div>
    </div>`;
  const q = (id: string) => cuerpo.querySelector<HTMLElement>(`[data-id="${id}"]`)!;
  const elHm = q("hm");
  const elSeg = q("seg");
  const elFecha = q("fecha");
  const elMeteo = q("meteo");
  const elProx = q("prox");
  const elSig = q("sig");

  let meteo: EstadoMeteo | null = null;
  let agenda: EstadoCalendario | null = null;
  let tieneUrl = false;
  let fechaPintada = "";

  const poner = (el: HTMLElement, html: string) => {
    if (el.dataset.html !== html) {
      el.dataset.html = html;
      el.innerHTML = html;
    }
  };

  const pintarDatos = () => {
    const ahora = new Date();
    const a = meteo?.ciudad ? meteo.actual : null;
    poner(elMeteo, a ? `${iconoMeteo(a.icono, 16, "rl__icono")}<span>${Math.round(a.temperatura)}°</span>` : `<span class="rl__nd">--</span>`);

    const eventos = tieneUrl && agenda ? agenda.eventos : [];
    const ev = proximo(eventos, ahora);
    poner(
      elProx,
      ev
        ? fila(ev, ahora, "rl__ev--prox")
        : `<div class="rl__ev rl__ev--vacio"><span class="rl__nd">--</span><span class="rl__titulo">SIN EVENTOS</span></div>`,
    );
    poner(
      elSig,
      siguientes(eventos, ev && !ev.todo_el_dia ? ev : null, ahora, 2)
        .map((e) => fila(e, ahora, "rl__ev--sig"))
        .join(""),
    );
  };

  const pintarReloj = () => {
    const ahora = new Date();
    elHm.textContent = `${dos(ahora.getHours())}:${dos(ahora.getMinutes())}`;
    elSeg.textContent = dos(ahora.getSeconds());
    const fecha = fechaCorta(ahora);
    if (fecha !== fechaPintada) {
      fechaPintada = fecha;
      elFecha.textContent = fecha;
    }
    // Lo que depende del minuto (AHORA, eventos que acaban) se recalcula al cambiar de minuto
    if (ahora.getSeconds() === 0) pintarDatos();
  };

  // El reloj corre solo con el widget a la vista
  let temporizador: number | undefined;
  const parar = () => {
    clearInterval(temporizador);
    temporizador = undefined;
  };
  const arrancar = () => {
    parar();
    pintarReloj();
    pintarDatos();
    temporizador = window.setInterval(pintarReloj, 1000);
  };
  ctx.alVisibilidad((v) => (v ? arrancar() : parar()));

  const leerConfig = () =>
    obtenerConfig()
      .then((c) => {
        tieneUrl = !!c.ics_url && c.ics_url.trim() !== "";
        if (ctx.visible()) pintarDatos();
      })
      .catch(() => {});

  const escucha = escuchas();
  escucha.anadir(alEvento("config://cambio", () => void leerConfig()));
  escucha.anadir(
    alEvento<EstadoMeteo>("meteo://estado", (m) => {
      meteo = m;
      if (ctx.visible()) pintarDatos();
    }),
  );
  escucha.anadir(
    alEvento<EstadoCalendario>("calendario://estado", (s) => {
      agenda = s;
      void leerConfig(); // un cambio de URL llega con un estado nuevo
    }),
  );

  pintarReloj();
  pintarDatos();
  if (ctx.visible()) arrancar();
  void leerConfig().then(() =>
    estadoCalendario()
      .then((s) => {
        agenda = s;
        if (ctx.visible()) pintarDatos();
      })
      .catch(() => {}),
  );
  estadoMeteo()
    .then((m) => {
      meteo = m;
      if (ctx.visible()) pintarDatos();
    })
    .catch(() => {});

  // Fuera del modo edición (en él el cuerpo no recibe clics)
  const alClic = (e: MouseEvent) => {
    const f = (e.target as HTMLElement).closest<HTMLElement>("[data-evid]");
    if (f?.dataset.evid) void irAEventoPanel(f.dataset.evid);
    else void abrirTarjeta("meteo");
  };
  cuerpo.addEventListener("click", alClic);

  return () => {
    parar();
    escucha.limpiar();
    cuerpo.removeEventListener("click", alClic);
  };
}
