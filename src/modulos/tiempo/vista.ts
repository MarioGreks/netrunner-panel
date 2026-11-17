// Módulo TIEMPO: tiempo meteorológico (Open-Meteo) y agenda (ICS de Outlook).
import {
  alEvento,
  categoriasOutlook,
  crearEvento,
  desmarcarCuenta,
  estadoCalendario,
  estadoCorreo,
  estadoCuentas,
  estadoMeteo,
  marcarCuenta,
  obtenerConfig,
  sincronizarCalendario,
  type CuentaAtras,
  type EstadoCorreo,
  type EstadoCalendario,
  type EstadoMeteo,
  type EventoCalendario,
  type HoraMeteo,
} from "../../lib/ipc";
import { abrirAjustes, alIrAEvento, tomarEventoPendiente } from "../../lib/navegacion";
import { alCambiarTiraRemota, recordarTira, tiraAbiertaGuardada } from "../../lib/interfaz";
import { iconoMeteo } from "../../lib/iconos-meteo";
import { colorCategoria } from "../../lib/categorias";

const DIAS = ["DOMINGO", "LUNES", "MARTES", "MIÉRCOLES", "JUEVES", "VIERNES", "SÁBADO"];
const MESES_LARGOS = ["ENERO", "FEBRERO", "MARZO", "ABRIL", "MAYO", "JUNIO", "JULIO", "AGOSTO", "SEPTIEMBRE", "OCTUBRE", "NOVIEMBRE", "DICIEMBRE"];
/** La agenda cubre unos 6 meses (igual que `DIAS_VENTANA` en calendario.rs) y se pinta de 14 en 14 días. */
const DIAS_AGENDA = 183;
const TANDA = 14;
const dos = (n: number) => String(n).padStart(2, "0");

const esc = (t: string) =>
  t.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

const icono = iconoMeteo;

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

const subtituloCategoria = (cats: string[]) =>
  cats.length
    ? `<span class="tmp__ev-cat" style="--cat:${colorCategoria(cats[0])}"><i class="tmp__cuadrado"></i>${esc(cats.join(" · ").toUpperCase())}</span>`
    : "";

/** «23D 04H 12M»; sin días «04H 12M»; sin horas «12M». */
function restante(ms: number): string {
  const min = Math.max(0, Math.floor(ms / 60000));
  const d = Math.floor(min / 1440);
  const h = Math.floor((min % 1440) / 60);
  const m = min % 60;
  if (d > 0) return `${d}D ${dos(h)}H ${dos(m)}M`;
  if (h > 0) return `${dos(h)}H ${dos(m)}M`;
  return `${dos(m)}M`;
}

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
  const limite = hoy + DIAS_AGENDA * 86400000;
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

/** METEO plegada o desplegada. El estado vive en lib/interfaz (se guarda y se sincroniza entre equipos); esta copia es la del módulo. */
let tiraAbierta = false;

/** Gráfica de 24 h en SVG: temperatura (línea y relleno, primario), lluvia (barras, secundario),
 *  etiquetas cada 3 h y máxima y mínima marcadas. Los colores salen de clases CSS con variables
 *  de tema, así que el cambio de tema no obliga a redibujar. */
