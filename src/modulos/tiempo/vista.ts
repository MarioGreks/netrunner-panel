// Módulo TIEMPO: reloj, tiempo meteorológico (Open-Meteo) y agenda (ICS de Outlook).
import {
  alEvento,
  estadoCalendario,
  estadoMeteo,
  obtenerConfig,
  sincronizarCalendario,
  type EstadoCalendario,
  type EstadoMeteo,
  type EventoCalendario,
} from "../../lib/ipc";
import { abrirAjustes } from "../../lib/navegacion";

const DIAS = ["DOMINGO", "LUNES", "MARTES", "MIÉRCOLES", "JUEVES", "VIERNES", "SÁBADO"];
const MESES = ["ENE", "FEB", "MAR", "ABR", "MAY", "JUN", "JUL", "AGO", "SEP", "OCT", "NOV", "DIC"];
const dos = (n: number) => String(n).padStart(2, "0");

const esc = (t: string) =>
  t.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

// --- Iconos meteorológicos: SVG de trazo fino ---
const NUBE = "M7 15h10a3.5 3.5 0 0 0 .4-6.98A5 5 0 0 0 7.5 7.5 3.75 3.75 0 0 0 7 15z";
const ICONOS: Record<string, string> = {
  sol: `<circle cx="12" cy="12" r="4"/><path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M4.9 19.1l1.8-1.8M17.3 6.7l1.8-1.8"/>`,
  luna: `<path d="M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5z"/>`,
  nubes: `<path d="M7 18h10a4 4 0 0 0 .5-7.97A5.5 5.5 0 0 0 7 9.5 4.25 4.25 0 0 0 7 18z"/>`,
  lluvia: `<path d="${NUBE}"/><path d="M8 18l-1 2.5M12 18l-1 2.5M16 18l-1 2.5"/>`,
  nieve: `<path d="${NUBE}"/><path d="M8 18.5v.01M12 20v.01M16 18.5v.01M10 21v.01M14 21.5v.01" stroke-width="2" stroke-linecap="round"/>`,
  tormenta: `<path d="${NUBE}"/><path d="M12.5 15.5l-2.2 3.2h3L11.3 22"/>`,
  niebla: `<path d="M4 8h16M6 12h14M4 16h14M8 20h10"/>`,
};
const icono = (nombre: string, tam = 24) =>
  `<svg class="tmp__icono" viewBox="0 0 24 24" width="${tam}" height="${tam}" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONOS[nombre] ?? ICONOS.nubes}</svg>`;

const ENGRANAJE = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M10.5 2.5h3l.5 2.4 1.9.8 2.1-1.4 2.1 2.1-1.4 2.1.8 1.9 2.4.5v3l-2.4.5-.8 1.9 1.4 2.1-2.1 2.1-2.1-1.4-1.9.8-.5 2.4h-3l-.5-2.4-1.9-.8-2.1 1.4-2.1-2.1 1.4-2.1-.8-1.9-2.4-.5v-3l2.4-.5.8-1.9-1.4-2.1 2.1-2.1 2.1 1.4 1.9-.8z"/></svg>`;
const GOTA = `<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M12 3C8 9 6 12 6 15a6 6 0 0 0 12 0c0-3-2-6-6-12z"/></svg>`;

const marco = (titulo: string, cabeceraDer: string, cuerpo: string, extra: string, id: string) => `
  <section class="marco ${extra}" data-id="${id}">
    <div class="marco__cabecera"><span>${titulo}</span><span class="tmp__der">${cabeceraDer}</span></div>
    <div class="marco__cuerpo" data-id="${id}-cuerpo">${cuerpo}</div>
  </section>`;

// --- Utilidades de fechas ---
const inicioDeDia = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const mismoDia = (a: Date, b: Date) => inicioDeDia(a).getTime() === inicioDeDia(b).getTime();
const hhmm = (d: Date) => `${dos(d.getHours())}:${dos(d.getMinutes())}`;
const redondear = (n: number) => `${Math.round(n)}°`;

