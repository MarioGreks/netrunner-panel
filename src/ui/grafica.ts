// Gráficas HUD reutilizables, sin librerías: onda en canvas y anillo segmentado en SVG.
import { EVENTO_TEMA } from "../lib/tema";

const PUNTOS = 60;
const SUAVIZADO = 0.25; // fracción que se acerca al valor objetivo por fotograma

export interface OpcionesOnda {
  color: string; // color CSS, p. ej. "var(--primario)" o un hex
  max: number; // valor que corresponde a la altura completa
}

export interface Onda {
  /** Añade un valor nuevo (desplaza la historia) */
  agregar(valor: number): void;
  /** Detiene la animación */
  destruir(): void;
}

/** Resuelve `var(--x)` a un color real, ya que el canvas no entiende variables CSS. */
function resolverColor(canvas: HTMLElement, color: string): string {
  const m = color.match(/^var\((--[\w-]+)\)$/);
  if (!m) return color;
  return getComputedStyle(canvas).getPropertyValue(m[1]).trim() || getComputedStyle(canvas).color;
}

/** Color de la cuadrícula (token neutro), resuelto desde el CSS. */
function colorCuadricula(canvas: HTMLElement): string {
  return getComputedStyle(canvas).getPropertyValue("--linea-neutra").trim() || "transparent";
}