function grafica24h(horas: HoraMeteo[]): string {
  const W = 340;
  const H = 108;
  const PX = 14;
  const Y_ALTO = 24;
  const Y_BAJO = 74;
  const BASE = 84;
  const BARRA_MAX = 28;
  const n = horas.length;
  if (n < 2) return `<div class="tmp__g-vacio">SIN DATOS HORARIOS</div>`;
  const x = (i: number) => PX + (i * (W - 2 * PX)) / (n - 1);
  const pts = horas.flatMap((h, i) => (h.temperatura === null ? [] : [{ i, t: h.temperatura }]));
  let capas = "";
  if (pts.length >= 2) {
    let tmin = Math.min(...pts.map((p) => p.t));
    let tmax = Math.max(...pts.map((p) => p.t));
    if (tmax - tmin < 1) {
      tmax += 0.5;
      tmin -= 0.5;
    }
    const y = (t: number) => Y_BAJO - ((t - tmin) / (tmax - tmin)) * (Y_BAJO - Y_ALTO);
    const linea = pts.map((p, k) => `${k ? "L" : "M"}${x(p.i).toFixed(1)} ${y(p.t).toFixed(1)}`).join("");
    const primero = pts[0];
    const ultimo = pts[pts.length - 1];
    capas += `<path class="tmp__g-area" d="${linea}L${x(ultimo.i).toFixed(1)} ${BASE}L${x(primero.i).toFixed(1)} ${BASE}Z"/>`;
    capas += `<path class="tmp__g-linea" d="${linea}"/>`;
    const pMax = pts.reduce((a, b) => (b.t > a.t ? b : a));
    const pMin = pts.reduce((a, b) => (b.t < a.t ? b : a));
    const marca = (p: { i: number; t: number }, etiqueta: string, arriba: boolean) => {
      const px = x(p.i);
      const py = y(p.t);
      const ancla = px < 44 ? "start" : px > W - 44 ? "end" : "middle";
      const ty = arriba ? Math.max(8, py - 6) : Math.min(BASE - 2, py + 11);
      return `<circle class="tmp__g-pico" cx="${px.toFixed(1)}" cy="${py.toFixed(1)}" r="2.6"/>
        <text class="tmp__g-txt tmp__g-txt--dato" x="${px.toFixed(1)}" y="${ty.toFixed(1)}" text-anchor="${ancla}">${etiqueta} ${redondear(p.t)}</text>`;
    };
    capas += marca(pMax, "MÁX", true);
    if (pMin.i !== pMax.i) capas += marca(pMin, "MÍN", false);
  }
  const barras = horas
    .map((h, i) => {
      const p = h.prob_lluvia ?? 0;
      if (p <= 0) return "";
      const alto = Math.max(1, (p / 100) * BARRA_MAX);
      return `<rect class="tmp__g-barra" x="${(x(i) - 1.3).toFixed(1)}" y="${(BASE - alto).toFixed(1)}" width="2.6" height="${alto.toFixed(1)}"><title>${Math.round(p)}% de lluvia</title></rect>`;
    })
    .join("");
  const etiquetas = horas
    .map((h, i) => {
      const hh = Number(h.hora.slice(11, 13));
      return hh % 3 === 0
        ? `<text class="tmp__g-txt" x="${x(i).toFixed(1)}" y="${H - 4}" text-anchor="middle">${dos(hh)}h</text><line class="tmp__g-marca" x1="${x(i).toFixed(1)}" x2="${x(i).toFixed(1)}" y1="${BASE}" y2="${BASE + 3}"/>`
        : "";
    })
    .join("");
  return `<svg class="tmp__grafica" viewBox="0 0 ${W} ${H}" role="img" aria-label="Previsión de las próximas 24 horas">
    <line class="tmp__g-base" x1="${PX}" x2="${W - PX}" y1="${BASE}" y2="${BASE}"/>
    ${barras}${capas}${etiquetas}
  </svg>`;
}

