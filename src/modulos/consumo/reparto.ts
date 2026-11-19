// Apartados PROYECTOS, EQUIPOS y USO de la ventana de CONSUMO: repartos del % semanal en barras de
// DOM (sin canvas). Solo se repintan al llegar datos nuevos; EQUIPOS refresca el «HACE N MIN» cada
// 30 s mientras la ventana está activa (un único temporizador, que muere con el apartado).
import type { DetalleConsumo, GrupoProyectoConsumo, ParteConsumo } from "../../lib/ipc";
import { colorProyecto, temaGrafica } from "./colores";
import type { Apartado, Cambio, Entorno } from "./apartados";

const OTROS = "(otros)";
const RESTO = "(resto)";
const MAX_BARRAS = 5; // como Consumo: con más, salen 4 y la quinta es «+N MÁS»
const MIN_PCT = 5; // % mínimo (sobre lo consumido) para tener fila propia
const MARGEN_SEMANA = 600; // s
const REFRESCO_HACE_MS = 30_000;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, clase?: string, texto?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (clase) e.className = clase;
  if (texto !== undefined) e.textContent = texto;
  return e;
}

const fmtPct = (v: number) => (v >= 1 || v === 0 ? `${Math.round(v)}%` : `${v.toFixed(1)}%`);
const nombreVisible = (n: string) => (n === OTROS ? "SIN ATRIBUIR" : n === RESTO ? "RESTO" : n === "" ? "SIN CLASIFICAR" : n);
const esGris = (n: string) => n === OTROS || n === RESTO || n === "";

/** Texto largo recortado con «…» por CSS (el nombre completo va en el title). */
function etiqueta(texto: string, clase: string): HTMLElement {
  const e = el("span", clase, texto.toUpperCase());
  e.title = texto;
  return e;
}

/** El detalle es de la semana en curso (como `semana_vigente` de Consumo). */
function vigente(e: Entorno): boolean {
  const d = e.datos.detalle;
  const r = e.datos.estado?.semanal.resetsAt;
  return !!d && typeof r === "number" && Math.abs(d.reset - r) <= MARGEN_SEMANA;
}

/** Fila con nombre, % y barra; `tramos` son (peso, color). */
function fila(
  nombre: string,
  titulo: string,
  pct: string,
  ancho: number,
  tramos: [number, string][],
  gris = false,
  extra?: HTMLElement | string,
): HTMLElement {
  const f = el("div", "rp__fila");
  const cab = el("div", "rp__cab");
  const n = etiqueta(nombre, gris ? "rp__nombre rp__nombre--tenue" : "rp__nombre");
  n.title = titulo;
  cab.append(n, el("b", "rp__pct", pct));
  const barra = el("div", "rp__barra");
  barra.style.width = `${Math.max(1.5, Math.min(100, ancho))}%`;
  const total = tramos.reduce((s, t) => s + t[0], 0) || 1;
  for (const [peso, color] of tramos) {
    const seg = el("i", "rp__seg");
    seg.style.flex = `${Math.max(peso, total * 0.01)} 1 0`;
    seg.style.background = color;
    barra.appendChild(seg);
  }
  f.append(cab, barra);
  if (extra) f.appendChild(typeof extra === "string" ? el("span", "rp__sub", extra) : extra);
  return f;
}

const sobreTotal = (v: number, total: number) => (total > 0 ? (v * 100) / total : 0);

/** Filas visibles y «+N MÁS»: las que pesan ≥ MIN_PCT, como mucho MAX_BARRAS. */
function partir<T extends { nombre: string; pct: number }>(items: T[], total: number): { salen: T[]; fuera: T[]; sumaFuera: number } {
  const otros = items.filter((i) => i.nombre === OTROS);
  const reales = items.filter((i) => i.nombre !== OTROS);
  let salen = reales.filter((i) => i.nombre !== RESTO && sobreTotal(i.pct, total) >= MIN_PCT);
  const capacidad = MAX_BARRAS - otros.length;
  const sobran = reales.filter((i) => !salen.includes(i)).length;
  if (salen.length + (sobran > 0 ? 1 : 0) > capacidad) salen = salen.slice(0, Math.max(1, capacidad - 1));
  const fuera = reales.filter((i) => !salen.includes(i));
  return { salen: [...salen, ...otros], fuera, sumaFuera: fuera.reduce((s, i) => s + i.pct, 0) };
}

