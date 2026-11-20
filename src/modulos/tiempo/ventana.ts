// Vista ampliada de METEO (ventana desacoplada): ahora, 48 h, 14 días y agenda de la semana.
// Solo lectura: los datos llegan del núcleo (estados y eventos) y aquí nunca se crea ni borra nada.
import "../../styles/tiempo-ventana.css";
import {
  alEvento,
  estadoCalendario,
  estadoMeteo,
  type DiaMeteo,
  type EstadoCalendario,
  type EstadoMeteo,
} from "../../lib/ipc";
import { iconoMeteo } from "../../lib/iconos-meteo";
import { colorCategoria } from "../../lib/categorias";
import { agrupar, DIAS, dos, esc, graficaHoras, hace, hhmm, inicioDeDia, redondear, type Linea } from "./vista";

const PUNTOS = ["N", "NE", "E", "SE", "S", "SO", "O", "NO"];
const DIAS_AGENDA_VENTANA = 7;
/** Por debajo de este ancho las etiquetas de la gráfica van cada 6 h en vez de cada 3 h. */
const ANCHO_ETIQUETA_3H = 620;
const ALTO_GRAFICA = 150;

/** Nivel de UV (escala OMS); en rojo solo de MUY ALTO para arriba. */
function nivelUv(uv: number): { texto: string; alto: boolean } {
  if (uv < 3) return { texto: "BAJO", alto: false };
  if (uv < 6) return { texto: "MODERADO", alto: false };
  if (uv < 8) return { texto: "ALTO", alto: false };
  if (uv < 11) return { texto: "MUY ALTO", alto: true };
  return { texto: "EXTREMO", alto: true };
}

const claveFecha = (d: Date) => `${d.getFullYear()}-${dos(d.getMonth() + 1)}-${dos(d.getDate())}`;
const fechaLocal = (f: string) => {
  const [y, m, d] = f.split("-").map(Number);
  return new Date(y, m - 1, d);
};
const grados = (n: number | null | undefined) => (n === null || n === undefined ? "--" : redondear(n));
const porcentaje = (n: number | null) => (n === null ? "--" : `${Math.round(n)}%`);
/** «AAAA-MM-DDTHH:MM» -> «HH:MM» */
const horaDe = (iso: string | null | undefined) => (iso ? iso.slice(11, 16) : "--:--");

const FLECHA = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 20V4M6 10l6-6 6 6"/></svg>`;

const marco = (titulo: string, etiqueta: string, cuerpo: string, clases: string, id: string) => `
  <section class="marco ${clases}" data-id="${id}">
    <div class="marco__cabecera"><span>${titulo}</span><span class="marco__etiqueta">${etiqueta}</span></div>
    <div class="marco__cuerpo tvm__cuerpo" data-id="${id}-cuerpo">${cuerpo}</div>
  </section>`;

function pintarAhora(m: EstadoMeteo): string {
  const a = m.actual!;
  const hoy = m.dias.find((d) => d.fecha === claveFecha(new Date())) ?? m.dias[0];
  const ciudad = m.ciudad ? `${esc(m.ciudad.nombre)}${m.ciudad.pais ? ` · ${esc(m.ciudad.pais)}` : ""}`.toUpperCase() : "";
  const dir = a.direccion_viento;
  // El viento viene DE `dir`: la flecha apunta hacia donde sopla (dir + 180)
  const viento =
    dir === null || dir === undefined
      ? ""
      : `<span class="tvm__flecha" style="transform:rotate(${Math.round(dir + 180)}deg)">${FLECHA}</span> ${PUNTOS[Math.round(dir / 45) % 8]}`;
  let uv = "--";
  if (a.uv !== null && a.uv !== undefined) {
    const n = nivelUv(a.uv);
    uv = `${Math.round(a.uv * 10) / 10} <span class="${n.alto ? "tvm__uv--alto" : ""}">${n.texto}</span>`;
  }
  return `
    <div class="tvm__ahora-cab">
      <span class="tvm__ahora-icono">${iconoMeteo(a.icono, 56)}</span>
      <span class="tvm__ahora-temp">${redondear(a.temperatura)}</span>
      <span class="tvm__ahora-texto">${esc(a.texto).toUpperCase()}</span>
    </div>
    <dl class="tvm__datos">
      <div><dt>SENSACIÓN</dt><dd>${redondear(a.sensacion)}</dd></div>
      <div><dt>HUMEDAD</dt><dd>${Math.round(a.humedad)}%</dd></div>
      <div><dt>VIENTO</dt><dd>${Math.round(a.viento)} km/h ${viento}</dd></div>
      <div><dt>UV</dt><dd>${uv}</dd></div>
      <div><dt>SALIDA</dt><dd>${horaDe(hoy?.salida_sol)}</dd></div>
      <div><dt>PUESTA</dt><dd>${horaDe(hoy?.puesta_sol)}</dd></div>
    </dl>
    <div class="tvm__ciudad">${ciudad}</div>`;
}

