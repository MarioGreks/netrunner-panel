// Widget CONSUMO: los dos límites de Claude (SEMANAL y 5 H) en anillos con el % en el centro, la marca
// del ritmo esperado y la cuenta atrás hasta su reset. Solo usa el estado de límites (no lee
// transcripciones). Los datos llegan por `consumo://estado`; la cuenta atrás se repinta cada 60 s
// y solo con el widget a la vista (al volver, al instante).
import { abrirVentanaConsumo, alEstadoConsumo, estadoConsumo, type EstadoConsumo, type VentanaConsumo } from "../lib/ipc";
import { crearAnillo, type Anillo } from "../ui/grafica";
import { formatearFalta } from "../modulos/consumo/piezas";
import type { ContextoWidget } from "../widget";
import { escuchas, prepararCuerpo } from "./comun";

const CADA_MS = 60000;

interface Bloque {
  anillo: Anillo;
  falta: HTMLElement;
  txt: HTMLElement;
}

const bloque = (clave: string, titulo: string) => `
  <div class="co__lim" data-id="${clave}">
    <span class="co__tit">${titulo}</span>
    <div class="co__anillo" data-id="${clave}-anillo"></div>
    <span class="co__txt" data-id="${clave}-txt"></span>
    <span class="co__falta" data-id="${clave}-falta"></span>
  </div>`;

export function montarConsumo(cuerpo: HTMLElement, ctx: ContextoWidget): () => void {
  prepararCuerpo(cuerpo, "wgc--consumo");
  cuerpo.innerHTML = `
    <div class="co">
      ${bloque("semanal", "SEMANAL")}
      ${bloque("cinco", "5 H")}
      <p class="co__nd" hidden>SIN DATOS DE CLAUDE</p>
    </div>`;
  const raiz = cuerpo.querySelector<HTMLElement>(".co")!;
  const nd = raiz.querySelector<HTMLElement>(".co__nd")!;
  const crear = (clave: string): Bloque => {
    const q = (id: string) => raiz.querySelector<HTMLElement>(`[data-id="${clave}-${id}"]`)!;
    const anillo = crearAnillo(q("anillo"), { color: "var(--cian)", tamano: 96, segmentos: 40 });
    anillo.fijar(null, true);
    return { anillo, falta: q("falta"), txt: q("txt") };
  };
  const sem = crear("semanal");
  const cinco = crear("cinco");

  let estado: EstadoConsumo | null = null;
  let recibido = Date.now();
  let temporizador: number | undefined;
  let releyendo = false;

  const parar = () => {
    clearTimeout(temporizador);
    temporizador = undefined;
  };

  const pintarBloque = (b: Bloque, v: VentanaConsumo, sinDatos: boolean) => {
    const sinDato = sinDatos || !v.hayDatos;
    // Por delante del ritmo (el panel lo llama «encima» o «riesgo»): rojo de alerta
    const alerta = !sinDato && v.nivel !== "bien";
    b.anillo.colorAnillo(alerta ? "var(--rojo)" : "var(--cian)");
    b.anillo.colorValor(alerta ? "var(--rojo)" : null);
    b.anillo.fijar(sinDato ? null : v.pct, true);
    b.anillo.marca(sinDato ? null : v.esperado);
    b.txt.textContent = sinDato ? "N/D" : `${Math.round(v.pct)}%`;
    b.txt.classList.toggle("co__txt--alerta", alerta);
    if (v.segundosHastaReset === null || sinDato) {
      b.falta.textContent = "";
      return;
    }
    const falta = v.segundosHastaReset - (Date.now() - recibido) / 1000;
    b.falta.textContent = `RESET ${formatearFalta(falta).toUpperCase()}`;
    if (falta <= 0 && !releyendo && ctx.visible()) {
      // Ya llegó el reset: se relee el estado (una a la vez)
      releyendo = true;
      estadoConsumo()
        .then((e) => recibir(e))
        .catch(() => {})
        .finally(() => (releyendo = false));
    }
  };

  const pintar = () => {
    parar();
    if (!estado) return;
    nd.hidden = !estado.sinDatos;
    raiz.classList.toggle("co--nd", estado.sinDatos);
    pintarBloque(sem, estado.semanal, estado.sinDatos);
    pintarBloque(cinco, estado.cincoHoras, estado.sinDatos);
    if (ctx.visible() && !estado.sinDatos) temporizador = window.setTimeout(pintar, CADA_MS);
  };

  function recibir(e: EstadoConsumo) {
    estado = e;
    recibido = Date.now();
    if (ctx.visible()) pintar();
  }

  ctx.alVisibilidad((v) => (v ? pintar() : parar()));

  const escucha = escuchas();
  escucha.anadir(alEstadoConsumo(recibir));
  estadoConsumo().then(recibir).catch(() => {});

  const alClic = () => void abrirVentanaConsumo().catch(() => {});
  cuerpo.addEventListener("click", alClic);

  return () => {
    parar();
    escucha.limpiar();
    cuerpo.removeEventListener("click", alClic);
    sem.anillo.destruir();
    cinco.anillo.destruir();
  };
}
