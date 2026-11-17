// Transición glitch «Cortes + RGB» entre pantallas (~0.5 s).
import { reducido } from "./util";

const I = 1.6; // intensidad
const R = (a: number, b: number) => a + Math.random() * (b - a);

type Rgb = [number, number, number];

function leerColor(estilo: CSSStyleDeclaration, nombre: string, defecto: Rgb): Rgb {
  const v = estilo.getPropertyValue(nombre).trim();
  const h = /^#([0-9a-f]{6})$/i.exec(v);
  if (h) {
    const n = parseInt(h[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  const c = /^#([0-9a-f]{3})$/i.exec(v);
  if (c) return [...c[1]].map((x) => parseInt(x + x, 16)) as Rgb;
  return defecto;
}

function tono([r, g, b]: Rgb): number {
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const d = mx - mn;
  if (d === 0) return 0;
  let h: number;
  if (mx === r) h = ((g - b) / d) % 6;
  else if (mx === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return (h * 60 + 360) % 360;
}

const rgba = ([r, g, b]: Rgb, a: number) => `rgba(${r},${g},${b},${a})`;
const aclarar = ([r, g, b]: Rgb): Rgb =>
  [(r + 510) / 3, (g + 510) / 3, (b + 510) / 3].map(Math.round) as Rgb;

/** `contenedor` es el interior de la ventana; la pantalla actual vive en `#pantalla`. */
export async function transicion(contenedor: HTMLElement, cambiar: () => void): Promise<void> {
  const pantalla = contenedor.querySelector<HTMLElement>("#pantalla");
  if (!pantalla) {
    cambiar();
    return;
  }
  const dur = reducido() ? 160 : 500;
  const estilo = getComputedStyle(contenedor);
  const alerta = leerColor(estilo, "--alerta", [255, 42, 77]);
  const cian = estilo.getPropertyValue("--secundario").trim() || "#2de2e6";
  const rojo = estilo.getPropertyValue("--alerta").trim() || "#ff2a4d";
  const giro = tono(alerta) - 38; // el sepia parte de ~38°

  const ancho = contenedor.clientWidth;
  const alto = contenedor.clientHeight;
  const canvas = document.createElement("canvas");
  canvas.width = ancho;
  canvas.height = alto;
  canvas.style.cssText = "position:absolute;inset:0;pointer-events:none;z-index:50";
  contenedor.appendChild(canvas);
  const ctx = canvas.getContext("2d")!;

  let clones: HTMLElement[] = [];
  let cambiado = false;
  const limpiarClones = () => {
    clones.forEach((c) => c.remove());
    clones = [];
  };

  await new Promise<void>((fin) => {
    const t0 = performance.now();
    const cuadro = (ahora: number) => {
      const p = Math.min(1, (ahora - t0) / dur);
      const e = Math.sin(p * Math.PI);

      // Se quitan los clones antes de cambiar para no clonarlos ni perderlos
      limpiarClones();
      if (!cambiado && p >= 0.5) {
        cambiado = true;
        cambiar();
      }

      // Canvas: franjas y ruido
      ctx.clearRect(0, 0, ancho, alto);
      const franjas = Math.round(R(4, 9) * I * (0.4 + e));
      for (let i = 0; i < franjas; i++) {
        const y = R(0, alto);
        const h = R(1, 22);
        ctx.fillStyle = rgba(alerta, 0.35 * R(0.4, 1));
        ctx.fillRect(0, y, ancho, h);
        const pts = Math.round(h * 14);
        for (let k = 0; k < pts; k++) {
          ctx.fillStyle = rgba(alerta, R(0.2, 0.9));
          ctx.fillRect(R(0, ancho), y + R(0, h), R(1, 3), 1);
        }
        ctx.fillStyle = rgba(aclarar(alerta), 0.5);
        ctx.fillRect(0, y + h / 2, ancho, 1);
      }

      // Fantasmas: clones recortados a franjas
      const fuente = pantalla.firstElementChild as HTMLElement | null;
      if (fuente) {
        const n = Math.round(R(1, 3) * I * e);
        for (let i = 0; i < n; i++) {
          const c = document.createElement("div");
          c.setAttribute("aria-hidden", "true");
          c.appendChild(fuente.cloneNode(true));
          const top = R(0, 85);
          const h = R(3, 18);
          c.style.cssText =
            "position:absolute;inset:0;pointer-events:none;mix-blend-mode:screen;" +
            `clip-path:inset(${top}% 0 ${Math.max(0, 100 - top - h)}% 0);` +
            `transform:translateX(${R(-30, 30) * I * e}px);`;
          if (Math.random() < 0.6) c.style.filter = `sepia(1) saturate(4) hue-rotate(${giro}deg)`;
          pantalla.appendChild(c);
          clones.push(c);
        }
      }

      // Contenido real: temblor y separación RGB
      pantalla.style.transform = `translateX(${R(-6, 6) * I * e}px)`;
      pantalla.style.textShadow = `${3 * I * e}px 0 ${rojo}, ${-3 * I * e}px 0 ${cian}`;

      if (p < 1) requestAnimationFrame(cuadro);
      else fin();
    };
    requestAnimationFrame(cuadro);
  });

  if (!cambiado) cambiar();
  limpiarClones();
  canvas.remove();
  pantalla.style.transform = "";
  pantalla.style.textShadow = "";
}
