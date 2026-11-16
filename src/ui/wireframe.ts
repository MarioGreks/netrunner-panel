// Toro de alambre cian en canvas 2D con proyección en perspectiva propia (sin librerías).
// Solo anima mientras el panel está visible: main.ts llama a iniciar() y parar().

const GRANDES = 18; // segmentos del anillo mayor
const PEQUENOS = 8; // segmentos de la sección del tubo
const R = 1; // radio mayor
const r = 0.4; // radio del tubo
const INTERVALO = 1000 / 30; // ~30 fps

export interface Wireframe {
  iniciar: () => void;
  parar: () => void;
}

interface Punto3 {
  x: number;
  y: number;
  z: number;
}

export function crearWireframe(canvas: HTMLCanvasElement): Wireframe {
  const ctx = canvas.getContext("2d");
  const movimientoReducido = window.matchMedia("(prefers-reduced-motion: reduce)");

  // Vértices del toro, indexados por (anillo mayor, punto del tubo)
  const vertices: Punto3[] = [];
  for (let i = 0; i < GRANDES; i++) {
    const u = (i / GRANDES) * Math.PI * 2;
    for (let j = 0; j < PEQUENOS; j++) {
      const v = (j / PEQUENOS) * Math.PI * 2;
      vertices.push({
        x: (R + r * Math.cos(v)) * Math.cos(u),
        y: (R + r * Math.cos(v)) * Math.sin(u),
        z: r * Math.sin(v),
      });
    }
  }
  const idx = (i: number, j: number) => (i % GRANDES) * PEQUENOS + (j % PEQUENOS);
  const aristas: [number, number][] = [];
  for (let i = 0; i < GRANDES; i++) {
    for (let j = 0; j < PEQUENOS; j++) {
      aristas.push([idx(i, j), idx(i, j + 1)], [idx(i, j), idx(i + 1, j)]);
    }
  }

  const color = () => getComputedStyle(document.documentElement).getPropertyValue("--cian").trim() || "#2de2e6";
  let trazo = color();

  const dibujar = (t: number) => {
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const ancho = canvas.clientWidth;
    const alto = canvas.clientHeight;
    if (ancho === 0 || alto === 0) return;
    if (canvas.width !== Math.round(ancho * dpr) || canvas.height !== Math.round(alto * dpr)) {
      canvas.width = Math.round(ancho * dpr);
      canvas.height = Math.round(alto * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, ancho, alto);

    // Giro lento sobre dos ejes, con el toro algo inclinado
    const ax = 1.05 + Math.sin(t * 0.25) * 0.18;
    const ay = t * 0.45;
    const [sx, cx] = [Math.sin(ax), Math.cos(ax)];
    const [sy, cy] = [Math.sin(ay), Math.cos(ay)];
    const escala = (Math.min(alto / 2, ancho / 2) * 0.92) / (R + r);
    const cam = 4.2;

    const proy = vertices.map((p) => {
      const y1 = p.y * cx - p.z * sx;
      const z1 = p.y * sx + p.z * cx;
      const x2 = p.x * cy + z1 * sy;
      const z2 = -p.x * sy + z1 * cy;
      const f = cam / (cam + z2);
      return { x: ancho / 2 + x2 * f * escala, y: alto / 2 + y1 * f * escala, z: z2 };
    });

    ctx.lineWidth = 0.8;
    ctx.strokeStyle = trazo;
    ctx.shadowColor = trazo;
    ctx.shadowBlur = 3;
    // Las aristas lejanas son más tenues: da sensación de profundidad
    for (const franja of [0.3, 0.6, 1]) {
      ctx.globalAlpha = 0.25 + franja * 0.6;
      ctx.beginPath();
      for (const [a, b] of aristas) {
        const prof = (proy[a].z + proy[b].z) / 2; // de -(R+r) a (R+r)
        const norm = 1 - (prof + R + r) / (2 * (R + r)); // 1 = cerca, 0 = lejos
        const grupo = norm < 0.35 ? 0.3 : norm < 0.65 ? 0.6 : 1;
        if (grupo !== franja) continue;
        ctx.moveTo(proy[a].x, proy[a].y);
        ctx.lineTo(proy[b].x, proy[b].y);
      }
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    ctx.shadowBlur = 0;
  };

  let marco = 0;
  let ultimo = 0;
  let t = 0;
  const paso = (ahora: number) => {
    marco = requestAnimationFrame(paso);
    const dt = ahora - ultimo;
    if (dt < INTERVALO) return;
    ultimo = ahora;
    t += Math.min(dt, 100) / 1000;
    dibujar(t);
  };

  // Primer dibujo estático, para que no aparezca vacío al abrir
  dibujar(0.6);

  return {
    iniciar() {
      trazo = color();
      if (movimientoReducido.matches) {
        dibujar(0.6);
        return;
      }
      if (marco) return;
      ultimo = performance.now();
      marco = requestAnimationFrame(paso);
    },
    parar() {
      if (marco) cancelAnimationFrame(marco);
      marco = 0;
    },
  };
}