function pintarDias(dias: DiaMeteo[]): string {
  const max = dias.flatMap((d) => (d.maxima === null ? [] : [d.maxima]));
  const min = dias.flatMap((d) => (d.minima === null ? [] : [d.minima]));
  const gMax = Math.max(...max);
  const gMin = Math.min(...min);
  const span = Math.max(1, gMax - gMin);
  const hoy = claveFecha(new Date());
  return dias
    .slice(0, 14)
    .map((d) => {
      const f = fechaLocal(d.fecha);
      const rango =
        d.maxima === null || d.minima === null
          ? `<span class="tvm__rango"></span>`
          : `<span class="tvm__rango"><i style="left:${(((d.minima - gMin) / span) * 100).toFixed(1)}%;width:${(((d.maxima - d.minima) / span) * 100).toFixed(1)}%"></i></span>`;
      return `<div class="tvm__dia${d.fecha === hoy ? " tvm__dia--hoy" : ""}" title="${esc(d.texto)}">
        <span class="tvm__dia-nombre">${d.fecha === hoy ? "HOY" : DIAS[f.getDay()].slice(0, 3)}</span>
        <span class="tvm__dia-fecha">${dos(f.getDate())}/${dos(f.getMonth() + 1)}</span>
        ${iconoMeteo(d.icono, 24)}
        <span class="tvm__dia-temp"><b>${grados(d.maxima)}</b> <i>${grados(d.minima)}</i></span>
        ${rango}
        <span class="tvm__dia-lluvia">${porcentaje(d.prob_lluvia)}</span>
      </div>`;
    })
    .join("");
}

function pintarAgenda(m: EstadoMeteo | null, cal: EstadoCalendario | null): string {
  const hoy = inicioDeDia(new Date());
  const grupos = agrupar(cal?.eventos ?? [], new Date());
  let hayEventos = false;
  const html = Array.from({ length: DIAS_AGENDA_VENTANA }, (_, i) => {
    const dia = new Date(hoy.getFullYear(), hoy.getMonth(), hoy.getDate() + i);
    const lineas: Linea[] = [...(grupos.get(dia.getTime()) ?? [])].sort(
      (a, b) => Number(b.ev.todo_el_dia) - Number(a.ev.todo_el_dia) || a.inicio.getTime() - b.inicio.getTime(),
    );
    if (lineas.length) hayEventos = true;
    const prev = m?.dias.find((d) => d.fecha === claveFecha(dia));
    const tiempo = prev
      ? `<span class="tvm__ag-tiempo">${iconoMeteo(prev.icono, 16)} ${grados(prev.maxima)} / ${grados(prev.minima)}</span>`
      : "";
    const nombre = i === 0 ? "HOY" : i === 1 ? "MAÑANA" : `${DIAS[dia.getDay()].slice(0, 3)} ${dia.getDate()}`;
    const evs = lineas
      .map((l) => {
        const cat = l.ev.categorias[0];
        return `<li class="tvm__ev">
          <span class="tvm__ev-hora">${l.ev.todo_el_dia ? "TODO EL DÍA" : hhmm(l.inicio)}</span>
          <i class="tvm__cuadrado" style="--cat:${cat ? colorCategoria(cat) : "var(--texto-tenue)"}"></i>
          <span class="tvm__ev-txt"><span class="tvm__ev-titulo">${esc(l.ev.titulo)}</span>${l.ev.ubicacion ? `<span class="tvm__ev-ubic">${esc(l.ev.ubicacion)}</span>` : ""}</span>
        </li>`;
      })
      .join("");
    return `<div class="tvm__ag-dia"><div class="tvm__ag-cab"><span>${nombre}</span>${tiempo}</div><ul class="tvm__evs">${evs}</ul></div>`;
  }).join("");
  return hayEventos ? html : `<div class="tvm__vacio">SIN EVENTOS</div>`;
}

