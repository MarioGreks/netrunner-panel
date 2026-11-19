// Apartados de la ventana de CONSUMO (como SECCIONES de la app Consumo): RESUMEN, RITMO, DÍAS,
// PROYECTOS, EQUIPOS, USO e HISTÓRICO. Solo existe en el DOM el apartado activo; al cambiar, el
// anterior se destruye (anillos, lienzos y observadores incluidos).
import type {
  DatosDisco,
  DetalleConsumo,
  DiagnosticoConsumo,
  EstadoConsumo,
  HistorialConsumo,
  SesionContexto,
} from "../../lib/ipc";
import {
  dibujarDias,
  dibujarRitmo,
  dibujarStack,
  crearLienzo,
  leyenda,
  proyeccionSemana,
  type Lienzo,
} from "./graficas";
import {
  COLOR_NIVEL,
  crearBloqueLimite,
  crearListaContexto,
  pintarCuentaAtras,
  pintarLimite,
  plantillaLimite,
  ventanasDe,
} from "./piezas";
import { montarEquipos, montarProyectos, montarUso } from "./reparto";

/** Lo último que llegó del núcleo; lo rellena la página y lo leen los apartados. */
export interface Datos {
  estado: EstadoConsumo | null;
  /** Marca de tiempo (ms) en que llegó `estado`, para la cuenta atrás. */
  recibidoEn: number;
  serie: HistorialConsumo | null;
  detalle: DetalleConsumo | null;
  sesiones: SesionContexto[];
  diag: DiagnosticoConsumo | null;
  disco: DatosDisco | null;
}

/** Qué dato cambió (el apartado repinta solo lo que le toca). */
export type Cambio = "estado" | "serie" | "detalle" | "contexto" | "diag" | "todo";

export interface Apartado {
  pintar(cambio: Cambio): void;
  /** Cuenta atrás (cada 1 s mientras se ve): solo textContent. True si hay que releer el estado. */
  reloj?(): boolean;
  destruir(): void;
}

export interface Entorno {
  datos: Datos;
  /** La ventana se ve (no minimizada ni oculta): si no, nada se pinta. */
  activo(): boolean;
  reducido(): boolean;
}

export interface InfoApartado {
  id: string;
  titulo: string;
  /** null: aún sin contenido («PRÓXIMAMENTE»). */
  montar: ((raiz: HTMLElement, e: Entorno) => Apartado) | null;
}

const MAX_SESIONES = 10;

// --- RESUMEN ---
function montarResumen(raiz: HTMLElement, e: Entorno): Apartado {
  raiz.innerHTML = `
    <div class="cv__col">
      <section class="marco">
        <div class="marco__cabecera"><span>LÍMITES</span><span class="marco__etiqueta">CLAUDE</span></div>
        <div class="marco__cuerpo cv__limites">
          ${plantillaLimite("sem", "SEMANAL")}
          ${plantillaLimite("cinco", "5 H")}
        </div>
      </section>
      <section class="marco marco--cian">
        <div class="marco__cabecera"><span>CONTEXTO</span><span class="marco__etiqueta" data-id="ctx-cuenta">0</span></div>
        <div class="marco__cuerpo">
          <div class="con__sesiones cv__sesiones" data-id="sesiones"></div>
          <p class="marco__vacio con__vacio" data-id="sin-sesiones">SIN SESIONES ACTIVAS</p>
        </div>
      </section>
      <p class="con__aviso-texto cv__aviso" data-id="aviso" hidden></p>
    </div>`;
  const q = <T extends HTMLElement>(id: string) => raiz.querySelector<T>(`[data-id="${id}"]`)!;
  const sem = crearBloqueLimite(raiz, "sem", 200, 60);
  const cinco = crearBloqueLimite(raiz, "cinco", 150, 44);
  const lista = crearListaContexto(q("sesiones"), q("sin-sesiones"), q("ctx-cuenta"), MAX_SESIONES);
  const aviso = q("aviso");
  let entrada = true; // la primera pintura de los anillos arranca desde 0

  const pintarEstado = () => {
    const est = e.datos.estado;
    if (!est) return;
    const animar = entrada && !e.reducido();
    entrada = false;
    pintarLimite(sem, est.semanal, animar, e.reducido());
    pintarLimite(cinco, est.cincoHoras, animar, e.reducido());
  };
  const pintarAviso = () => {
    const d = e.datos.diag;
    const hay = !!e.datos.estado?.sinDatos || (d !== null && d.codigo !== "ok");
    aviso.hidden = !hay;
    if (hay) aviso.textContent = d && d.codigo !== "ok" ? d.texto : "SIN DATOS DE CLAUDE";
  };

  return {
    pintar(c) {
      if (c === "estado" || c === "todo") pintarEstado();
      if (c === "contexto" || c === "todo") lista.pintar(e.datos.sesiones);
      if (c === "estado" || c === "diag" || c === "todo") pintarAviso();
    },
    reloj() {
      const est = e.datos.estado;
      return est ? pintarCuentaAtras(ventanasDe(est, sem, cinco), (Date.now() - e.datos.recibidoEn) / 1000) : false;
    },
    destruir() {
      sem.anillo.destruir();
      cinco.anillo.destruir();
      raiz.replaceChildren();
    },
  };
}

