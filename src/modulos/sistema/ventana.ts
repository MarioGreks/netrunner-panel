// Vista ampliada de SISTEMA (ventana desacoplada): historial de 10 minutos, núcleos, procesos y
// resumen. Mientras la ventana está a la vista el núcleo mide cada segundo; oculta o minimizada
// no se mide, y aquí tampoco se pide nada (los procesos solo se refrescan si el documento es visible).
import "../../styles/sistema-ventana.css";
import {
  alEvento,
  historialSistema,
  muestraSistema,
  procesosSistema,
  type MuestraSistema,
  type Proceso,
  type PuntoHistorial,
} from "../../lib/ipc";
import { crearOnda, type Onda } from "../../ui/grafica";
import { esc } from "../notas/vista";
import { UMBRAL_ALTO } from "./vista";

const MAX_PUNTOS = 600; // 10 min a una muestra por segundo (igual que el búfer del núcleo)
const REFRESCO_PROCESOS_MS = 2000;
/** Por debajo de este ancho la rejilla pasa a una columna. */
const ANCHO_UNA_COLUMNA = 620;
const GB = 1024 ** 3;

const SERIES = [
  { id: "cpu", nombre: "CPU", token: "--amarillo" },
  { id: "ram", nombre: "RAM", token: "--cian" },
  { id: "gpu", nombre: "GPU", token: "--texto" },
] as const;
type IdSerie = (typeof SERIES)[number]["id"];

const valorDe = (p: PuntoHistorial, id: IdSerie): number | null =>
  id === "cpu" ? p.cpu : id === "ram" ? p.ram_pct : p.gpu;

const memoria = (b: number) => (b >= GB ? `${(b / GB).toFixed(1)} GB` : `${Math.round(b / 1024 ** 2)} MB`);
const gb = (b: number) => (b / GB).toFixed(1);