const marco = (titulo: string, etiq: string, ayuda: string, clase = "") => {
  const m = el("section", `marco ${clase}`.trim());
  const cab = el("div", "marco__cabecera");
  cab.append(el("span", undefined, titulo), el("span", "marco__etiqueta", etiq));
  const cuerpo = el("div", "marco__cuerpo");
  cuerpo.appendChild(el("p", "rp__ayuda", ayuda));
  m.append(cab, cuerpo);
  return { m, cuerpo };
};

const vacio = (cuerpo: HTMLElement, texto: string) => cuerpo.appendChild(el("p", "marco__vacio rp__vacio", texto));

function montarCol(raiz: HTMLElement) {
  const col = el("div", "cv__col");
  raiz.replaceChildren(col);
  return col;
}

// --- PROYECTOS ---
export function montarProyectos(raiz: HTMLElement, e: Entorno): Apartado {
  const col = montarCol(raiz);
  const act = marco("PROYECTO Y SESIÓN", "SEMANA ACTUAL", "UNA BARRA POR PROYECTO, PARTIDA EN SUS SESIONES; LA MÁS LARGA ES LA QUE MÁS GASTA", "marco--cian");
  const prev = marco("SEMANA ANTERIOR", "PROYECTOS", "CÓMO SE REPARTIÓ LA SEMANA PASADA");
  col.append(act.m, prev.m);
  const zona = el("div", "rp__lista");
  const zonaPrev = el("div", "rp__lista rp__lista--tenue");
  act.cuerpo.appendChild(zona);
  prev.cuerpo.appendChild(zonaPrev);

  const sesionesVisibles = (g: GrupoProyectoConsumo) => g.sesiones.filter((s) => s.pct > 0);

  const pintar = () => {
    const t = temaGrafica();
    zona.replaceChildren();
    zonaPrev.replaceChildren();
    const d = e.datos.detalle;
    if (!d || !vigente(e)) {
      vacio(zona, "CALCULANDO…");
    } else if (!d.proyectos.length) {
      vacio(zona, "SIN DATOS DE REPARTO TODAVÍA");
    } else {
      const total = d.proyectos.reduce((s, p) => s + p.pct, 0);
      const mx = Math.max(...d.proyectos.map((p) => p.pct), 0.0001);
      const { salen, fuera, sumaFuera } = partir(d.proyectos, total);
      for (const p of salen) {
        const gris = esGris(p.nombre);
        const color = colorProyecto(p.nombre, t);
        const ses = sesionesVisibles(p);
        const mezclas = [0, 32, 16, 45];
        const tramos: [number, string][] = (ses.length ? ses : [{ nombre: "", pct: p.pct }]).map((s, j) => [
          s.pct,
          gris ? color : `color-mix(in srgb, ${color}, var(--fondo) ${mezclas[j % 4]}%)`,
        ]);
        const sub = ses.length > 1 ? `SESIONES  ${ses.slice(0, 6).map((s) => fmtPct(sobreTotal(s.pct, total))).join(" · ")}` : undefined;
        const titulo = `${nombreVisible(p.nombre)}\n${ses.map((s) => `${s.nombre} · ${fmtPct(sobreTotal(s.pct, total))}`).join("\n")}`.trim();
        zona.appendChild(fila(nombreVisible(p.nombre), titulo, fmtPct(sobreTotal(p.pct, total)), (p.pct / mx) * 100, tramos, gris, sub));
      }
      if (fuera.length) {
        const f = el("div", "rp__mas");
        const n = el("span", undefined, `+${fuera.length} MÁS`);
        n.title = fuera.map((p) => nombreVisible(p.nombre)).join(", ");
        f.append(n, el("b", undefined, fmtPct(sobreTotal(sumaFuera, total))));
        zona.appendChild(f);
      }
    }
    const p = d?.previas;
    prev.m.hidden = !p || !p.proyectos.length;
    if (p && p.proyectos.length) {
      const total = p.proyectos.reduce((s, x) => s + x.pct, 0);
      for (const g of p.proyectos.slice(0, 5)) {
        const f = el("div", "rp__previa");
        const c = el("i", "rp__punto");
        c.style.background = colorProyecto(g.nombre, t);
        f.append(c, etiqueta(nombreVisible(g.nombre), "rp__nombre rp__nombre--tenue"), el("b", "rp__pct", fmtPct(sobreTotal(g.pct, total))));
        zonaPrev.appendChild(f);
      }
    }
  };

  return {
    pintar(c: Cambio) {
      if (c === "detalle" || c === "estado" || c === "todo") pintar();
    },
    destruir: () => raiz.replaceChildren(),
  };
}