// --- Consumo por hora (se usa en RITMO y en DÍAS) ---
const plantillaStack = (claseMarco: string) => `
  <section class="marco ${claseMarco}">
    <div class="marco__cabecera"><span>CONSUMO POR HORA</span><span class="marco__etiqueta">ACUMULADO · SEMANA ACTUAL</span></div>
    <div class="marco__cuerpo">
      <div class="cv__lienzo cv__lienzo--stack"><canvas data-id="stack"></canvas></div>
      <div class="cv__leyenda" data-id="leyenda"></div>
    </div>
  </section>`;

function montarStack(raiz: HTMLElement, e: Entorno) {
  const canvas = raiz.querySelector<HTMLCanvasElement>('[data-id="stack"]')!;
  const ley = raiz.querySelector<HTMLElement>('[data-id="leyenda"]')!;
  const lienzo = crearLienzo(
    canvas,
    (c, w, h, t) => dibujarStack({ estado: e.datos.estado, horas: e.datos.detalle?.porHora ?? null })(c, w, h, t),
    e.activo,
  );
  return {
    pintar() {
      const horas = e.datos.detalle?.porHora;
      leyenda(
        ley,
        horas ? horas.series.filter((s) => s.valores.some((v) => v > 0)).map((s) => s.nombre) : [],
      );
      lienzo.redibujar();
    },
    redibujar: () => lienzo.redibujar(),
    destruir: () => lienzo.destruir(),
  };
}

// --- RITMO ---
function montarRitmo(raiz: HTMLElement, e: Entorno): Apartado {
  raiz.innerHTML = `
    <div class="cv__col">
      <section class="marco">
        <div class="marco__cabecera"><span>RITMO DE LA SEMANA</span><span class="marco__etiqueta">REAL · ESPERADO</span></div>
        <div class="marco__cuerpo">
          <div class="cv__lienzo cv__lienzo--ritmo"><canvas data-id="ritmo"></canvas></div>
          <div class="cv__textos">
            <b class="cv__cifra" data-id="pct"></b>
            <span data-id="esperado"></span>
            <span class="cv__proy" data-id="proy"></span>
          </div>
        </div>
      </section>
      ${plantillaStack("marco--cian")}
    </div>`;
  const q = <T extends HTMLElement>(id: string) => raiz.querySelector<T>(`[data-id="${id}"]`)!;
  const lienzo: Lienzo = crearLienzo(
    q<HTMLCanvasElement>("ritmo"),
    (c, w, h, t) => dibujarRitmo({ estado: e.datos.estado, serie: e.datos.serie?.semanal ?? null })(c, w, h, t),
    e.activo,
  );
  const stack = montarStack(raiz, e);
  const pct = q("pct");
  const esperado = q("esperado");
  const proy = q("proy");

  const textos = () => {
    const est = e.datos.estado;
    const s = est?.semanal;
    if (!est || !s || !s.hayDatos) {
      pct.textContent = esperado.textContent = proy.textContent = "";
      return;
    }
    pct.textContent = `${Math.round(s.pct)}%`;
    pct.style.color = COLOR_NIVEL[s.nivel];
    esperado.textContent = `DEBERÍA IR POR ${Math.round(s.esperado)}%`;
    const p = proyeccionSemana(est);
    proy.textContent = p === null ? "" : `AL RITMO DE AHORA ACABAS EN ~${Math.round(p)}%`;
    proy.style.color = p !== null && p > 100 ? "var(--rojo)" : COLOR_NIVEL[s.nivel];
  };

  return {
    pintar(c) {
      if (c === "contexto" || c === "diag") return;
      if (c !== "detalle") {
        textos();
        lienzo.redibujar();
      }
      if (c === "detalle" || c === "estado" || c === "todo") stack.pintar();
    },
    destruir() {
      lienzo.destruir();
      stack.destruir();
      raiz.replaceChildren();
    },
  };
}