export function montar(contenedor: HTMLElement): void {
  tiraAbierta = tiraAbiertaGuardada();
  contenedor.innerHTML = `
    <div class="tmp">
      <section class="marco marco--cian tmp__tira" data-id="meteo"></section>
      ${marco(
        "AGENDA",
        `<span class="marco__etiqueta" data-id="agenda-hace"></span>
         <button class="tmp__engranaje" data-id="engranaje" type="button" title="Ajustes del calendario" aria-label="Abrir los ajustes">${ENGRANAJE}</button>
         <button class="tmp__sync tmp__nuevo-btn" data-id="nuevo-btn" type="button" title="Comprobando Outlook…" disabled>+ EVENTO</button>
         <button class="tmp__sync tmp__hoy" data-id="hoy" type="button" title="Volver a hoy" hidden>HOY</button>
         <button class="tmp__sync" data-id="sync" type="button" title="Sincronizar ahora"><span class="tmp__giro" data-id="giro">⟳</span> SYNC</button>`,
        `<form class="tmp__nuevo" data-id="nuevo" autocomplete="off" hidden>
           <input class="tmp__campo" data-id="n-titulo" type="text" placeholder="Título" maxlength="255" aria-label="Título" />
           <div class="tmp__n-fila">
             <input class="tmp__campo" data-id="n-fecha" type="date" aria-label="Fecha" />
             <label class="tmp__n-todo"><input data-id="n-todo" type="checkbox" /> Todo el día</label>
           </div>
           <div class="tmp__n-fila" data-id="n-horas">
             <label class="tmp__n-hora">DE <input class="tmp__campo" data-id="n-ini" type="time" aria-label="Hora de inicio" /></label>
             <label class="tmp__n-hora">A <input class="tmp__campo" data-id="n-fin" type="time" aria-label="Hora de fin" /></label>
           </div>
           <input class="tmp__campo" data-id="n-ubic" type="text" placeholder="Ubicación (opcional)" maxlength="255" aria-label="Ubicación" />
           <div class="tmp__n-cats-caja" hidden><span class="tmp__n-etq">CATEGORÍA</span><div class="tmp__n-cats" data-id="n-cats"></div></div>
           <div class="tmp__n-msg" data-id="n-msg" role="status"></div>
           <div class="tmp__n-fila tmp__n-fila--der">
             <button class="tmp__btn" data-id="n-cancelar" type="button">CANCELAR</button>
             <button class="tmp__btn tmp__btn--ok" data-id="n-crear" type="submit">CREAR</button>
           </div>
         </form>
         <div class="tmp__error" data-id="agenda-error" hidden></div>
         <div class="tmp__aviso" data-id="aviso" hidden></div>
         <div class="tmp__cuentas" data-id="cuentas" hidden></div>
         <div class="tmp__scroll" data-id="agenda-scroll"></div>`,
        "tmp__marco-agenda",
        "agenda",
      )}
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const tira = q("meteo");
  const raiz = tira;
  const errorAgenda = q("agenda-error");
  const cuentasEl = q("cuentas");
  const avisoEl = q("aviso");
  const scroll = q("agenda-scroll");
  const botonHoy = q<HTMLButtonElement>("hoy");
  const agendaHace = q("agenda-hace");
  const giro = q("giro");

  let vivo = true;
  let meteo: EstadoMeteo | null = null;
  let agenda: EstadoCalendario | null = null;
  let tieneUrl = false;
  let cuentas: CuentaAtras[] = [];
  let marcadas = new Set<string>();

  // Estado del pintado por tandas
  interface Entrada {
    dia: number;
    idx: number;
    lineas: Linea[];
  }
  let entradas: Entrada[] = [];
  let porId = new Map<string, EventoCalendario>();
  let siguiente: Linea | null = null;
  let ahoraPintado = new Date();
  let diasPintados = TANDA;
  let pintadas = 0;
  let mesPrevio = 0;

  q("engranaje").addEventListener("click", () => abrirAjustes("calendario"));

  // --- Meteo ---
  const pintarMeteo = () => {
    const m = meteo;
    if (!m || !m.ciudad) {
      tira.title = "";
      tira.classList.remove("tmp__tira--abierta", "tmp__tira--clicable");
      tira.removeAttribute("aria-expanded");
      tira.innerHTML = `<button class="tmp__sin-ciudad" data-accion="meteo" type="button">SIN CIUDAD · CONFIGURAR</button>`;
      return;
    }
    const a = m.actual;
    const hoy = m.dias[0];
    const ciudad = `${esc(m.ciudad.nombre).toUpperCase()}${m.ciudad.pais ? ` · ${esc(m.ciudad.pais).toUpperCase()}` : ""}`;
    tira.title = a
      ? `${a.texto}
Sensación ${redondear(a.sensacion)}
Humedad ${Math.round(a.humedad)}%
Viento ${Math.round(a.viento)} km/h${m.actualizado ? `
Actualizado ${hace(m.actualizado).toLowerCase()}` : ""}`
      : "";
    const izq = a
      ? `<span class="tmp__t-icono">${icono(a.icono, 24)}</span>
         <span class="tmp__t-temp-col"><span class="tmp__temp">${redondear(a.temperatura)}</span><span class="tmp__ciudad">${ciudad}</span></span>`
      : `<span class="tmp__t-temp-col"><span class="tmp__ciudad">${m.cargando ? "CARGANDO…" : m.error ? esc(m.error) : "SIN DATOS"}</span><span class="tmp__ciudad">${ciudad}</span></span>`;
    const centro = hoy
      ? `<span>MÁX <b>${hoy.maxima === null ? "--" : redondear(hoy.maxima)}</b></span>
         <span>MÍN <b>${hoy.minima === null ? "--" : redondear(hoy.minima)}</b></span>
         <span class="tmp__t-lluvia">${GOTA}<b>${hoy.prob_lluvia === null ? "--" : `${Math.round(hoy.prob_lluvia)}%`}</b></span>`
      : "";
    const dias = m.dias
      .slice(0, 5)
      .map((d) => {
        const [y, mes, dia] = d.fecha.split("-").map(Number);
        const nombre = DIAS[new Date(y, mes - 1, dia).getDay()].slice(0, 3);
        return `<span class="tmp__dia" title="${esc(d.texto)}">
          <span class="tmp__dia-nombre">${nombre}</span>
          ${icono(d.icono, 16)}
          <span class="tmp__dia-max">${d.maxima === null ? "--" : redondear(d.maxima)}</span>
        </span>`;
      })
      .join("");
    // Detalle que se despliega: condición actual, gráfica de 24 h y los 5 días en grande
    const detalleActual = a
      ? `<div class="tmp__d-actual">
          <span class="tmp__d-cond">${esc(a.texto).toUpperCase()}</span>
          <span>SENSACIÓN <b>${redondear(a.sensacion)}</b></span>
          <span>HUMEDAD <b>${Math.round(a.humedad)}%</b></span>
          <span>VIENTO <b>${Math.round(a.viento)} km/h</b></span>
        </div>`
      : "";
    const diasGrandes = m.dias
      .slice(0, 5)
      .map((d, i) => {
        const [y, mes, dia] = d.fecha.split("-").map(Number);
        const nombre = i === 0 ? "HOY" : DIAS[new Date(y, mes - 1, dia).getDay()].slice(0, 3);
        return `<div class="tmp__dg">
          <span class="tmp__dg-nombre">${nombre}</span>
          <span class="tmp__dg-icono">${icono(d.icono, 28)}</span>
          <span class="tmp__dg-cond">${esc(d.texto)}</span>
          <span class="tmp__dg-temp"><b>${d.maxima === null ? "--" : redondear(d.maxima)}</b> <i>${d.minima === null ? "--" : redondear(d.minima)}</i></span>
          <span class="tmp__dg-lluvia">${GOTA}${d.prob_lluvia === null ? "--" : `${Math.round(d.prob_lluvia)}%`}</span>
        </div>`;
      })
      .join("");
    tira.classList.toggle("tmp__tira--abierta", tiraAbierta);
    tira.classList.add("tmp__tira--clicable");
    tira.setAttribute("aria-expanded", String(tiraAbierta));
    tira.innerHTML = `
      <div class="tmp__t-fila">
        <div class="tmp__t-izq">${izq}</div>
        <div class="tmp__t-centro">${centro}</div>
        <div class="tmp__dias">${dias}</div>
      </div>
      <span class="tmp__t-flecha" aria-hidden="true">${tiraAbierta ? "▴" : "▾"}</span>
      <div class="tmp__detalle">
        <div class="tmp__detalle-in">
          ${detalleActual}
          ${grafica24h(m.por_horas ?? [])}
          <div class="tmp__dg-fila">${diasGrandes}</div>
        </div>
      </div>`;
  };

  const pintarTiraAbierta = () => {
    tira.classList.toggle("tmp__tira--abierta", tiraAbierta);
    tira.setAttribute("aria-expanded", String(tiraAbierta));
    const flecha = tira.querySelector(".tmp__t-flecha");
    if (flecha) flecha.textContent = tiraAbierta ? "▴" : "▾";
  };

  // El clic en la tira (salvo en el botón de configurar) la despliega o pliega en el sitio
  tira.addEventListener("click", (e) => {
    if ((e.target as HTMLElement).closest("[data-accion]") || !meteo?.ciudad) return;
    tiraAbierta = !tiraAbierta;
    recordarTira(tiraAbierta);
    pintarTiraAbierta();
  });

  // --- Agenda ---
  const centinela = document.createElement("div");
  centinela.className = "tmp__centinela";
  const observador = new IntersectionObserver(
    (es) => {
      if (es.some((e) => e.isIntersecting)) ampliar();
    },
    { root: scroll, rootMargin: "0px 0px 240px 0px" },
  );

  const htmlEvento = (l: Linea): string => {
    const ev = l.ev;
    const ahora = ahoraPintado;
    const enCurso = !ev.todo_el_dia && l.inicio <= ahora && ahora < l.fin;
    const pasado = !ev.todo_el_dia && l.fin <= ahora && mismoDia(l.fin, ahora);
    const futuro = l.inicio > ahora;
    const clases = ["tmp__evento", enCurso ? "tmp__evento--curso" : "", pasado ? "tmp__evento--pasado" : ""].join(" ");
    const marca = marcadas.has(ev.id);
    const estrella =
      futuro && ev.id
        ? `<button class="tmp__estrella${marca ? " tmp__estrella--on" : ""}" type="button" data-evid="${esc(ev.id)}" aria-pressed="${marca}" title="${marca ? "Quitar la cuenta atrás" : "Cuenta atrás"}">★</button>`
        : "";
    return `
      <div class="${clases}" data-evid="${esc(ev.id)}">
        <span class="tmp__hora-ev">${ev.todo_el_dia ? "TODO EL DÍA" : hhmm(l.inicio)}</span>
        <span class="tmp__ev-texto">
          <span class="tmp__ev-fila"><span class="tmp__ev-t" title="${esc(ev.titulo)}">${esc(ev.titulo)}</span>${l === siguiente ? `<span class="tmp__marca">SIGUIENTE</span>` : ""}</span>
          ${subtituloCategoria(ev.categorias ?? [])}
          ${ev.ubicacion ? `<span class="tmp__ev-ubic">${esc(ev.ubicacion)}</span>` : ""}
        </span>
        ${estrella}
      </div>`;
  };

  /** Pinta (sin borrar nada) las entradas aún no pintadas que caen dentro de `diasPintados`. */
  const anexar = () => {
    let html = "";
    while (pintadas < entradas.length && entradas[pintadas].idx < diasPintados) {
      const e = entradas[pintadas++];
      const d = new Date(e.dia);
      const mes = d.getFullYear() * 12 + d.getMonth();
      if (mes !== mesPrevio) {
        mesPrevio = mes;
        html += `<div class="tmp__mes"><span>${MESES_LARGOS[d.getMonth()]} ${d.getFullYear()}</span></div>`;
      }
      html += `<div class="tmp__dia-grupo">${etiquetaDia(d, ahoraPintado)}</div>`;
      html += e.lineas.map(htmlEvento).join("");
    }
    centinela.insertAdjacentHTML("beforebegin", html);
    observador.unobserve(centinela);
    if (diasPintados >= DIAS_AGENDA) {
      centinela.textContent = "FIN DE LA AGENDA";
    } else {
      // Si el centinela sigue a la vista, volver a observarlo dispara otra tanda
      observador.observe(centinela);
    }
  };

  function ampliar() {
    if (diasPintados >= DIAS_AGENDA) return;
    diasPintados = Math.min(diasPintados + TANDA, DIAS_AGENDA);
    anexar();
  }

  const actualizarBotonHoy = () => {
    botonHoy.hidden = scroll.scrollTop < 60;
  };

  const pintarAgenda = () => {
    const s = agenda;
    giro.classList.toggle("tmp__giro--activo", !!s?.sincronizando);
    agendaHace.textContent = tieneUrl || s?.ultima_sync ? hace(s?.ultima_sync ?? null) : "";
    errorAgenda.hidden = !(tieneUrl && s?.error);
    errorAgenda.textContent = tieneUrl && s?.error ? s.error : "";
    observador.unobserve(centinela);
    const antes = scroll.scrollTop;
    scroll.replaceChildren();
    entradas = [];
    porId = new Map();
    pintadas = 0;
    if (!tieneUrl) {
      scroll.innerHTML = `<div class="tmp__vacio">SIN CALENDARIO CONFIGURADO<button class="tmp__btn" data-accion="calendario" type="button">CONFIGURAR</button></div>`;
      actualizarBotonHoy();
      return;
    }
    ahoraPintado = new Date();
    const ahora = ahoraPintado;
    const hoy = inicioDeDia(ahora).getTime();
    const grupos = agrupar(s?.eventos ?? [], ahora);
    for (const ev of s?.eventos ?? []) if (ev.id) porId.set(ev.id, ev);
    if (grupos.size === 0) {
      scroll.innerHTML = `<div class="tmp__vacio">${s?.sincronizando && !s.ultima_sync ? "SINCRONIZANDO…" : "SIN EVENTOS EN 6 MESES"}</div>`;
      actualizarBotonHoy();
      return;
    }
    entradas = [...grupos].map(([dia, lineas]) => ({ dia, lineas, idx: Math.round((dia - hoy) / 86400000) }));
    // El siguiente evento con hora que aún no ha empezado
    siguiente = null;
    for (const lineas of grupos.values()) {
      for (const l of lineas) {
        if (!l.ev.todo_el_dia && l.inicio > ahora && (!siguiente || l.inicio < siguiente.inicio)) siguiente = l;
      }
    }
    mesPrevio = ahora.getFullYear() * 12 + ahora.getMonth();
    centinela.textContent = "";
    scroll.append(centinela);
    anexar();
    scroll.scrollTop = antes;
    actualizarBotonHoy();
  };

  // --- Cuentas atrás ---
  const pintarCuentas = () => {
    const ahora = Date.now();
    const lista = cuentas.filter((c) => new Date(c.inicio).getTime() > ahora);
    cuentasEl.hidden = lista.length === 0;
    if (lista.length === 0) {
      cuentasEl.replaceChildren();
      return;
    }
    cuentasEl.innerHTML = `
      <div class="tmp__cuentas-cab">CUENTAS ATRÁS</div>
      <div class="tmp__cuentas-lista">${lista
        .map(
          (c) => `<button class="tmp__cuenta" type="button" data-evid="${esc(c.id_evento)}" title="${esc(c.titulo)}">
            <span class="tmp__cuenta-texto">
              <span class="tmp__cuenta-titulo">${esc(c.titulo)}</span>
              ${subtituloCategoria(c.categorias ?? [])}
            </span>
            <span class="tmp__cuenta-tiempo">${restante(new Date(c.inicio).getTime() - ahora)}</span>
          </button>`,
        )
        .join("")}</div>`;
  };

  /** Refleja `marcadas` en las estrellas ya pintadas, sin repintar la agenda. */
  const actualizarEstrellas = () => {
    scroll.querySelectorAll<HTMLElement>(".tmp__estrella").forEach((b) => {
      const on = marcadas.has(b.dataset.evid ?? "");
      b.classList.toggle("tmp__estrella--on", on);
      b.setAttribute("aria-pressed", String(on));
      b.title = on ? "Quitar la cuenta atrás" : "Cuenta atrás";
    });
  };

  let temporizadorAviso = 0;
  const avisar = (texto: string, ok = false, ms = 3500) => {
    avisoEl.textContent = texto;
    avisoEl.classList.toggle("tmp__aviso--ok", ok);
    avisoEl.hidden = false;
    clearTimeout(temporizadorAviso);
    temporizadorAviso = window.setTimeout(() => (avisoEl.hidden = true), ms);
  };

  let temporizadorDestello = 0;
  /** Va hasta el evento: pinta las tandas que falten, centra el evento en la lista de la agenda
   *  (el único contenedor con scroll; `.tmp` y `.contenido` no deben moverse) y lo destella. */
  const irAEvento = (id: string) => {
    const entrada = entradas.find((e) => e.lineas.some((l) => l.ev.id === id));
    if (!entrada) {
      avisar("EL EVENTO YA NO ESTÁ EN LA AGENDA");
      return;
    }
    avisoEl.hidden = true;
    if (entrada.idx >= diasPintados) {
      diasPintados = Math.min(DIAS_AGENDA, Math.ceil((entrada.idx + 1) / TANDA) * TANDA);
      anexar();
    }
    const el = scroll.querySelector<HTMLElement>(`.tmp__evento[data-evid="${CSS.escape(id)}"]`);
    if (!el) {
      avisar("EL EVENTO YA NO ESTÁ EN LA AGENDA");
      return;
    }
    // `scroll` es offsetParent de los eventos: no depende de transformaciones (animación del panel)
    const top = el.offsetTop - (scroll.clientHeight - el.offsetHeight) / 2;
    scroll.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
    clearTimeout(temporizadorDestello);
    scroll.querySelectorAll(".tmp__evento--destello").forEach((x) => x.classList.remove("tmp__evento--destello"));
    void el.offsetWidth;
    el.classList.add("tmp__evento--destello");
    temporizadorDestello = window.setTimeout(() => el.classList.remove("tmp__evento--destello"), 1300);
  };

  // Petición de ir a un evento (desde la miniventana): espera a tener la agenda leída
  let pendiente: string | null = tomarEventoPendiente();
  const procesarPendiente = () => {
    if (!pendiente || !agenda) return;
    const id = pendiente;
    pendiente = null;
    irAEvento(id);
  };

  cuentasEl.addEventListener("click", (e) => {
    const b = (e.target as HTMLElement).closest<HTMLElement>(".tmp__cuenta");
    if (b?.dataset.evid) irAEvento(b.dataset.evid);
  });
  scroll.addEventListener("scroll", actualizarBotonHoy, { passive: true });
  botonHoy.addEventListener("click", () => scroll.scrollTo({ top: 0, behavior: "smooth" }));
  scroll.addEventListener("click", (e) => {
    const b = (e.target as HTMLElement).closest<HTMLElement>(".tmp__estrella");
    const id = b?.dataset.evid;
    const ev = id ? porId.get(id) : undefined;
    if (!b || !id || !ev) return;
    const estabaOn = marcadas.has(id);
    if (estabaOn) marcadas.delete(id);
    else marcadas.add(id);
    actualizarEstrellas();
    b.classList.remove("tmp__estrella--pulso");
    void b.offsetWidth;
    b.classList.add("tmp__estrella--pulso");
    (estabaOn ? desmarcarCuenta(id) : marcarCuenta(ev)).catch(() => {
      marcadas = new Set(cuentas.map((c) => c.id_evento));
      actualizarEstrellas();
    });
  });

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

  // --- Crear evento en Outlook (el calendario real; el ICS lo refleja en unos minutos) ---
  const nuevoBtn = q<HTMLButtonElement>("nuevo-btn");
  const form = q<HTMLFormElement>("nuevo");
  const fTitulo = q<HTMLInputElement>("n-titulo");
  const fFecha = q<HTMLInputElement>("n-fecha");
  const fTodo = q<HTMLInputElement>("n-todo");
  const fIni = q<HTMLInputElement>("n-ini");
  const fFin = q<HTMLInputElement>("n-fin");
  const fUbic = q<HTMLInputElement>("n-ubic");
  const fCats = q("n-cats");
  const fHoras = q("n-horas");
  const fMsg = q("n-msg");
  const fCrear = q<HTMLButtonElement>("n-crear");
  let outlook: EstadoCorreo | null = null;
  let creando = false;

  const motivoOutlook = (): string | null => {
    if (!outlook) return "Comprobando Outlook…";
    if (outlook.estado === "conectado") return null;
    if (outlook.estado === "cerrado") return "Outlook está cerrado: ábrelo para crear eventos";
    if (outlook.estado === "conectando") return "Conectando con Outlook…";
    return `Outlook no responde${outlook.detalle ? `: ${outlook.detalle}` : ""}`;
  };
  const pintarNuevoBtn = () => {
    const m = motivoOutlook();
    nuevoBtn.disabled = m !== null;
    nuevoBtn.title = m ?? "Crear un evento en tu calendario de Outlook";
    if (m !== null && !creando) form.hidden = true;
  };

  const fechaIso = (d: Date) => `${d.getFullYear()}-${dos(d.getMonth() + 1)}-${dos(d.getDate())}`;
  const mensajeForm = (texto: string, error = false) => {
    fMsg.textContent = texto;
    fMsg.classList.toggle("tmp__n-msg--error", error);
  };
  /** Error de validación del formulario, o null si se puede crear. */
  const validarForm = (): string | null => {
    if (!fTitulo.value.trim()) return "Escribe un título";
    if (!fFecha.value) return "Elige una fecha";
    if (!fTodo.checked) {
      if (!fIni.value || !fFin.value) return "Elige la hora de inicio y de fin";
      if (fFin.value <= fIni.value) return "La hora de fin debe ser posterior a la de inicio";
    }
    return null;
  };
  const revisarForm = () => {
    fHoras.hidden = fTodo.checked;
    const err = fTitulo.value.trim() ? validarForm() : null;
    mensajeForm(err ?? "", err !== null);
    fCrear.disabled = creando;
  };

  const pintarCategorias = (lista: string[]) => {
    fCats.replaceChildren(
      ...lista.map((nombre) => {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "tmp__n-cat";
        b.style.setProperty("--cat", colorCategoria(nombre));
        b.setAttribute("aria-pressed", "false");
        b.textContent = nombre;
        b.addEventListener("click", () => {
          const on = b.getAttribute("aria-pressed") !== "true";
          b.setAttribute("aria-pressed", String(on));
        });
        return b;
      }),
    );
    fCats.parentElement!.hidden = lista.length === 0;
  };

  const abrirForm = () => {
    // Por defecto: hoy (o mañana si la próxima hora en punto cae después de medianoche), de la próxima hora a una más
    const ini = new Date();
    ini.setMinutes(0, 0, 0);
    ini.setHours(ini.getHours() + 1);
    fTitulo.value = "";
    fFecha.value = fechaIso(ini);
    fTodo.checked = false;
    fIni.value = hhmm(ini);
    fFin.value = ini.getHours() === 23 ? "23:59" : hhmm(new Date(ini.getTime() + 3600000));
    fUbic.value = "";
    pintarCategorias([]);
    mensajeForm("");
    revisarForm();
    form.hidden = false;
    fTitulo.focus();
    categoriasOutlook()
      .then((l) => vivo && pintarCategorias(l))
      .catch(() => {});
  };
  nuevoBtn.addEventListener("click", () => (form.hidden ? abrirForm() : (form.hidden = true)));
  q("n-cancelar").addEventListener("click", () => {
    if (!creando) form.hidden = true;
  });
  [fTitulo, fFecha, fTodo, fIni, fFin].forEach((c) => c.addEventListener("input", revisarForm));
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (creando) return;
    const err = validarForm();
    if (err) return mensajeForm(err, true);
    creando = true;
    fCrear.disabled = true;
    fCrear.textContent = "CREANDO…";
    mensajeForm("");
    const cats = [...fCats.querySelectorAll<HTMLElement>('.tmp__n-cat[aria-pressed="true"]')].map((b) => b.textContent ?? "");
    try {
      await crearEvento({
        titulo: fTitulo.value.trim(),
        inicio: `${fFecha.value}T${fTodo.checked ? "00:00" : fIni.value}`,
        fin: `${fFecha.value}T${fTodo.checked ? "00:00" : fFin.value}`,
        todo_el_dia: fTodo.checked,
        ubicacion: fUbic.value.trim() || null,
        categorias: cats,
      });
      form.hidden = true;
      avisar("Creado · aparecerá en la agenda en unos minutos", true, 8000);
    } catch (e) {
      mensajeForm(typeof e === "string" ? e : "No se pudo crear el evento", true);
    } finally {
      creando = false;
      fCrear.textContent = "CREAR";
      fCrear.disabled = false;
      if (vivo) pintarNuevoBtn();
    }
  });

  // --- Ciclo de vida ---
  const suscripciones: (() => void)[] = [];
  let intervalo = 0;
  const limpiar = () => {
    if (!vivo) return;
    vivo = false;
    clearInterval(intervalo);
    observador.disconnect();
    suscripciones.forEach((u) => u());
    clearTimeout(temporizadorAviso);
    clearTimeout(temporizadorDestello);
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
    procesarPendiente();
  };
  const recibirCuentas = (c: CuentaAtras[]) => {
    if (!comprobar()) return;
    cuentas = c;
    marcadas = new Set(c.map((x) => x.id_evento));
    pintarCuentas();
    actualizarEstrellas();
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

  pintarAgenda();
  pintarCuentas();
  pintarMeteo();
  intervalo = window.setInterval(() => {
    if (!comprobar()) return;
    // La agenda y los «hace X min» se refrescan al cambiar el minuto
    if (new Date().getSeconds() === 0) {
      pintarAgenda();
      pintarCuentas();
      pintarMeteo();
    }
  }, 1000);

  suscripciones.push(
    alIrAEvento(() => {
      if (!comprobar()) return;
      pendiente = tomarEventoPendiente() ?? pendiente;
      procesarPendiente();
    }),
  );
  suscribir(alEvento<EstadoCalendario>("calendario://estado", (s) => {
    // Un cambio de URL llega con un estado nuevo: se relee la config para saber si hay URL
    void leerConfig().then(() => recibirAgenda(s));
  }));
  suscribir(alEvento<EstadoMeteo>("meteo://estado", recibirMeteo));
  suscribir(alEvento<CuentaAtras[]>("cuentas://estado", recibirCuentas));
  suscribir(
    alEvento<EstadoCorreo>("correo://estado", (e) => {
      if (!comprobar()) return;
      outlook = e;
      pintarNuevoBtn();
    }),
  );
  // La tira se plegó o desplegó en el otro equipo
  suscripciones.push(
    alCambiarTiraRemota((abierta) => {
      if (!comprobar()) return;
      tiraAbierta = abierta;
      pintarTiraAbierta();
    }),
  );
  suscribir(alEvento("panel://mostrar", () => {
    if (!vivo) return;
    // Al volver a abrir el panel, la agenda empieza de nuevo por HOY
    diasPintados = TANDA;
    scroll.scrollTop = 0;
    pintarAgenda();
    pintarCuentas();
    pintarMeteo();
  }));

  void leerConfig().then(() => {
    estadoCalendario().then(recibirAgenda).catch(() => {});
    estadoMeteo().then(recibirMeteo).catch(() => {});
    estadoCuentas().then(recibirCuentas).catch(() => {});
    estadoCorreo()
      .then((e) => {
        if (!vivo) return;
        outlook = e;
        pintarNuevoBtn();
      })
      .catch(() => {});
  });
}
