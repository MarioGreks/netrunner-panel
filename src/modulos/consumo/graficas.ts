// Gráficas de la ventana de CONSUMO (canvas 2D, estilo HUD): ritmo de la semana, consumo por día y
// consumo acumulado por hora. Portan pintar_ritmo, pintar_dias y pintar_stack de la app Consumo.
//
// REGLA DE RECURSOS: ningún bucle ni animación. `crearLienzo` solo repinta bajo petición
// (llegan datos, cambia el tema o cambia el tamaño) y funde las peticiones en un único
// requestAnimationFrame; con `activo() === false` no pinta nada.
import { EVENTO_TEMA } from "../../lib/tema";
import type { DiasConsumo, EstadoConsumo, NivelConsumo, PorHoraConsumo, SerieVentanaConsumo } from "../../lib/ipc";
import { colorProyecto, conAlfa, temaGrafica, type TemaGrafica } from "./colores";

const SEMANA_S = 604_800;
const DIA_S = 86_400;
const LETRAS = "LMXJVSD";

type Dibujo = (c: CanvasRenderingContext2D, ancho: number, alto: number, t: TemaGrafica) => void;

export interface Lienzo {
  /** Pide un repintado (se funde con los demás en un fotograma). */
  redibujar(): void;
  destruir(): void;
}

/**
 * Enlaza un canvas con su dibujo. Repinta al redimensionarse (ResizeObserver, a un fotograma como
 * mucho), al cambiar el tema y cuando se llama a `redibujar`. Si `activo()` es falso no pinta.
 */
export function crearLienzo(canvas: HTMLCanvasElement, dibujar: Dibujo, activo: () => boolean): Lienzo {
  const ctx = canvas.getContext("2d")!;
  let raf = 0;
  let vivo = true;

  const pintar = () => {
    raf = 0;
    if (!vivo || !activo()) return;
    const dpr = window.devicePixelRatio || 1;
    const r = canvas.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return;
    const w = Math.round(r.width * dpr);
    const h = Math.round(r.height * dpr);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, r.width, r.height);
    dibujar(ctx, r.width, r.height, temaGrafica());
  };
  const pedir = () => {
    if (!raf && vivo && activo()) raf = requestAnimationFrame(pintar);
  };

  const observador = new ResizeObserver(pedir);
  observador.observe(canvas);
  window.addEventListener(EVENTO_TEMA, pedir);
  pedir();

  return {
    redibujar: pedir,
    destruir() {
      vivo = false;
      cancelAnimationFrame(raf);
      raf = 0;
      observador.disconnect();
      window.removeEventListener(EVENTO_TEMA, pedir);
    },
  };
}

// --- Utilidades ---
export const fuente = (px: number, negrita = false) => `${negrita ? "700 " : ""}${px}px "JetBrains Mono", monospace`;

const fechaLocal = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
const letraDe = (d: Date) => LETRAS[(d.getDay() + 6) % 7];

function colorNivel(n: NivelConsumo, t: TemaGrafica): string {
  return n === "riesgo" ? t.rojo : n === "encima" ? t.amarillo : t.cian;
}

function sinDatos(c: CanvasRenderingContext2D, t: TemaGrafica, texto = "SIN DATOS TODAVÍA") {
  c.fillStyle = t.tenue;
  c.font = fuente(11);
  c.textAlign = "left";
  c.textBaseline = "middle";
  c.fillText(texto, 8, 20);
}

export function lineaH(c: CanvasRenderingContext2D, x0: number, x1: number, y: number, color: string) {
  c.strokeStyle = color;
  c.lineWidth = 1;
  c.beginPath();
  c.moveTo(x0, Math.round(y) + 0.5);
  c.lineTo(x1, Math.round(y) + 0.5);
  c.stroke();
}

export function lineaV(c: CanvasRenderingContext2D, x: number, y0: number, y1: number, color: string) {
  c.strokeStyle = color;
  c.lineWidth = 1;
  c.beginPath();
  c.moveTo(Math.round(x) + 0.5, y0);
  c.lineTo(Math.round(x) + 0.5, y1);
  c.stroke();
}

type Punto = [number, number];

/**
 * Parte la curva real [(días, %)] en tramos según esté por encima (+1) o por debajo (-1) de lo
 * esperado (la diagonal % = días / 7 · 100); donde la cruza, un tramo acaba y otro empieza.
 */