function hace(iso: string | null): string {
  if (!iso) return "SIN SINCRONIZAR";
  const min = Math.floor((Date.now() - new Date(iso).getTime()) / 60000);
  if (min < 1) return "AHORA";
  if (min < 60) return `HACE ${min} MIN`;
  return `HACE ${Math.floor(min / 60)} H`;
}

function etiquetaDia(d: Date, hoy: Date): string {
  const dif = Math.round((inicioDeDia(d).getTime() - inicioDeDia(hoy).getTime()) / 86400000);
  if (dif === 0) return "HOY";
  if (dif === 1) return "MAÑANA";
  return `${DIAS[d.getDay()].slice(0, 3)} ${d.getDate()}`;
}

interface Linea {
  ev: EventoCalendario;
  inicio: Date;
  fin: Date;
}

/** Agrupa por día. Los eventos de día completo aparecen en cada día que cubren. */
function agrupar(eventos: EventoCalendario[], ahora: Date): Map<number, Linea[]> {
  const hoy = inicioDeDia(ahora).getTime();
  const limite = hoy + 14 * 86400000;
  const grupos = new Map<number, Linea[]>();
  const meter = (dia: number, l: Linea) => {
    if (dia < hoy || dia >= limite) return;
    const g = grupos.get(dia) ?? [];
    g.push(l);
    grupos.set(dia, g);
  };
  for (const ev of eventos) {
    const l: Linea = { ev, inicio: new Date(ev.inicio), fin: new Date(ev.fin) };
    if (ev.todo_el_dia) {
      for (let d = inicioDeDia(l.inicio); d < l.fin; d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1)) {
        meter(d.getTime(), l);
      }
    } else {
      meter(inicioDeDia(l.inicio).getTime(), l);
    }
  }
  return new Map([...grupos.entries()].sort((a, b) => a[0] - b[0]));
}