/** Convierte "#rrggbb" a rgba con alfa; otros formatos pasan por color-mix. */
function conAlfa(color: string, alfa: number): string {
  const m = color.match(/^#([0-9a-f]{6})$/i);
  if (!m) return `color-mix(in srgb, ${color} ${alfa * 100}%, transparent)`;
  const n = parseInt(m[1], 16);
  return `rgba(${n >> 16}, ${(n >> 8) & 255}, ${n & 255}, ${alfa})`;
}

export function crearOnda(canvas: HTMLCanvasElement, opciones: OpcionesOnda): Onda {
  const ctx = canvas.getContext("2d")!;
  // Los colores se releen al cambiar de tema (evento de ventana), no en cada fotograma
  let color = resolverColor(canvas, opciones.color);
  let cuadricula = colorCuadricula(canvas);
  const valores: number[] = new Array(PUNTOS).fill(0);
  // Fracción 0..1 de desplazamiento horizontal entre muestras, para el movimiento suave
  let avance = 1;
  let marca = performance.now();
  let raf = 0;
  let ancho = 0;
  let alto = 0;

  const ajustar = () => {
    const dpr = window.devicePixelRatio || 1;
    const r = canvas.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    ancho = r.width;
    alto = r.height;
    const w = Math.round(ancho * dpr);
    const h = Math.round(alto * dpr);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return true;
  };

  // Solo se anima mientras dura el desplazamiento tras cada muestra (1 s);
  // después el bucle se para hasta la siguiente muestra, para no gastar GPU
  // redibujando a 60 fps con el panel quieto u oculto.
  const dibujar = (ahora: number) => {
    raf = 0;
    if (!ajustar()) {
      raf = requestAnimationFrame(dibujar); // aún sin tamaño: reintentar
      return;
    }
    avance = Math.min(1, (ahora - marca) / 1000);
    if (avance < 1) raf = requestAnimationFrame(dibujar);
    ctx.clearRect(0, 0, ancho, alto);

    // Cuadrícula tenue
    ctx.lineWidth = 1;
    ctx.strokeStyle = cuadricula;
    ctx.beginPath();
    for (let i = 1; i < 4; i++) {
      const y = Math.round((alto * i) / 4) + 0.5;
      ctx.moveTo(0, y);
      ctx.lineTo(ancho, y);
    }
    for (let i = 1; i < 6; i++) {
      const x = Math.round((ancho * i) / 6) + 0.5;
      ctx.moveTo(x, 0);
      ctx.lineTo(x, alto);
    }
    ctx.stroke();

    // Puntos de la línea; el conjunto se desliza a la izquierda entre muestras
    const paso = ancho / (PUNTOS - 2);
    const desplazamiento = avance * paso;
    const y = (v: number) => alto - 2 - Math.min(1, Math.max(0, v / opciones.max)) * (alto - 4);
    ctx.beginPath();
    valores.forEach((v, i) => {
      const px = i * paso - desplazamiento;
      if (i === 0) ctx.moveTo(px, y(v));
      else ctx.lineTo(px, y(v));
    });
    const lineaX = (PUNTOS - 1) * paso - desplazamiento;

    // Relleno degradado bajo la línea
    ctx.save();
    ctx.lineTo(lineaX, alto);
    ctx.lineTo(-desplazamiento, alto);
    ctx.closePath();
    const g = ctx.createLinearGradient(0, 0, 0, alto);
    g.addColorStop(0, conAlfa(color, 0.35));
    g.addColorStop(1, conAlfa(color, 0));
    ctx.fillStyle = g;
    ctx.fill();
    ctx.restore();

    // Línea
    ctx.beginPath();
    valores.forEach((v, i) => {
      const px = i * paso - desplazamiento;
      if (i === 0) ctx.moveTo(px, y(v));
      else ctx.lineTo(px, y(v));
    });
    ctx.lineWidth = 1.5;
    ctx.lineJoin = "round";
    ctx.strokeStyle = color;
    ctx.shadowColor = color;
    ctx.shadowBlur = 4;
    ctx.stroke();
    ctx.shadowBlur = 0;
  };
  raf = requestAnimationFrame(dibujar);

  const alCambiarTema = () => {
    color = resolverColor(canvas, opciones.color);
    cuadricula = colorCuadricula(canvas);
    if (!raf) raf = requestAnimationFrame(dibujar);
  };
  window.addEventListener(EVENTO_TEMA, alCambiarTema);

  return {
    agregar(valor) {
      valores.push(valor);
      valores.shift();
      marca = performance.now();
      avance = 0;
      if (!raf) raf = requestAnimationFrame(dibujar);
    },
    destruir() {
      cancelAnimationFrame(raf);
      window.removeEventListener(EVENTO_TEMA, alCambiarTema);
    },
  };
}

// --- Anillo segmentado ---

export interface OpcionesAnillo {
  color: string; // color CSS (admite var(--x))
  segmentos?: number; // número de muescas, por defecto 40
  tamano?: number; // lado en px, por defecto 96
}

export interface Anillo {
  /** Fija el valor 0..100; null muestra "N/D" atenuado. `inmediato` salta la animación de relleno. */
  fijar(valor: number | null, inmediato?: boolean): void;
  /** Marca fina en el anillo (p. ej. el % esperado); null la quita. */
  marca(valor: number | null): void;
  /** Tramo tenue desde el valor actual hasta `hasta` (p. ej. el % estimado); null lo quita. */
  tramo(hasta: number | null): void;
  /** Cambia el color de los segmentos (admite var(--x)). */
  colorAnillo(color: string): void;
  /** Cambia el color del valor central (p. ej. rojo por encima del umbral) */
  colorValor(color: string | null): void;
  destruir(): void;
}

const NS = "http://www.w3.org/2000/svg";

export function crearAnillo(contenedor: HTMLElement, opciones: OpcionesAnillo): Anillo {
  const n = opciones.segmentos ?? 40;
  const lado = opciones.tamano ?? 96;
  const c = lado / 2;
  const rExt = c - 3;
  const rInt = rExt - 8;
  const hueco = 0.18; // fracción angular de cada segmento que queda vacía (muesca)

  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${lado} ${lado}`);
  svg.setAttribute("width", String(lado));
  svg.setAttribute("height", String(lado));
  svg.classList.add("anillo");
  svg.style.setProperty("--color-anillo", opciones.color);

  const segmentos: SVGPathElement[] = [];
  const grado = (Math.PI * 2) / n;
  for (let i = 0; i < n; i++) {
    // Empieza arriba (-90 grados) y avanza en sentido horario
    const a0 = -Math.PI / 2 + i * grado + (grado * hueco) / 2;
    const a1 = -Math.PI / 2 + (i + 1) * grado - (grado * hueco) / 2;
    const p = (r: number, a: number) => `${(c + r * Math.cos(a)).toFixed(2)} ${(c + r * Math.sin(a)).toFixed(2)}`;
    const d = `M${p(rInt, a0)} L${p(rExt, a0)} L${p(rExt, a1)} L${p(rInt, a1)}Z`;
    const el = document.createElementNS(NS, "path");
    el.setAttribute("d", d);
    el.setAttribute("class", "anillo__seg");
    svg.appendChild(el);
    segmentos.push(el);
  }

  // Marca fina radial (opcional), sobresale un poco del anillo
  const marcaEl = document.createElementNS(NS, "line");
  marcaEl.setAttribute("class", "anillo__marca");
  marcaEl.setAttribute("display", "none");
  svg.appendChild(marcaEl);

  const texto = document.createElementNS(NS, "text");
  texto.setAttribute("x", String(c));
  texto.setAttribute("y", String(c));
  texto.setAttribute("class", "anillo__valor");
  texto.setAttribute("text-anchor", "middle");
  texto.setAttribute("dominant-baseline", "central");
  svg.appendChild(texto);
  contenedor.appendChild(svg);

  let actual = 0;
  let objetivo = 0;
  let raf = 0;
  let sinDato = false;
  let hastaTramo: number | null = null;

  const pintar = () => {
    const encendidos = (actual / 100) * n;
    const tramoFin = hastaTramo === null ? 0 : (Math.min(100, hastaTramo) / 100) * n;
    segmentos.forEach((s, i) => {
      // El segmento parcial se enciende con opacidad proporcional
      const f = Math.min(1, Math.max(0, encendidos - i));
      const t = f === 0 ? Math.min(1, Math.max(0, tramoFin - i)) : 0;
      s.style.opacity = String(t > 0 ? 0.14 + t * 0.26 : 0.14 + f * 0.86);
      s.classList.toggle("anillo__seg--on", f > 0);
    });
    texto.textContent = sinDato ? "N/D" : `${Math.round(actual)}%`;
  };

  const paso = () => {
    const dif = objetivo - actual;
    if (Math.abs(dif) < 0.1) {
      actual = objetivo;
      raf = 0;
    } else {
      actual += dif * SUAVIZADO;
      raf = requestAnimationFrame(paso);
    }
    pintar();
  };

  pintar();

  return {
    fijar(valor, inmediato = false) {
      sinDato = valor === null;
      objetivo = valor === null ? 0 : Math.min(100, Math.max(0, valor));
      svg.classList.toggle("anillo--nd", sinDato);
      if (inmediato) {
        cancelAnimationFrame(raf);
        raf = 0;
        actual = objetivo;
        pintar();
      } else if (!raf) raf = requestAnimationFrame(paso);
    },
    marca(valor) {
      if (valor === null) {
        marcaEl.setAttribute("display", "none");
        return;
      }
      const a = -Math.PI / 2 + (Math.min(100, Math.max(0, valor)) / 100) * Math.PI * 2;
      const p = (r: number) => [(c + r * Math.cos(a)).toFixed(2), (c + r * Math.sin(a)).toFixed(2)];
      const [x1, y1] = p(rInt - 3);
      const [x2, y2] = p(rExt + 2);
      marcaEl.setAttribute("x1", x1);
      marcaEl.setAttribute("y1", y1);
      marcaEl.setAttribute("x2", x2);
      marcaEl.setAttribute("y2", y2);
      marcaEl.removeAttribute("display");
    },
    tramo(hasta) {
      hastaTramo = hasta;
      pintar();
    },
    colorAnillo(color) {
      svg.style.setProperty("--color-anillo", color);
    },
    colorValor(color) {
      if (color) texto.style.fill = color;
      else texto.style.removeProperty("fill");
    },
    destruir() {
      cancelAnimationFrame(raf);
      raf = 0;
    },
  };
}