export function cintasDiagonal(pts: Punto[], semana = 7): [number, Punto[]][] {
  const esp = (d: number) => (d / semana) * 100;
  const cintas: [number, Punto[]][] = [];
  let cur: Punto[] = [];
  let signo = 0;
  let prev: [number, number, number] | null = null;
  for (const [d, p] of pts) {
    const f = p - esp(d);
    const s = (f > 1e-9 ? 1 : 0) - (f < -1e-9 ? 1 : 0);
    if (prev && s && signo && s !== signo) {
      const [d0, p0, f0] = prev;
      const k = f0 / (f0 - f);
      const corte: Punto = [d0 + k * (d - d0), p0 + k * (p - p0)];
      cintas.push([signo, [...cur, corte]]);
      cur = [corte];
    }
    cur.push([d, p]);
    signo = s || signo;
    prev = [d, p, f];
  }
  if (cur.length > 1) cintas.push([signo || 1, cur]);
  return cintas;
}

// --- Ritmo de la semana ---
export interface DatosRitmo {
  estado: EstadoConsumo | null;
  serie: SerieVentanaConsumo | null;
}

/** % al acabar la semana al ritmo de ahora (null muy al principio). */
export function proyeccionSemana(estado: EstadoConsumo): number | null {
  const s = estado.semanal;
  return s.exceso > 0 ? 100 + s.exceso : s.proyeccionSemana;
}

export const dibujarRitmo = (d: DatosRitmo): Dibujo => (c, w, h, t) => {
  const sem = d.estado?.semanal;
  if (!sem || !sem.hayDatos || !d.serie) return sinDatos(c, t);
  const px0 = 34;
  const pw = w - px0 - 12;
  const py0 = 10;
  const ph = h - py0 - 24;
  const X = (dias: number) => px0 + (dias / 7) * pw;
  const Y = (p: number) => py0 + ph - (Math.min(p, 100) / 100) * ph;
  c.font = fuente(9);
  c.textAlign = "right";
  c.textBaseline = "middle";
  for (const v of [0, 50, 100]) {
    lineaH(c, px0, px0 + pw, Y(v), v === 0 ? t.rejillaFuerte : t.rejilla);
    c.fillStyle = t.tenue;
    c.fillText(`${v}%`, px0 - 5, Y(v));
  }
  const ahora = Date.now() / 1000;
  const hoy = Math.min(7, Math.max(0, (ahora - d.serie.inicio) / DIA_S));
  // Letra de cada tramo (la del día de su mitad); resalta el día de calendario de hoy
  const hoyFecha = fechaLocal(new Date(ahora * 1000));
  const fechas = Array.from({ length: 7 }, (_, i) => new Date((d.serie!.inicio + i * DIA_S + DIA_S / 2) * 1000));
  const hoyI = Math.max(0, fechas.findIndex((f) => fechaLocal(f) === hoyFecha)) || Math.min(6, Math.floor(hoy));
  c.textAlign = "center";
  fechas.forEach((f, i) => {
    c.fillStyle = i === hoyI ? t.texto : t.tenue;
    c.font = fuente(9, i === hoyI);
    c.fillText(letraDe(f), X(i + 0.5), py0 + ph + 13);
    if (i > 0) lineaV(c, X(i), py0, py0 + ph, t.rejilla);
  });

  // Lo esperado por el tiempo: la diagonal
  c.save();
  c.setLineDash([4, 4]);
  c.strokeStyle = t.tenue;
  c.lineWidth = 1;
  c.beginPath();
  c.moveTo(X(0), Y(0));
  c.lineTo(X(7), Y(100));
  c.stroke();
  c.restore();

  const pct = sem.pct;
  const pts: Punto[] = [[0, 0]];
  for (const [ts, p] of d.serie.muestras) {
    const dd = (ts - d.serie.inicio) / DIA_S;
    if (dd > 0 && dd < hoy) pts.push([dd, p]);
  }
  pts.push([hoy, pct]);
  // Áreas entre lo real y lo esperado: cian por debajo (vas bien), amarillo por encima
  for (const [signo, tramo] of cintasDiagonal(pts)) {
    const base = signo > 0 ? t.amarillo : t.cian;
    const esp = tramo.map(([dd]): Punto => [dd, (dd / 7) * 100]).reverse();
    c.beginPath();
    [...tramo, ...esp].forEach(([dd, p], i) => (i ? c.lineTo(X(dd), Y(p)) : c.moveTo(X(dd), Y(p))));
    c.closePath();
    const g = c.createLinearGradient(0, py0, 0, py0 + ph);
    g.addColorStop(0, conAlfa(base, 0.5));
    g.addColorStop(1, conAlfa(base, 0.12));
    c.fillStyle = g;
    c.fill();
  }
  const col = colorNivel(sem.nivel, t);
  c.beginPath();
  pts.forEach(([dd, p], i) => (i ? c.lineTo(X(dd), Y(p)) : c.moveTo(X(dd), Y(p))));
  c.strokeStyle = col;
  c.lineWidth = 2;
  c.lineJoin = "round";
  c.shadowColor = col;
  c.shadowBlur = 5;
  c.stroke();
  c.shadowBlur = 0;

  // Proyección hasta el reset: si pasa del 100 %, la línea acaba donde lo toca
  const proy = d.estado ? proyeccionSemana(d.estado) : null;
  if (proy !== null) {
    const fin = proy <= 100 || proy <= pct ? 7 : hoy + ((100 - pct) / (proy - pct)) * (7 - hoy);
    c.save();
    c.setLineDash([3, 3]);
    c.strokeStyle = proy > 100 ? t.rojo : col;
    c.lineWidth = 1.5;
    c.beginPath();
    c.moveTo(X(hoy), Y(pct));
    c.lineTo(X(fin), Y(Math.min(proy, 100)));
    c.stroke();
    c.restore();
  }
  c.fillStyle = col;
  c.beginPath();
  c.arc(X(hoy), Y(pct), 3.5, 0, Math.PI * 2);
  c.fill();
};