// --- EQUIPOS ---
const haceTexto = (s: number) => {
  const t = Math.max(0, Math.floor(s));
  return t < 120 ? "AHORA" : t < 3600 ? `HACE ${Math.floor(t / 60)} MIN` : t < 86400 ? `HACE ${Math.floor(t / 3600)} H` : `HACE ${Math.floor(t / 86400)} D`;
};

export function montarEquipos(raiz: HTMLElement, e: Entorno): Apartado {
  const col = montarCol(raiz);
  const m = marco("CONSUMO POR EQUIPO", "SEMANA ACTUAL", "QUIÉN HA GASTADO QUÉ; EN AMARILLO, EL EQUIPO PARADO MÁS DE DOS DÍAS", "marco--cian");
  col.appendChild(m.m);
  const lista = el("div", "rp__lista");
  const nota = el("p", "rp__nota", "SOLO ESTE EQUIPO · CONFIGURA LA CARPETA DE SINCRONIZACIÓN EN AJUSTES → CONSUMO");
  nota.hidden = true;
  m.cuerpo.append(lista, nota);
  let horas: { el: HTMLElement; ultima: number }[] = [];
  let temporizador: number | undefined;

  const pintarHace = () => {
    const ahora = Date.now() / 1000;
    for (const h of horas) {
      const t = haceTexto(ahora - h.ultima);
      if (h.el.textContent !== t) h.el.textContent = t;
      h.el.classList.toggle("rp__hace--parado", ahora - h.ultima > 2 * 86400);
    }
  };
  const ajustarTemporizador = () => {
    const debe = e.activo() && horas.length > 0;
    if (debe && temporizador === undefined) temporizador = window.setInterval(pintarHace, REFRESCO_HACE_MS);
    else if (!debe && temporizador !== undefined) {
      clearInterval(temporizador);
      temporizador = undefined;
    }
  };

  const pintar = () => {
    lista.replaceChildren();
    horas = [];
    nota.hidden = true;
    const d = e.datos.detalle;
    if (!d || !vigente(e)) {
      vacio(lista, "CALCULANDO…");
      ajustarTemporizador();
      return;
    }
    if (!d.equipos.length) {
      vacio(lista, "SIN DATOS DE REPARTO TODAVÍA");
      ajustarTemporizador();
      return;
    }
    const t = temaGrafica();
    const total = d.equipos.reduce((s, x) => s + x.pct, 0);
    const mx = Math.max(...d.equipos.map((x) => x.pct), 0.0001);
    d.equipos.forEach((q, i) => {
      const otros = q.nombre === OTROS;
      const color = otros ? t.otros : i % 2 === 0 ? t.cian : t.amarillo;
      let hace: HTMLElement | undefined;
      if (!otros && q.ultima) {
        hace = el("span", "rp__hace");
        horas.push({ el: hace, ultima: q.ultima });
      }
      lista.appendChild(fila(nombreVisible(q.nombre), q.nombre, fmtPct(sobreTotal(q.pct, total)), (q.pct / mx) * 100, [[1, color]], otros, hace));
    });
    pintarHace();
    nota.hidden = d.equipos.filter((q) => q.nombre !== OTROS).length > 1;
    ajustarTemporizador();
  };

  return {
    pintar(c: Cambio) {
      if (c === "detalle" || c === "estado" || c === "todo") pintar();
      else ajustarTemporizador();
    },
    destruir() {
      if (temporizador !== undefined) clearInterval(temporizador);
      temporizador = undefined;
      raiz.replaceChildren();
    },
  };
}