export function montarVentanaMeteo(cont: HTMLElement): () => void {
  let vivo = true;
  let meteo: EstadoMeteo | null = null;
  let agenda: EstadoCalendario | null = null;
  let frame: number | null = null;
  const desuscribir: Array<() => void> = [];

  const aviso = (texto: string) => {
    cont.innerHTML = `<section class="marco tvm__marco-aviso"><div class="marco__cuerpo tvm__vacio">${texto}</div></section>`;
  };

  const grafica = () => {
    const el = cont.querySelector<HTMLElement>('[data-id="h48-cuerpo"]');
    if (!el) return;
    const ancho = Math.max(300, Math.round(el.clientWidth));
    el.innerHTML = graficaHoras((meteo?.por_horas ?? []).slice(0, 48), {
      ancho,
      alto: ALTO_GRAFICA,
      cada: ancho >= ANCHO_ETIQUETA_3H ? 3 : 6,
      medianoche: true,
    });
  };

  const pintar = () => {
    if (!vivo) return;
    const m = meteo;
    if (!m) return aviso("CARGANDO…");
    if (!m.ciudad) return aviso("CONFIGURA UNA CIUDAD EN AJUSTES");
    if (!m.actual) return aviso(m.error ? esc(m.error) : "CARGANDO…");
    const scroll = cont.scrollTop;
    const act = m.actualizado ? `ACTUALIZADO ${hace(m.actualizado)}` : "SIN SINCRONIZAR";
    cont.innerHTML = `
      <div class="tvm">
        ${marco("AHORA", m.cargando ? "ACTUALIZANDO…" : "", pintarAhora(m), "tvm__ahora", "ahora")}
        ${marco("AGENDA · 7 DÍAS", "", pintarAgenda(m, agenda), "tvm__agenda", "agenda")}
        ${marco("48 H", "TEMPERATURA · LLUVIA", "", "marco--cian tvm__h48", "h48")}
        ${marco("14 DÍAS", "", `<div class="tvm__dias">${pintarDias(m.dias)}</div>`, "marco--cian tvm__14", "dias")}
        <div class="tvm__pie">${esc(act)}${m.error ? ` · <span class="tvm__uv--alto">${esc(m.error)}</span>` : ""}</div>
      </div>`;
    grafica();
    cont.scrollTop = scroll;
  };

  // Redibuja la gráfica al cambiar el ancho, como mucho una vez por frame
  const obs = new ResizeObserver(() => {
    if (frame !== null) return;
    frame = requestAnimationFrame(() => {
      frame = null;
      if (vivo) grafica();
    });
  });
  obs.observe(cont);

  pintar();
  estadoMeteo()
    .then((e) => {
      meteo = e;
      pintar();
    })
    .catch(() => {});
  estadoCalendario()
    .then((e) => {
      agenda = e;
      pintar();
    })
    .catch(() => {});
  // Si la limpieza llega antes de que se resuelva la suscripción, se quita en cuanto llegue
  const suscribir = (p: Promise<() => void>) => void p.then((u) => (vivo ? desuscribir.push(u) : u())).catch(() => {});
  suscribir(
    alEvento<EstadoMeteo>("meteo://estado", (e) => {
      meteo = e;
      pintar();
    }),
  );
  suscribir(
    alEvento<EstadoCalendario>("calendario://estado", (e) => {
      agenda = e;
      pintar();
    }),
  );
  // «Hace X min» y el cambio de día se refrescan solos
  const reloj = setInterval(pintar, 60_000);

  return () => {
    vivo = false;
    clearInterval(reloj);
    obs.disconnect();
    if (frame !== null) cancelAnimationFrame(frame);
    desuscribir.forEach((u) => u());
  };
}