// --- DÍAS ---
function montarDias(raiz: HTMLElement, e: Entorno): Apartado {
  raiz.innerHTML = `
    <div class="cv__col">
      <section class="marco">
        <div class="marco__cabecera"><span>CONSUMO POR DÍA</span><span class="marco__etiqueta">SEMANA ACTUAL</span></div>
        <div class="marco__cuerpo">
          <div class="cv__lienzo cv__lienzo--dias"><canvas data-id="dias"></canvas></div>
          <div class="cv__textos">
            <b class="cv__cifra" data-id="hoy"></b>
            <span data-id="ayer"></span>
          </div>
        </div>
      </section>
      ${plantillaStack("marco--cian")}
    </div>`;
  const q = <T extends HTMLElement>(id: string) => raiz.querySelector<T>(`[data-id="${id}"]`)!;
  const lienzo = crearLienzo(
    q<HTMLCanvasElement>("dias"),
    (c, w, h, t) => dibujarDias({ dias: e.datos.detalle?.dias ?? null })(c, w, h, t),
    e.activo,
  );
  const stack = montarStack(raiz, e);
  const hoyEl = q("hoy");
  const ayerEl = q("ayer");

  const textos = () => {
    const d = e.datos.detalle?.dias;
    hoyEl.textContent = ayerEl.textContent = "";
    if (!d) return;
    const ahora = new Date();
    const f = `${ahora.getFullYear()}-${String(ahora.getMonth() + 1).padStart(2, "0")}-${String(ahora.getDate()).padStart(2, "0")}`;
    const i = d.fechas.indexOf(f);
    if (i < 0) return;
    hoyEl.textContent = `HOY +${Math.round(d.total[i] ?? 0)}%`;
    if (i >= 1) ayerEl.textContent = `AYER +${Math.round(d.total[i - 1] ?? 0)}%`;
  };

  return {
    pintar(c) {
      if (c === "contexto" || c === "diag") return;
      if (c === "detalle" || c === "todo") {
        textos();
        lienzo.redibujar();
        stack.pintar();
      } else if (c === "estado") {
        stack.redibujar(); // el stack usa el % real y el reset del estado
      }
    },
    destruir() {
      lienzo.destruir();
      stack.destruir();
      raiz.replaceChildren();
    },
  };
}

// --- Pendientes ---
const proximamente = (titulo: string) => (raiz: HTMLElement): Apartado => {
  raiz.innerHTML = `
    <div class="cv__col">
      <section class="marco marco--cian">
        <div class="marco__cabecera"><span>${titulo}</span></div>
        <div class="marco__cuerpo"><p class="marco__vacio cv__proximamente">PRÓXIMAMENTE</p></div>
      </section>
    </div>`;
  return { pintar() {}, destruir: () => raiz.replaceChildren() };
};

export const APARTADOS: InfoApartado[] = [
  { id: "resumen", titulo: "RESUMEN", montar: montarResumen },
  { id: "ritmo", titulo: "RITMO", montar: montarRitmo },
  { id: "dias", titulo: "DÍAS", montar: montarDias },
  { id: "proyectos", titulo: "PROYECTOS", montar: montarProyectos },
  { id: "equipos", titulo: "EQUIPOS", montar: montarEquipos },
  { id: "uso", titulo: "USO", montar: montarUso },
  { id: "historico", titulo: "HISTÓRICO", montar: null },
];

export const montarApartado = (info: InfoApartado, raiz: HTMLElement, e: Entorno): Apartado =>
  info.montar ? info.montar(raiz, e) : proximamente(info.titulo)(raiz);
