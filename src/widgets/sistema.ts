// Widget SISTEMA: CPU, RAM y GPU en barras finas, discos, batería y, con alto suficiente, una
// minigráfica de CPU de los últimos ~2 minutos. Las muestras llegan por `sistema://muestra`
// (Rust mide cada 3 s si solo mira el widget); sin muestras se queda con la última, atenuada.
import {
  abrirTarjeta,
  alEvento,
  historialSistema,
  muestraSistema,
  type MuestraSistema,
} from "../lib/ipc";
import { EVENTO_TEMA } from "../lib/tema";
import { esc, gb } from "../lib/texto";
import type { ContextoWidget } from "../widget";
import { escuchas, prepararCuerpo } from "./comun";

const UMBRAL_ALTO = 90; // por encima, el valor se pinta en rojo
const VENTANA_MS = 120000; // lo que enseña la gráfica

interface Punto {
  t: number;
  cpu: number;
}

const pct = (v: number) => Math.min(100, Math.max(0, v));

const barra = (id: string, etiqueta: string) => `
  <div class="si__fila" data-id="${id}">
    <span class="si__etq">${etiqueta}</span>
    <div class="si__barra"><i></i></div>
    <span class="si__val">--</span>
  </div>`;

export function montarSistema(cuerpo: HTMLElement, ctx: ContextoWidget): () => void {
  prepararCuerpo(cuerpo, "wgc--sistema");
  cuerpo.innerHTML = `
    <div class="si">
      <div class="si__medidas">
        ${barra("cpu", "CPU")}
        ${barra("ram", "RAM")}
        ${barra("gpu", "GPU")}
      </div>
      <div class="si__discos" data-id="discos"></div>
      <div class="si__bat" data-id="bat" hidden>
        <span class="si__etq">BAT</span>
        <span class="si__val" data-id="bat-val">--</span>
        <span class="si__sub" data-id="bat-sub"></span>
      </div>
      <canvas class="si__onda" data-id="onda"></canvas>
    </div>`;
  const raiz = cuerpo.querySelector<HTMLElement>(".si")!;
  const q = (id: string) => cuerpo.querySelector<HTMLElement>(`[data-id="${id}"]`)!;
  const fila = (id: string) => {
    const f = q(id);
    return {
      f,
      relleno: f.querySelector<HTMLElement>(".si__barra i")!,
      val: f.querySelector<HTMLElement>(".si__val")!,
    };
  };
  const cpu = fila("cpu");
  const ram = fila("ram");
  const gpu = fila("gpu");
  const elDiscos = q("discos");
  const elBat = q("bat");
  const elBatVal = q("bat-val");
  const elBatSub = q("bat-sub");
  const onda = q("onda") as HTMLCanvasElement;

  let puntos: Punto[] = [];
  let ultima: MuestraSistema | null = null;

  const ponerFila = (r: ReturnType<typeof fila>, valor: number | null, texto: string) => {
    r.f.classList.toggle("si__fila--nd", valor === null);
    r.f.classList.toggle("si__fila--alta", valor !== null && valor > UMBRAL_ALTO);
    r.relleno.style.width = `${valor === null ? 0 : pct(valor)}%`;
    r.val.textContent = texto;
  };

  // --- Gráfica de CPU (canvas, se dibuja solo al llegar una muestra o cambiar el tamaño) ---
  const dibujar = () => {
    const r = onda.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return; // oculta por el tamaño
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(r.width * dpr);
    const h = Math.round(r.height * dpr);
    if (onda.width !== w || onda.height !== h) {
      onda.width = w;
      onda.height = h;
    }
    const c = onda.getContext("2d")!;
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    c.clearRect(0, 0, r.width, r.height);
    const estilo = getComputedStyle(onda);
    const cian = estilo.getPropertyValue("--cian").trim() || "#2de2e6";
    const rejilla = estilo.getPropertyValue("--linea-neutra").trim() || "rgba(255,255,255,0.06)";
    // Rejilla horizontal al 50 %
    c.strokeStyle = rejilla;
    c.lineWidth = 1;
    c.beginPath();
    c.moveTo(0, Math.round(r.height / 2) + 0.5);
    c.lineTo(r.width, Math.round(r.height / 2) + 0.5);
    c.stroke();
    const fin = puntos.length ? puntos[puntos.length - 1].t : Date.now();
    const visibles = puntos.filter((p) => p.t >= fin - VENTANA_MS);
    if (visibles.length < 2) return;
    const x = (t: number) => ((t - (fin - VENTANA_MS)) / VENTANA_MS) * r.width;
    const y = (v: number) => r.height - 1 - (pct(v) / 100) * (r.height - 2);
    c.beginPath();
    visibles.forEach((p, i) => (i === 0 ? c.moveTo(x(p.t), y(p.cpu)) : c.lineTo(x(p.t), y(p.cpu))));
    c.strokeStyle = cian;
    c.lineWidth = 1.5;
    c.lineJoin = "round";
    c.stroke();
    c.lineTo(x(visibles[visibles.length - 1].t), r.height);
    c.lineTo(x(visibles[0].t), r.height);
    c.closePath();
    c.globalAlpha = 0.14;
    c.fillStyle = cian;
    c.fill();
    c.globalAlpha = 1;
  };

  // --- Discos: se rehacen solo si cambia la lista de unidades ---
  let claveDiscos = "";
  let discos: { relleno: HTMLElement; val: HTMLElement; fila: HTMLElement }[] = [];
  const pintarDiscos = (lista: MuestraSistema["discos"]) => {
    const clave = lista.map((d) => d.montaje).join("|");
    if (clave !== claveDiscos) {
      claveDiscos = clave;
      elDiscos.innerHTML = lista
        .map((d) => {
          const letra = d.montaje.replace(/[\\/]+$/, "") || d.montaje;
          return `<div class="si__fila si__disco" title="${esc(d.montaje)}"><span class="si__etq">${esc(letra)}</span><div class="si__barra"><i></i></div><span class="si__val">--</span></div>`;
        })
        .join("");
      discos = Array.from(elDiscos.children).map((f) => ({
        fila: f as HTMLElement,
        relleno: f.querySelector<HTMLElement>(".si__barra i")!,
        val: f.querySelector<HTMLElement>(".si__val")!,
      }));
    }
    lista.forEach((d, i) => {
      const usado = d.total > 0 ? ((d.total - d.libre) / d.total) * 100 : 0;
      discos[i].relleno.style.width = `${pct(usado)}%`;
      discos[i].val.textContent = `${Math.round(usado)}%`;
      discos[i].fila.classList.toggle("si__fila--alta", usado > UMBRAL_ALTO);
    });
  };

  const pintar = (m: MuestraSistema) => {
    ultima = m;
    ponerFila(cpu, m.cpu_total, `${Math.round(m.cpu_total)}%`);
    const pctRam = m.ram_total > 0 ? (m.ram_usada / m.ram_total) * 100 : 0;
    ponerFila(ram, pctRam, `${gb(m.ram_usada)}/${gb(m.ram_total)}G`);
    ponerFila(gpu, m.gpu_uso, m.gpu_uso === null ? "N/D" : `${Math.round(m.gpu_uso)}%`);
    pintarDiscos(m.discos);
    elBat.hidden = m.bateria === null;
    if (m.bateria) {
      elBatVal.textContent = `${Math.round(m.bateria.porcentaje)}%`;
      elBatSub.textContent = m.bateria.cargando ? "CARGANDO" : "";
      elBat.classList.toggle("si__bat--carga", m.bateria.cargando);
      elBat.classList.toggle("si__bat--baja", !m.bateria.cargando && m.bateria.porcentaje < 15);
    }
  };

  const recibir = (m: MuestraSistema) => {
    // Con el widget tapado no se pinta nada: se guarda la muestra para el regreso
    ultima = m;
    if (!ctx.visible()) return;
    raiz.classList.remove("si--viejo");
    pintar(m);
    puntos.push({ t: Date.now(), cpu: m.cpu_total });
    const corte = Date.now() - VENTANA_MS;
    if (puntos.length > 0 && puntos[0].t < corte) puntos = puntos.filter((p) => p.t >= corte);
    dibujar();
  };

  // Al taparse se atenúa la última muestra; al volver se pide una al momento
  ctx.alVisibilidad((v) => {
    if (!v) {
      raiz.classList.add("si--viejo");
      return;
    }
    if (ultima) pintar(ultima);
    dibujar();
    void muestraSistema().then(recibir).catch(() => {});
  });
  if (!ctx.visible()) raiz.classList.add("si--viejo");

  const escucha = escuchas();
  escucha.anadir(alEvento<MuestraSistema>("sistema://muestra", recibir));

  // Historial previo para que la gráfica no empiece vacía
  historialSistema()
    .then((h) => {
      const corte = Date.now() - VENTANA_MS;
      const previos = h.filter((p) => p.t >= corte).map((p) => ({ t: p.t, cpu: p.cpu }));
      const ultimoT = previos.length ? previos[previos.length - 1].t : 0;
      puntos = previos.concat(puntos.filter((p) => p.t > ultimoT));
      dibujar();
    })
    .catch(() => {});
  void muestraSistema().then(recibir).catch(() => {});

  // Redibujo por eventos: cambio de tamaño y de tema
  let medicion = 0;
  const observador = new ResizeObserver(() => {
    cancelAnimationFrame(medicion);
    medicion = requestAnimationFrame(dibujar);
  });
  observador.observe(onda);
  window.addEventListener(EVENTO_TEMA, dibujar);

  const alClic = () => void abrirTarjeta("sistema");
  cuerpo.addEventListener("click", alClic);

  return () => {
    cancelAnimationFrame(medicion);
    observador.disconnect();
    window.removeEventListener(EVENTO_TEMA, dibujar);
    escucha.limpiar();
    cuerpo.removeEventListener("click", alClic);
  };
}