// --- Consumo por día ---
export interface DatosDias {
  dias: DiasConsumo | null;
}

const ESCALAS_DIAS = [5, 10, 20, 30, 50, 100];

export const dibujarDias = (d: DatosDias): Dibujo => (c, w, h, t) => {
  const dias = d.dias;
  if (!dias || dias.fechas.length === 0) return sinDatos(c, t);
  const n = dias.fechas.length;
  const ym = ESCALAS_DIAS.find((v) => v >= Math.max(...dias.total, 0)) ?? 100;
  const px0 = 32;
  const pw = w - px0 - 12;
  const py0 = 18;
  const ph = h - py0 - 30;
  const Y = (v: number) => py0 + ph - (v / ym) * ph;
  c.font = fuente(9);
  c.textAlign = "right";
  c.textBaseline = "middle";
  for (const v of [0, ym / 2, ym]) {
    lineaH(c, px0, px0 + pw, Y(v), v === 0 ? t.rejillaFuerte : t.rejilla);
  }
  c.fillStyle = t.tenue;
  c.fillText("0", px0 - 5, Y(0));
  c.fillText(`${ym}%`, px0 - 5, Y(ym));
  const hoyI = dias.fechas.indexOf(fechaLocal(new Date()));
  const bw = pw / n;
  const ancho = Math.max(4, bw - 8);
  dias.fechas.forEach((f, i) => {
    const [a, m, dd] = f.split("-").map(Number);
    const xa = px0 + i * bw + (bw - ancho) / 2;
    let base = 0;
    for (const s of dias.series) {
      const v = s.valores[i] ?? 0;
      if (v <= 0) continue;
      c.fillStyle = conAlfa(colorProyecto(s.nombre, t), 0.85);
      c.fillRect(xa, Y(base + v), ancho, Y(base) - Y(base + v));
      base += v;
    }
    const total = dias.total[i] ?? base;
    c.textAlign = "center";
    if (total >= 0.5) {
      c.fillStyle = i === hoyI ? t.texto : t.tenue;
      c.font = fuente(9, i === hoyI);
      c.fillText(String(Math.round(total)), xa + ancho / 2, Y(base) - 8);
    }
    c.fillStyle = i === hoyI ? t.texto : t.tenue;
    c.font = fuente(9, i === hoyI);
    c.fillText(letraDe(new Date(a, m - 1, dd)), xa + ancho / 2, py0 + ph + 13);
    if (i === hoyI) {
      c.fillStyle = t.cian;
      c.fillRect(xa, py0 + ph + 22, ancho, 2);
    }
  });
};

// --- Consumo acumulado por hora (stack) ---
export interface DatosStack {
  estado: EstadoConsumo | null;
  horas: PorHoraConsumo | null;
}

