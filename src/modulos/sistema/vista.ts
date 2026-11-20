// Módulo SISTEMA: CPU, memoria, GPU, discos y batería en vivo.
import { alEvento, muestraSistema, type MuestraSistema } from "../../lib/ipc";
import { crearAnillo, crearOnda } from "../../ui/grafica";
import { hacerDesacoplable } from "../../ui/desacoplar";

export const UMBRAL_ALTO = 90; // por encima, el valor se pinta en rojo
const SEGMENTOS_DISCO = 24;
const GB = 1024 ** 3;

export const gb = (bytes: number) => (bytes / GB).toFixed(1);

const marco = (titulo: string, etiqueta: string, cuerpo: string, extra = "") => `
  <section class="marco ${extra}">
    <div class="marco__cabecera"><span>${titulo}</span><span class="marco__etiqueta">${etiqueta}</span></div>
    <div class="marco__cuerpo">${cuerpo}</div>
  </section>`;

export function montar(contenedor: HTMLElement): void {
  contenedor.innerHTML = `
    <div class="sis">
      ${marco(
        "CPU",
        "CPU_01",
        `<div class="sis__fila">
           <span class="sis__grande" data-id="cpu-valor">--%</span>
           <canvas class="sis__onda" data-id="cpu-onda"></canvas>
         </div>
         <div class="sis__nucleos" data-id="cpu-nucleos"></div>`,
      )}
      ${marco(
        "MEMORIA",
        "MEM_01",
        `<div class="sis__fila sis__fila--centro">
           <div data-id="ram-anillo"></div>
           <div class="sis__texto-ram">
             <span class="sis__sub">EN USO</span>
             <span class="sis__dato" data-id="ram-texto">-- / -- GB</span>
           </div>
         </div>`,
        "marco--cian",
      )}
      ${marco(
        "GPU",
        "GPU_01",
        `<div class="sis__fila sis__fila--centro" data-id="gpu-cuerpo">
           <div data-id="gpu-anillo"></div>
           <div class="sis__columna">
             <canvas class="sis__onda sis__onda--baja" data-id="gpu-onda"></canvas>
             <span class="sis__sub">TEMP <span class="sis__dato" data-id="gpu-temp">N/D</span></span>
           </div>
         </div>`,
      )}
      ${marco("DISCOS", "DSK", `<div class="sis__discos" data-id="discos"></div>`, "marco--cian")}
      <div data-id="bateria-marco" hidden>
        ${marco(
          "BATERÍA",
          "BAT_01",
          `<div class="sis__fila sis__fila--centro">
             <span class="sis__grande sis__grande--s" data-id="bat-valor">--%</span>
             <span class="sis__bat-icono" data-id="bat-icono" aria-hidden="true">
               <svg viewBox="0 0 24 24"><path d="M3 8h15v8H3zM18 11h3v2h-3z"/><path class="sis__rayo" d="M11 9l-2.5 4H12l-1.5 3"/></svg>
             </span>
             <span class="sis__sub" data-id="bat-tiempo"></span>
           </div>`,
        )}
      </div>
    </div>`;
  // Cualquiera de las tarjetas lleva a la ventana de SISTEMA
  contenedor.querySelectorAll<HTMLElement>(".marco").forEach((t) => {
    hacerDesacoplable(t, { tipo: "sistema", titulo: t.querySelector(".marco__cabecera span")?.textContent ?? "SISTEMA" });
  });

  const q = <T extends HTMLElement>(id: string) =>
    contenedor.querySelector<T>(`[data-id="${id}"]`)!;

  const cpuValor = q("cpu-valor");
  const cpuNucleos = q("cpu-nucleos");
  const ramTexto = q("ram-texto");
  const gpuCuerpo = q("gpu-cuerpo");
  const gpuTemp = q("gpu-temp");
  const discos = q("discos");
  const bateriaMarco = q("bateria-marco");
  const batValor = q("bat-valor");
  const batIcono = q("bat-icono");
  const batTiempo = q("bat-tiempo");

  const ondaCpu = crearOnda(q<HTMLCanvasElement>("cpu-onda"), { color: "var(--amarillo)", max: 100 });
  const ondaGpu = crearOnda(q<HTMLCanvasElement>("gpu-onda"), { color: "var(--cian)", max: 100 });
  const anilloRam = crearAnillo(q("ram-anillo"), { color: "var(--cian)" });
  const anilloGpu = crearAnillo(q("gpu-anillo"), { color: "var(--cian)" });
  anilloRam.fijar(0);
  anilloGpu.fijar(null);
  gpuCuerpo.classList.add("sis--nd");

  let minibarras: HTMLElement[] = [];
  let filasDisco: { barra: HTMLElement; segs: HTMLElement[]; texto: HTMLElement }[] = [];
  let claveDiscos = "";

  const pintarNucleos = (valores: number[]) => {
    if (minibarras.length !== valores.length) {
      cpuNucleos.replaceChildren(
        ...valores.map(() => {
          const b = document.createElement("div");
          b.className = "minibarra";
          b.appendChild(document.createElement("i"));
          return b;
        }),
      );
      minibarras = Array.from(cpuNucleos.children) as HTMLElement[];
    }
    valores.forEach((v, i) => {
      const relleno = minibarras[i].firstElementChild as HTMLElement;
      relleno.style.height = `${Math.min(100, Math.max(0, v))}%`;
      minibarras[i].classList.toggle("minibarra--alta", v > UMBRAL_ALTO);
    });
  };

  const pintarDiscos = (lista: MuestraSistema["discos"]) => {
    const clave = lista.map((d) => d.montaje).join("|");
    if (clave !== claveDiscos) {
      claveDiscos = clave;
      discos.replaceChildren();
      filasDisco = lista.map((d) => {
        const fila = document.createElement("div");
        fila.className = "disco";
        const letra = d.montaje.replace(/[\\/]+$/, "") || d.montaje;
        fila.innerHTML = `<span class="disco__letra">${letra}</span><div class="disco__barra"></div><span class="disco__texto"></span>`;
        const barra = fila.querySelector<HTMLElement>(".disco__barra")!;
        const segs = Array.from({ length: SEGMENTOS_DISCO }, () => {
          const s = document.createElement("i");
          barra.appendChild(s);
          return s;
        });
        discos.appendChild(fila);
        return { barra, segs, texto: fila.querySelector<HTMLElement>(".disco__texto")! };
      });
    }
    lista.forEach((d, i) => {
      const f = filasDisco[i];
      const usado = d.total > 0 ? (d.total - d.libre) / d.total : 0;
      const encendidos = Math.round(usado * SEGMENTOS_DISCO);
      f.segs.forEach((s, k) => s.classList.toggle("on", k < encendidos));
      f.barra.classList.toggle("disco__barra--llena", usado * 100 > UMBRAL_ALTO);
      f.texto.textContent = `${gb(d.libre)} GB libres de ${gb(d.total)} GB`;
    });
  };

  const pintar = (m: MuestraSistema) => {
    // CPU
    cpuValor.textContent = `${Math.round(m.cpu_total)}%`;
    cpuValor.classList.toggle("sis--alto", m.cpu_total > UMBRAL_ALTO);
    ondaCpu.agregar(m.cpu_total);
    pintarNucleos(m.cpu_nucleos);

    // Memoria
    const pctRam = m.ram_total > 0 ? (m.ram_usada / m.ram_total) * 100 : 0;
    anilloRam.fijar(pctRam);
    anilloRam.colorValor(pctRam > UMBRAL_ALTO ? "var(--rojo)" : null);
    ramTexto.textContent = `${gb(m.ram_usada)} / ${gb(m.ram_total)} GB`;

    // GPU
    anilloGpu.fijar(m.gpu_uso);
    gpuCuerpo.classList.toggle("sis--nd", m.gpu_uso === null);
    if (m.gpu_uso !== null) ondaGpu.agregar(m.gpu_uso);
    gpuTemp.textContent = m.gpu_temp === null ? "N/D" : `${Math.round(m.gpu_temp)} °C`;

    // Discos
    pintarDiscos(m.discos);

    // Batería
    bateriaMarco.hidden = m.bateria === null;
    if (m.bateria) {
      batValor.textContent = `${Math.round(m.bateria.porcentaje)}%`;
      batIcono.classList.toggle("sis__bat-icono--carga", m.bateria.cargando);
      const min = m.bateria.minutos_restantes;
      const hm = min === null ? "" : `${Math.floor(min / 60)} h ${String(min % 60).padStart(2, "0")} min`;
      batTiempo.textContent = m.bateria.cargando
        ? min === null ? "CARGANDO" : `CARGA COMPLETA EN ${hm}`
        : min === null ? "" : `QUEDAN ${hm}`;
    }
  };

  // Limpieza al cambiar de pestaña: el contenedor se vacía y deja de estar en el documento
  let desuscribir: (() => void) | null = null;
  let vivo = true;
  const limpiar = () => {
    if (!vivo) return;
    vivo = false;
    ondaCpu.destruir();
    ondaGpu.destruir();
    anilloRam.destruir();
    anilloGpu.destruir();
    desuscribir?.();
  };
  const recibir = (m: MuestraSistema) => {
    if (!contenedor.isConnected || !contenedor.contains(cpuValor)) {
      limpiar();
      return;
    }
    pintar(m);
  };

  const pedir = () => muestraSistema().then(recibir).catch(() => {});

  const suscripciones: (() => void)[] = [];
  desuscribir = () => suscripciones.forEach((u) => u());
  const suscribir = (p: Promise<() => void>) =>
    p.then((un) => {
      if (vivo) suscripciones.push(un);
      else un();
    });
  suscribir(alEvento<MuestraSistema>("sistema://muestra", recibir));
  // Al reabrirse el panel se pide una muestra al momento, sin esperar al siguiente segundo
  suscribir(
    alEvento("panel://mostrar", () => {
      if (vivo) void pedir();
    }),
  );
  void pedir();
}