// --- USO (modelo y tarea) ---
export function montarUso(raiz: HTMLElement, e: Entorno): Apartado {
  const col = montarCol(raiz);
  const mod = marco("CONSUMO POR MODELO", "SEMANA ACTUAL", "UNA BARRA APILADA: LO ANCHO DE CADA TRAMO ES SU PARTE DEL CONSUMO", "marco--cian");
  const tar = marco("CONSUMO POR TAREA", "SEMANA ACTUAL", "CADA LLAMADA CUENTA PARA LO QUE HIZO EN ELLA; PESA SOBRE TODO POR EL CONTEXTO QUE ARRASTRA");
  col.append(mod.m, tar.m);
  const zonaMod = el("div");
  const zonaTar = el("div");
  mod.cuerpo.appendChild(zonaMod);
  tar.cuerpo.appendChild(zonaTar);

  /** Barra apilada con leyenda; los colores salen de los tokens (acentos, mezclas y tenue). */
  const reparto = (zona: HTMLElement, partes: ParteConsumo[], aviso?: string) => {
    zona.replaceChildren();
    if (!partes.length) {
      vacio(zona, "SIN DATOS DE REPARTO TODAVÍA");
      return;
    }
    const t = temaGrafica();
    const total = partes.reduce((s, p) => s + p.pct, 0);
    const base = [t.cian, t.amarillo, ...t.proyectos.slice(2)];
    const barra = el("div", "rp__apilada");
    const ley = el("div", "rp__ley");
    partes.forEach((p, i) => {
      const c = esGris(p.nombre) ? t.otros : base[i % base.length];
      const seg = el("i", "rp__seg rp__seg--apilado");
      seg.style.flex = `${Math.max(p.pct, total * 0.005)} 1 0`;
      seg.style.background = c;
      seg.title = `${nombreVisible(p.nombre)} · ${fmtPct(sobreTotal(p.pct, total))}`;
      barra.appendChild(seg);
      const f = el("div", "rp__ley-fila");
      const punto = el("i", "rp__punto");
      punto.style.background = c;
      f.append(punto, etiqueta(nombreVisible(p.nombre), "rp__nombre"), el("b", "rp__pct", fmtPct(sobreTotal(p.pct, total))));
      ley.appendChild(f);
    });
    zona.append(barra, ley);
    if (partes.filter((p) => !esGris(p.nombre)).length === 1) zona.appendChild(el("p", "rp__sub", "ESTA SEMANA SOLO HAY UNA PARTE"));
    if (aviso) zona.appendChild(el("p", "rp__nota", aviso));
  };

  const pintar = () => {
    const d: DetalleConsumo | null = e.datos.detalle;
    if (!d || !vigente(e)) {
      for (const z of [zonaMod, zonaTar]) {
        z.replaceChildren();
        vacio(z, "CALCULANDO…");
      }
      return;
    }
    reparto(zonaMod, d.modelos);
    const hayVacia = d.tareas.some((p) => p.nombre === "" || p.nombre === "sin clasificar");
    const aviso = d.sinTarea.length && hayVacia ? `LOS EQUIPOS ${d.sinTarea.join(", ").toUpperCase()} NO GUARDAN LA TAREA` : undefined;
    reparto(zonaTar, d.tareas, aviso);
  };

  return {
    pintar(c: Cambio) {
      if (c === "detalle" || c === "estado" || c === "todo") pintar();
    },
    destruir: () => raiz.replaceChildren(),
  };
}