export function montar(contenedor: HTMLElement): void {
  contenedor.innerHTML = `
    <div class="tmp">
      <div class="tmp__reloj">
        <div class="tmp__hora" data-id="hora">--:--:--</div>
        <div class="tmp__fecha" data-id="fecha"></div>
      </div>
      <button class="tmp__engranaje" data-id="engranaje" type="button" title="Ajustes del calendario" aria-label="Abrir los ajustes">${ENGRANAJE}</button>

      ${marco("METEO", `<span class="marco__etiqueta" data-id="meteo-hace"></span>`, "", "marco--cian", "meteo")}
      ${marco(
        "AGENDA",
        `<span class="marco__etiqueta" data-id="agenda-hace"></span>
         <button class="tmp__sync" data-id="sync" type="button" title="Sincronizar ahora"><span class="tmp__giro" data-id="giro">⟳</span> SYNC</button>`,
        "",
        "",
        "agenda",
      )}
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const raiz = q("hora");
  const hora = raiz;
  const fecha = q("fecha");
  const cuerpoMeteo = q("meteo-cuerpo");
  const meteoHace = q("meteo-hace");
  const cuerpoAgenda = q("agenda-cuerpo");
  const agendaHace = q("agenda-hace");
  const giro = q("giro");

  let vivo = true;
  let meteo: EstadoMeteo | null = null;
  let agenda: EstadoCalendario | null = null;
  let tieneUrl = false;

  q("engranaje").addEventListener("click", () => abrirAjustes("calendario"));

  // --- Reloj ---
  const pintarReloj = () => {
    const d = new Date();
    hora.textContent = `${dos(d.getHours())}:${dos(d.getMinutes())}:${dos(d.getSeconds())}`;
    fecha.textContent = `${DIAS[d.getDay()]} ${d.getDate()} ${MESES[d.getMonth()]} ${d.getFullYear()}`;
  };

  // --- Meteo ---
  const pintarMeteo = () => {
    const m = meteo;
    meteoHace.textContent = m?.actualizado ? hace(m.actualizado) : "";
    if (!m || !m.ciudad) {
      cuerpoMeteo.innerHTML = `<div class="tmp__vacio">SIN CIUDAD CONFIGURADA<button class="tmp__btn" data-accion="meteo" type="button">CONFIGURAR</button></div>`;
      return;
    }
    const a = m.actual;
    let html = "";
    if (a) {
      html += `
        <div class="tmp__actual-meteo">
          <div class="tmp__icono-grande">${icono(a.icono, 56)}</div>
          <div class="tmp__temp">${redondear(a.temperatura)}</div>
          <div class="tmp__detalles">
            <span class="tmp__condicion">${esc(a.texto).toUpperCase()}</span>
            <span>SENSACIÓN <b>${redondear(a.sensacion)}</b></span>
            <span>HUMEDAD <b>${Math.round(a.humedad)}%</b></span>
            <span>VIENTO <b>${Math.round(a.viento)} km/h</b></span>
          </div>
        </div>`;
    } else {
      html += `<div class="tmp__vacio">${m.cargando ? "CARGANDO…" : "SIN DATOS"}</div>`;
    }
    html += `<div class="tmp__ciudad">${esc(m.ciudad.nombre).toUpperCase()}${m.ciudad.pais ? ` · ${esc(m.ciudad.pais).toUpperCase()}` : ""}</div>`;
    if (m.dias.length) {
      html += `<div class="tmp__dias">${m.dias
        .map((d) => {
          const [y, mes, dia] = d.fecha.split("-").map(Number);
          const nombre = DIAS[new Date(y, mes - 1, dia).getDay()].slice(0, 3);
          const lluvia = d.prob_lluvia === null ? "" : `${GOTA}${Math.round(d.prob_lluvia)}%`;
          return `<div class="tmp__dia" title="${esc(d.texto)}">
            <span class="tmp__dia-nombre">${nombre}</span>
            ${icono(d.icono, 26)}
            <span class="tmp__dia-max">${d.maxima === null ? "--" : redondear(d.maxima)}</span>
            <span class="tmp__dia-min">${d.minima === null ? "--" : redondear(d.minima)}</span>
            <span class="tmp__dia-lluvia">${lluvia}</span>
          </div>`;
        })
        .join("")}</div>`;
    }
    if (m.error) html += `<div class="tmp__error">${esc(m.error)}</div>`;
    cuerpoMeteo.innerHTML = html;
  };

  // --- Agenda ---
  const pintarAgenda = () => {
    const s = agenda;
    giro.classList.toggle("tmp__giro--activo", !!s?.sincronizando);
    agendaHace.textContent = tieneUrl || s?.ultima_sync ? hace(s?.ultima_sync ?? null) : "";
    if (!tieneUrl) {
      cuerpoAgenda.innerHTML = `<div class="tmp__vacio">SIN CALENDARIO CONFIGURADO<button class="tmp__btn" data-accion="calendario" type="button">CONFIGURAR</button></div>`;
      return;
    }
    let html = s?.error ? `<div class="tmp__error">${esc(s.error)}</div>` : "";
    const ahora = new Date();
    const grupos = agrupar(s?.eventos ?? [], ahora);
    if (grupos.size === 0) {
      html += `<div class="tmp__vacio">${s?.sincronizando && !s.ultima_sync ? "SINCRONIZANDO…" : "SIN EVENTOS EN 14 DÍAS"}</div>`;
    }
    // El siguiente evento con hora que aún no ha empezado
    let siguiente: Linea | null = null;
    for (const lineas of grupos.values()) {
      for (const l of lineas) {
        if (!l.ev.todo_el_dia && l.inicio > ahora && (!siguiente || l.inicio < siguiente.inicio)) siguiente = l;
      }
    }
    for (const [dia, lineas] of grupos) {
      html += `<div class="tmp__dia-grupo">${etiquetaDia(new Date(dia), ahora)}</div>`;
      for (const l of lineas) {
        const enCurso = !l.ev.todo_el_dia && l.inicio <= ahora && ahora < l.fin;
        const pasado = !l.ev.todo_el_dia && l.fin <= ahora && mismoDia(l.fin, ahora);
        const clases = ["tmp__evento", enCurso ? "tmp__evento--curso" : "", pasado ? "tmp__evento--pasado" : ""].join(" ");
        html += `
          <div class="${clases}">
            <span class="tmp__hora-ev">${l.ev.todo_el_dia ? "TODO EL DÍA" : hhmm(l.inicio)}</span>
            <span class="tmp__ev-texto">
              <span class="tmp__ev-titulo">${esc(l.ev.titulo)}${l === siguiente ? ` <span class="tmp__marca">SIGUIENTE</span>` : ""}</span>
              ${l.ev.ubicacion ? `<span class="tmp__ev-ubic">${esc(l.ev.ubicacion)}</span>` : ""}
            </span>
          </div>`;
      }
    }
    cuerpoAgenda.innerHTML = html;
  };

  // Los botones «CONFIGURAR» de los marcos vacíos llevan a su sección de AJUSTES
  contenedor.addEventListener("click", (e) => {
    const b = (e.target as HTMLElement).closest<HTMLElement>("[data-accion]");
    if (b?.dataset.accion === "meteo") abrirAjustes("meteo");
    else if (b?.dataset.accion === "calendario") abrirAjustes("calendario");
  });
  q("sync").addEventListener("click", () => {
    if (!tieneUrl) return abrirAjustes("calendario");
    void sincronizarCalendario();
  });

  // --- Ciclo de vida ---
  const suscripciones: (() => void)[] = [];
  let intervalo = 0;
  const limpiar = () => {
    if (!vivo) return;
    vivo = false;
    clearInterval(intervalo);
    suscripciones.forEach((u) => u());
  };
  const comprobar = () => {
    if (!contenedor.isConnected || !contenedor.contains(raiz)) {
      limpiar();
      return false;
    }
    return true;
  };
  const suscribir = (p: Promise<() => void>) =>
    p.then((un) => {
      if (vivo) suscripciones.push(un);
      else un();
    });

  const recibirAgenda = (s: EstadoCalendario) => {
    if (!comprobar()) return;
    agenda = s;
    pintarAgenda();
  };
  const recibirMeteo = (m: EstadoMeteo) => {
    if (!comprobar()) return;
    meteo = m;
    pintarMeteo();
  };

  // La configuración manda sobre si hay URL y ciudad
  const leerConfig = () =>
    obtenerConfig()
      .then((c) => {
        if (!vivo) return;
        tieneUrl = !!c.ics_url && c.ics_url.trim() !== "";
        pintarAgenda();
        pintarMeteo();
      })
      .catch(() => {});

  pintarReloj();
  pintarAgenda();
  pintarMeteo();
  intervalo = window.setInterval(() => {
    if (!comprobar()) return;
    pintarReloj();
    // La agenda y los «hace X min» se refrescan al cambiar el minuto
    if (new Date().getSeconds() === 0) {
      pintarAgenda();
      pintarMeteo();
    }
  }, 1000);

  suscribir(alEvento<EstadoCalendario>("calendario://estado", (s) => {
    // Un cambio de URL llega con un estado nuevo: se relee la config para saber si hay URL
    void leerConfig().then(() => recibirAgenda(s));
  }));
  suscribir(alEvento<EstadoMeteo>("meteo://estado", recibirMeteo));
  suscribir(alEvento("panel://mostrar", () => {
    if (!vivo) return;
    pintarReloj();
    pintarAgenda();
    pintarMeteo();
  }));

  void leerConfig().then(() => {
    estadoCalendario().then(recibirAgenda).catch(() => {});
    estadoMeteo().then(recibirMeteo).catch(() => {});
  });
}