export const dibujarStack = (d: DatosStack): Dibujo => (c, w, h, t) => {
  const horas = d.horas;
  if (!horas || !horas.series.some((s) => s.valores.some((v) => v > 0))) return sinDatos(c, t);
  const px0 = 34;
  const pw = w - px0 - 40;
  const py0 = 10;
  const ph = h - py0 - 24;
  const ahora = Date.now() / 1000;
  const sem = d.estado?.semanal;
  const ini = horas.inicio;
  // El eje va del inicio de la semana (izquierda) a ahora (derecha): la curva llena todo el ancho
  let t0 = sem?.resetsAt ? sem.resetsAt - SEMANA_S : ini;
  t0 = Math.min(t0, ahora - 3600);
  const n = Math.min(168, Math.floor((ahora - ini) / 3600) + 1);
  const i0 = Math.max(0, Math.floor((t0 - ini) / 3600));
  // % acumulado de cada grupo al final de cada hora (solo desde el inicio de la semana)
  const cum = horas.series.map((s) => {
    let acc = 0;
    return s.valores.map((v, i) => (i >= i0 ? (acc += v) : 0));
  });
  const total = cum.reduce((a, k) => a + (k[n - 1] ?? 0), 0);
  const real = sem?.hayDatos ? sem.pct : total;
  const ym = Math.max(1, total, real);
  const X = (ts: number) => px0 + Math.min(1, Math.max(0, (ts - t0) / (ahora - t0))) * pw;
  const Y = (v: number) => py0 + ph - (v / ym) * ph;
  for (const v of [0, ym / 2, ym]) lineaH(c, px0, px0 + pw, Y(v), v === 0 ? t.rejillaFuerte : t.rejilla);
  c.font = fuente(9);
  c.textAlign = "right";
  c.textBaseline = "middle";
  c.fillStyle = t.tenue;
  c.fillText("0", px0 - 5, Y(0));
  c.fillText(`${Math.round(ym)}%`, px0 - 5, Y(ym));

  // Rayas cada 6 h (más marcadas a medianoche) y letra del día centrada en lo que se ve de él
  const dia0 = new Date(t0 * 1000);
  dia0.setHours(0, 0, 0, 0);
  const diaN = new Date(ahora * 1000);
  diaN.setHours(0, 0, 0, 0);
  c.textAlign = "center";
  for (const f = new Date(dia0); f <= diaN; f.setDate(f.getDate() + 1)) {
    const med = f.getTime() / 1000;
    for (let k = 0; k < 4; k++) {
      const ts = med + k * 21600;
      if (ts > t0 && ts < ahora) lineaV(c, X(ts), py0, py0 + ph, k === 0 ? t.rejillaFuerte : t.rejilla);
    }
    const a = Math.max(med, t0);
    const b = Math.min(med + DIA_S, ahora);
    if (X(b) - X(a) >= 16) {
      const hoy = f.getTime() === diaN.getTime();
      c.fillStyle = hoy ? t.texto : t.tenue;
      c.font = fuente(9, hoy);
      c.fillText(letraDe(f), (X(a) + X(b)) / 2, py0 + ph + 13);
    }
  }

  // Puntos de la curva: (tiempo, hora de la que sale el valor; null = 0). La primera hora con
  // consumo sube en vertical desde 0 (arranca recta, como el resto de saltos).
  const primera = (() => {
    for (let i = i0; i < n; i++) if (cum.some((k) => (k[i] ?? 0) > 0)) return i;
    return -1;
  })();
  const puntos: [number, number | null][] = [[t0, null]];
  for (let i = i0; i < n; i++) {
    if (i === primera) {
      const inicio = Math.min(Math.max(ini + i * 3600, t0), ahora);
      puntos.push([inicio, null], [inicio, i]);
    }
    puntos.push([Math.min(ini + (i + 1) * 3600, ahora), i]);
  }
  let base = new Array<number>(puntos.length).fill(0);
  // La capa que empezó antes va más abajo (su color sigue siendo el de su proyecto)
  const empezo = (k: number) => {
    const i = cum[k].findIndex((v) => v > 0);
    return i < 0 ? 1e6 : i;
  };
  const orden = cum.map((_, k) => k).sort((a, b) => empezo(a) - empezo(b));
  for (const k of orden) {
    const arriba = puntos.map(([, i], j) => base[j] + (i === null ? 0 : cum[k][i]));
    const col = colorProyecto(horas.series[k].nombre, t);
    c.beginPath();
    puntos.forEach(([ts], j) => (j ? c.lineTo(X(ts), Y(arriba[j])) : c.moveTo(X(ts), Y(arriba[j]))));
    for (let j = puntos.length - 1; j >= 0; j--) c.lineTo(X(puntos[j][0]), Y(base[j]));
    c.closePath();
    const g = c.createLinearGradient(0, py0, 0, py0 + ph);
    g.addColorStop(0, conAlfa(col, 0.85));
    g.addColorStop(1, conAlfa(col, 0.4));
    c.fillStyle = g;
    c.fill();
    base = arriba;
  }
  // Raya en «ahora» con el % real
  const xa = px0 + pw;
  lineaV(c, xa, py0, py0 + ph, t.texto);
  c.fillStyle = t.texto;
  c.font = fuente(9, true);
  c.textAlign = "left";
  c.fillText(`${Math.round(real)}%`, xa + 5, Math.max(py0 + 6, Y(total) - 2));
};

/** Nombres de las series, para la leyenda (en el orden recibido). */
export function leyenda(el: HTMLElement, nombres: string[]): void {
  const t = temaGrafica();
  const clave = `${t.cian}\u0001${nombres.join("\u0001")}`; // cambia con los nombres y con el tema
  if (el.dataset.clave === clave) return; // sin cambios: no se toca el DOM
  el.dataset.clave = clave;
  el.replaceChildren(
    ...nombres.map((n) => {
      const item = document.createElement("span");
      item.className = "cv__ley";
      const caja = document.createElement("i");
      caja.style.background = colorProyecto(n, t);
      const texto = document.createElement("span");
      texto.textContent = n;
      item.append(caja, texto);
      return item;
    }),
  );
}
