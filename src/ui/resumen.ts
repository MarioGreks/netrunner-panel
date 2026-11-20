// Resumen en vivo de la franja de la cabecera: METEO, NEXT (próximo evento) y CPU.
import {
  alEvento,
  estadoCalendario,
  estadoMeteo,
  muestraSistema,
  obtenerConfig,
  type EstadoCalendario,
  type EstadoMeteo,
  type EventoCalendario,
  type MuestraSistema,
} from "../lib/ipc";
import { iconoMeteo } from "../lib/iconos-meteo";
import { dos, esc, recortar } from "../lib/texto";


/** Evento en curso o, si no hay, el siguiente con hora; si no, uno de día completo de hoy. */
export function proximo(eventos: EventoCalendario[], ahora: Date): EventoCalendario | null {
  const inicioHoy = new Date(ahora.getFullYear(), ahora.getMonth(), ahora.getDate()).getTime();
  const finHoy = inicioHoy + 86400000;
  let mejor: EventoCalendario | null = null;
  let mejorInicio = Infinity;
  for (const ev of eventos) {
    if (ev.todo_el_dia) continue;
    const ini = new Date(ev.inicio).getTime();
    const fin = new Date(ev.fin).getTime();
    if (fin <= ahora.getTime()) continue;
    if (ini < mejorInicio) {
      mejor = ev;
      mejorInicio = ini;
    }
  }
  if (mejor) return mejor;
  return (
    eventos.find((ev) => {
      if (!ev.todo_el_dia) return false;
      return new Date(ev.inicio).getTime() < finHoy && new Date(ev.fin).getTime() > inicioHoy;
    }) ?? null
  );
}

export function montarResumen(
  contenedor: HTMLElement,
  irA: (id: string) => void,
): void {
  contenedor.innerHTML = `
    <button class="resumen__celda" data-ir="tiempo" type="button" title="Ir a METEO">
      <span class="resumen__micro">METEO</span>
      <span class="resumen__valor" data-id="meteo">—</span>
    </button>
    <button class="resumen__celda resumen__celda--ancha" data-ir="tiempo" type="button" title="Ir a METEO">
      <span class="resumen__micro">NEXT</span>
      <span class="resumen__valor" data-id="next">—</span>
    </button>
    <button class="resumen__celda" data-ir="sistema" type="button" title="Ir a SISTEMA">
      <span class="resumen__micro">CPU</span>
      <span class="resumen__valor" data-id="cpu">—</span>
    </button>`;
  contenedor.querySelectorAll<HTMLElement>("[data-ir]").forEach((c) =>
    c.addEventListener("click", () => irA(c.dataset.ir!)),
  );
  const q = (id: string) => contenedor.querySelector<HTMLElement>(`[data-id="${id}"]`)!;
  const vMeteo = q("meteo");
  const vNext = q("next");
  const vCpu = q("cpu");

  let meteo: EstadoMeteo | null = null;
  let agenda: EstadoCalendario | null = null;
  let tieneUrl = false;
  let visible = true;

  const vacio = (el: HTMLElement) => {
    el.textContent = "—";
    el.classList.add("resumen__valor--vacio");
  };
  const lleno = (el: HTMLElement, html: string) => {
    el.innerHTML = html;
    el.classList.remove("resumen__valor--vacio");
  };

  const pintarMeteo = () => {
    const a = meteo?.ciudad ? meteo.actual : null;
    if (!a) return vacio(vMeteo);
    lleno(vMeteo, `${iconoMeteo(a.icono, 14, "resumen__icono")}<span>${Math.round(a.temperatura)}°</span>`);
  };

  const pintarNext = () => {
    const ev = tieneUrl && agenda ? proximo(agenda.eventos, new Date()) : null;
    if (!ev) return vacio(vNext);
    if (ev.todo_el_dia) return lleno(vNext, `<span class="resumen__hora">HOY ·</span> ${esc(recortar(ev.titulo, 14))}`);
    const ini = new Date(ev.inicio);
    lleno(
      vNext,
      `<span class="resumen__hora">${dos(ini.getHours())}:${dos(ini.getMinutes())}</span> ${esc(recortar(ev.titulo, 12))}`,
    );
  };

  const pintarCpu = (m: MuestraSistema | null) => {
    if (!m) return vacio(vCpu);
    lleno(vCpu, `${Math.round(m.cpu_total)}%`);
    vCpu.classList.toggle("resumen__valor--alerta", m.cpu_total > 90);
  };

  alEvento<EstadoMeteo>("meteo://estado", (m) => {
    meteo = m;
    pintarMeteo();
  });
  alEvento<EstadoCalendario>("calendario://estado", (s) => {
    agenda = s;
    // Un cambio de URL llega con un estado nuevo: se relee la configuración
    void leerConfig();
  });
  alEvento<MuestraSistema>("sistema://muestra", pintarCpu);
  alEvento("panel://mostrar", () => {
    visible = true;
    pintarNext();
  });
  alEvento("panel://ocultar", () => {
    visible = false;
  });

  const leerConfig = () =>
    obtenerConfig()
      .then((c) => {
        tieneUrl = !!c.ics_url && c.ics_url.trim() !== "";
        pintarNext();
      })
      .catch(() => {});

  // El próximo evento se recalcula cada minuto mientras el panel está visible
  window.setInterval(() => {
    if (visible) pintarNext();
  }, 60000);

  void leerConfig().then(() => {
    estadoCalendario().then((s) => { agenda = s; pintarNext(); }).catch(() => {});
  });
  estadoMeteo().then((m) => { meteo = m; pintarMeteo(); }).catch(() => {});
  muestraSistema().then(pintarCpu).catch(() => {});
}