export function montarVentanaSistema(cont: HTMLElement): () => void {
  cont.innerHTML = `
    <div class="svm" data-id="raiz">
      <section class="marco marco--cian svm__hist">
        <div class="marco__cabecera"><span>HISTORIAL · 10 MIN</span><span class="marco__etiqueta">HIS_01</span></div>
        <div class="marco__cuerpo">
          ${SERIES.map(
            (s) => `
            <div class="svm__franja">
              <div class="svm__lado"><span class="svm__nombre">${s.nombre}</span><span class="svm__valor" data-id="val-${s.id}">--%</span></div>
              <canvas class="svm__canvas" data-id="can-${s.id}"></canvas>
            </div>`,
          ).join("")}
          <div class="svm__eje"><span>-10 MIN</span><span>-5 MIN</span><span>AHORA</span></div>
        </div>
      </section>

      <section class="marco svm__nuc">
        <div class="marco__cabecera"><span>NÚCLEOS</span><span class="marco__etiqueta" data-id="nuc-cuenta">--</span></div>
        <div class="marco__cuerpo"><div class="svm__nucleos" data-id="nucleos"></div></div>
      </section>

      <section class="marco svm__proc">
        <div class="marco__cabecera"><span>PROCESOS</span><span class="marco__etiqueta">PRC_01</span></div>
        <div class="marco__cuerpo">
          <table class="svm__tabla">
            <thead><tr>
              <th>NOMBRE</th>
              <th><button type="button" class="svm__orden" data-orden="cpu">CPU %</button></th>
              <th><button type="button" class="svm__orden" data-orden="memoria">RAM</button></th>
            </tr></thead>
            <tbody data-id="procesos"><tr><td colspan="3" class="svm__vacio">LEYENDO_</td></tr></tbody>
          </table>
        </div>
      </section>

      <section class="marco marco--cian svm__res">
        <div class="marco__cabecera"><span>RESUMEN</span><span class="marco__etiqueta">RES</span></div>
        <div class="marco__cuerpo">
          <div class="svm__fila"><span>RAM</span><b data-id="res-ram">-- / -- GB</b></div>
          <div class="svm__fila"><span>GPU TEMP</span><b data-id="res-temp">N/D</b></div>
          <div class="svm__fila" data-id="res-bat-fila" hidden><span>BATERÍA</span><b data-id="res-bat">--</b></div>
          <div class="svm__discos" data-id="res-discos"></div>
        </div>
      </section>
    </div>`;

  const q = <T extends HTMLElement>(id: string) => cont.querySelector<T>(`[data-id="${id}"]`)!;
  const raiz = q("raiz");

  // --- Historial ---
  const hist: PuntoHistorial[] = [];
  const canvas = Object.fromEntries(SERIES.map((s) => [s.id, q<HTMLCanvasElement>(`can-${s.id}`)])) as Record<IdSerie, HTMLCanvasElement>;
  const textos = Object.fromEntries(SERIES.map((s) => [s.id, q(`val-${s.id}`)])) as Record<IdSerie, HTMLElement>;
  let rafHist = 0;

  const dibujarFranja = (s: (typeof SERIES)[number]) => {
    const c = canvas[s.id];
    const r = c.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(r.width * dpr);
    const h = Math.round(r.height * dpr);
    if (c.width !== w || c.height !== h) {
      c.width = w;
      c.height = h;
    }
    const ctx = c.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, r.width, r.height);
    // El canvas no entiende var(--x): los tokens se resuelven en cada dibujo (uno por muestra)
    const estilo = getComputedStyle(c);
    const color = estilo.getPropertyValue(s.token).trim() || estilo.color;
    ctx.strokeStyle = estilo.getPropertyValue("--linea-neutra").trim() || "transparent";
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let i = 1; i < 4; i++) {
      const y = Math.round((r.height * i) / 4) + 0.5;
      ctx.moveTo(0, y);
      ctx.lineTo(r.width, y);
    }
    ctx.stroke();

    // Alineado a la derecha: la muestra más reciente es "AHORA"
    const paso = r.width / (MAX_PUNTOS - 1);
    const n = hist.length;
    const py = (v: number) => r.height - 2 - Math.min(1, Math.max(0, v / 100)) * (r.height - 4);
    const px = (i: number) => r.width - (n - 1 - i) * paso;
    ctx.beginPath();
    let abierto = false;
    let primeroX = 0;
    let ultimoX = 0;
    hist.forEach((p, i) => {
      const v = valorDe(p, s.id);
      if (v === null) {
        abierto = false; // hueco: la GPU puede no estar disponible
        return;
      }
      if (!abierto) {
        ctx.moveTo(px(i), py(v));
        if (primeroX === 0) primeroX = px(i);
        abierto = true;
      } else ctx.lineTo(px(i), py(v));
      ultimoX = px(i);
    });
    ctx.lineWidth = 1.5;
    ctx.lineJoin = "round";
    ctx.strokeStyle = color;
    ctx.stroke();
    if (abierto) {
      // Relleno tenue bajo la línea
      ctx.lineTo(ultimoX, r.height);
      ctx.lineTo(primeroX, r.height);
      ctx.closePath();
      ctx.globalAlpha = 0.12;
      ctx.fillStyle = color;
      ctx.fill();
      ctx.globalAlpha = 1;
    }
  };

  const dibujarHistorial = () => {
    rafHist = 0;
    SERIES.forEach(dibujarFranja);
  };
  // Como mucho un repintado por fotograma, aunque lleguen varias muestras o cambios de tamaño
  const pedirDibujo = () => {
    if (!rafHist) rafHist = requestAnimationFrame(dibujarHistorial);
  };

  const pintarValores = () => {
    const ult = hist[hist.length - 1];
    SERIES.forEach((s) => {
      const v = ult ? valorDe(ult, s.id) : null;
      textos[s.id].textContent = v === null ? "N/D" : `${Math.round(v)}%`;
      textos[s.id].classList.toggle("svm__valor--alto", v !== null && v > UMBRAL_ALTO);
    });
  };

  const agregarPunto = (p: PuntoHistorial) => {
    hist.push(p);
    if (hist.length > MAX_PUNTOS) hist.shift();
    pintarValores();
    pedirDibujo();
  };

  // --- Núcleos ---
  const nucleosEl = q("nucleos");
  let ondas: Onda[] = [];
  let valoresNuc: HTMLElement[] = [];
  const pintarNucleos = (v: number[]) => {
    if (ondas.length !== v.length) {
      ondas.forEach((o) => o.destruir());
      nucleosEl.replaceChildren(
        ...v.map((_, i) => {
          const t = document.createElement("div");
          t.className = "svm__nuc-item";
          t.innerHTML = `<span class="svm__nuc-id">C${String(i).padStart(2, "0")}</span><b class="svm__nuc-val">--</b><canvas class="svm__nuc-onda"></canvas>`;
          return t;
        }),
      );
      const items = Array.from(nucleosEl.children);
      valoresNuc = items.map((t) => t.querySelector<HTMLElement>(".svm__nuc-val")!);
      ondas = items.map((t) => crearOnda(t.querySelector<HTMLCanvasElement>("canvas")!, { color: "var(--amarillo)", max: 100 }));
      q("nuc-cuenta").textContent = `${v.length} NÚCLEOS`;
    }
    v.forEach((x, i) => {
      valoresNuc[i].textContent = `${Math.round(x)}%`;
      valoresNuc[i].classList.toggle("svm__valor--alto", x > UMBRAL_ALTO);
      ondas[i].agregar(x);
    });
  };

  // --- Resumen ---
  const discosEl = q("res-discos");
  const pintarResumen = (m: MuestraSistema) => {
    q("res-ram").textContent = `${gb(m.ram_usada)} / ${gb(m.ram_total)} GB`;
    q("res-temp").textContent = m.gpu_temp === null ? "N/D" : `${Math.round(m.gpu_temp)} °C`;
    q("res-bat-fila").hidden = m.bateria === null;
    if (m.bateria) q("res-bat").textContent = `${Math.round(m.bateria.porcentaje)}%${m.bateria.cargando ? " CARGANDO" : ""}`;
    discosEl.innerHTML = m.discos
      .map((d) => {
        const uso = d.total > 0 ? ((d.total - d.libre) / d.total) * 100 : 0;
        return `<div class="svm__disco${uso > UMBRAL_ALTO ? " svm__disco--lleno" : ""}">
          <span>${esc(d.montaje.replace(/[\\/]+$/, "") || d.montaje)}</span>
          <i><u style="width:${uso.toFixed(0)}%"></u></i>
          <span>${gb(d.libre)} GB</span></div>`;
      })
      .join("");
  };

  const recibir = (m: MuestraSistema) => {
    pintarNucleos(m.cpu_nucleos);
    pintarResumen(m);
  };

  // --- Procesos ---
  const cuerpoProc = q("procesos");
  let procesos: Proceso[] = [];
  let orden: "cpu" | "memoria" = "cpu";
  const pintarProcesos = () => {
    cont.querySelectorAll<HTMLElement>(".svm__orden").forEach((b) => b.classList.toggle("svm__orden--activo", b.dataset.orden === orden));
    const lista = [...procesos].sort((a, b) => b[orden] - a[orden]);
    const maxCpu = Math.max(1, ...lista.map((p) => p.cpu));
    cuerpoProc.innerHTML = lista.length
      ? lista
          .map(
            (p) => `<tr>
              <td class="svm__pn">${esc(p.nombre)}${p.cuantos > 1 ? ` <span class="svm__x">×${p.cuantos}</span>` : ""}</td>
              <td><span class="svm__cpu"><i style="width:${Math.min(100, (p.cpu / maxCpu) * 100).toFixed(0)}%"></i></span>${p.cpu.toFixed(1)}</td>
              <td class="svm__mem">${memoria(p.memoria)}</td></tr>`,
          )
          .join("")
      : `<tr><td colspan="3" class="svm__vacio">SIN DATOS</td></tr>`;
  };
  cont.querySelectorAll<HTMLElement>(".svm__orden").forEach((b) =>
    b.addEventListener("click", () => {
      orden = b.dataset.orden as "cpu" | "memoria";
      pintarProcesos();
    }),
  );
  pintarProcesos();

  let vivo = true;
  const refrescarProcesos = () => {
    // Con la ventana oculta o minimizada no se pide nada
    if (document.visibilityState !== "visible") return;
    procesosSistema()
      .then((p) => {
        if (!vivo) return;
        procesos = p;
        pintarProcesos();
      })
      .catch(() => {});
  };
  const intervalo = setInterval(refrescarProcesos, REFRESCO_PROCESOS_MS);

  // --- Rejilla adaptable y repintado ---
  const observador = new ResizeObserver(() => {
    raiz.classList.toggle("svm--una", raiz.clientWidth < ANCHO_UNA_COLUMNA);
    pedirDibujo();
  });
  observador.observe(raiz);
  // Al volver a ser visible se repinta y se refrescan los procesos sin esperar los 2 s
  const alVisibilidad = () => {
    if (document.visibilityState === "visible") {
      refrescarProcesos();
      pedirDibujo();
    }
  };
  document.addEventListener("visibilitychange", alVisibilidad);

  // --- Datos: carga inicial y muestras en vivo ---
  const suscripciones: (() => void)[] = [];
  alEvento<MuestraSistema>("sistema://muestra", (m) => {
    if (!vivo) return;
    const ram_pct = m.ram_total > 0 ? (m.ram_usada / m.ram_total) * 100 : 0;
    agregarPunto({ t: Date.now(), cpu: m.cpu_total, ram_pct, gpu: m.gpu_uso });
    recibir(m);
  })
    .then((un) => (vivo ? suscripciones.push(un) : un()))
    .catch(() => {});

  historialSistema()
    .then((h) => {
      if (!vivo) return;
      // Las muestras que llegaron por evento durante la carga van detrás de las del historial
      const nuevas = hist.splice(0);
      hist.push(...h, ...nuevas);
      hist.splice(0, Math.max(0, hist.length - MAX_PUNTOS));
      pintarValores();
      pedirDibujo();
    })
    .catch(() => {});
  muestraSistema()
    .then((m) => vivo && recibir(m))
    .catch(() => {});
  refrescarProcesos();

  return () => {
    vivo = false;
    clearInterval(intervalo);
    cancelAnimationFrame(rafHist);
    observador.disconnect();
    document.removeEventListener("visibilitychange", alVisibilidad);
    suscripciones.forEach((u) => u());
    ondas.forEach((o) => o.destruir());
  };
}
