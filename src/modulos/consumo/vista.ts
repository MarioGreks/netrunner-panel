// Módulo CONSUMO: resumen del consumo de Claude (límites semanal y de 5 h, contexto de las sesiones vivas).
// La información completa irá en una ventana flotante (abrirVentanaConsumo).
//
// REGLA DE RECURSOS: la vista solo está «visible» si la pestaña está montada Y el panel se ve.
// Mientras no lo está no hay temporizadores ni animaciones, y el hilo de contexto del núcleo
// se para (vistaConsumo(false)). Los oyentes de eventos solo guardan el dato; pintan si hay vista.
import {
  abrirChatConsumo,
  abrirVentanaConsumo,
  alContextoConsumo,
  alDatosConsumo,
  alDiagnosticoConsumo,
  alEstadoConsumo,
  contextoConsumo,
  datosConsumo,
  diagnosticoConsumo,
  estadoConsumo,
  instalarPluginConsumo,
  vistaConsumo,
  type DatosDisco,
  type DiagnosticoConsumo,
  type EstadoConsumo,
  type NivelConsumo,
  type NivelContexto,
  type SesionContexto,
  type VentanaConsumo,
} from "../../lib/ipc";
import { alCambiarVisibilidadPanel, panelVisible } from "../../lib/visibilidad";
import { crearAnillo, type Anillo } from "../../ui/grafica";

/** La ventana flotante aún no existe (encargo 6): el botón ABRIR queda oculto hasta entonces. */
const VENTANA_DISPONIBLE = false;
const MAX_SESIONES = 10;
const REFRESCO_MIN_MS = 30_000; // mínimo entre relecturas del estado al llegar el reset

const COLOR_NIVEL: Record<NivelConsumo, string> = {
  bien: "var(--cian)",
  encima: "var(--amarillo)",
  riesgo: "var(--rojo)",
};

/** Mismo formato que `falta` del núcleo: «1d 1h», «1h 01m», «10m». */
function formatearFalta(s: number): string {
  const t = Math.max(0, Math.floor(s));
  const d = Math.floor(t / 86400);
  const h = Math.floor((t % 86400) / 3600);
  const m = Math.floor((t % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
  return `${m}m`;
}

const tokensCorto = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 100000 ? 0 : 1)}K` : String(n));

interface BloqueLimite {
  anillo: Anillo;
  ritmo: HTMLElement;
  semana: HTMLElement;
  exceso: HTMLElement;
  falta: HTMLElement;
}

const plantillaLimite = (clave: string, titulo: string) => `
  <div class="con__limite" data-id="${clave}">
    <span class="con__sub">${titulo}</span>
    <div class="con__anillo" data-id="${clave}-anillo"></div>
    <span class="con__exceso" data-id="${clave}-exceso" hidden></span>
    <span class="con__ritmo" data-id="${clave}-ritmo"></span>
    <span class="con__semana" data-id="${clave}-semana"></span>
    <span class="con__falta">RESET <b data-id="${clave}-falta">--</b></span>
  </div>`;

export function montar(contenedor: HTMLElement): () => void {
  contenedor.innerHTML = `
    <div class="con">
      <section class="marco">
        <div class="marco__cabecera"><span>LÍMITES</span><span class="marco__etiqueta">CLAUDE</span></div>
        <div class="marco__cuerpo con__limites">
          ${plantillaLimite("sem", "SEMANAL")}
          ${plantillaLimite("cinco", "5 H")}
        </div>
      </section>
      <section class="marco marco--cian">
        <div class="marco__cabecera"><span>CONTEXTO</span><span class="marco__etiqueta" data-id="ctx-cuenta">0</span></div>
        <div class="marco__cuerpo">
          <div class="con__sesiones" data-id="sesiones"></div>
          <p class="marco__vacio con__vacio" data-id="sin-sesiones">SIN SESIONES ACTIVAS</p>
        </div>
      </section>
      <div class="con__aviso" data-id="aviso" hidden>
        <div class="banda"></div>
        <p class="con__aviso-texto" data-id="aviso-texto"></p>
        <div class="con__aviso-acciones">
          <button class="fm__btn fm__btn--ok" data-id="instalar" hidden>INSTALAR PLUGIN</button>
        </div>
        <p class="con__aviso-resultado" data-id="aviso-resultado" hidden></p>
        <div class="banda"></div>
      </div>
      <div class="con__pie">
        <button class="fm__btn" data-id="abrir" hidden>ABRIR</button>
        <span class="con__datos" data-id="datos"></span>
      </div>
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const movimientoReducido = window.matchMedia("(prefers-reduced-motion: reduce)");

  const crearBloque = (clave: string, tamano: number, segmentos: number): BloqueLimite => {
    const anillo = crearAnillo(q(`${clave}-anillo`), { color: "var(--cian)", tamano, segmentos });
    anillo.fijar(null, true);
    return {
      anillo,
      ritmo: q(`${clave}-ritmo`),
      semana: q(`${clave}-semana`),
      exceso: q(`${clave}-exceso`),
      falta: q(`${clave}-falta`),
    };
  };
  const bloqueSem = crearBloque("sem", 132, 48);
  const bloqueCinco = crearBloque("cinco", 96, 32);

  const avisoEl = q("aviso");
  const avisoTexto = q("aviso-texto");
  const avisoResultado = q("aviso-resultado");
  const botonInstalar = q<HTMLButtonElement>("instalar");
  const botonAbrir = q<HTMLButtonElement>("abrir");
  const datosEl = q("datos");
  const sesionesEl = q("sesiones");
  const sinSesiones = q("sin-sesiones");
  const cuentaCtx = q("ctx-cuenta");

  if (VENTANA_DISPONIBLE) {
    botonAbrir.hidden = false;
    botonAbrir.addEventListener("click", () => void abrirVentanaConsumo().catch(() => {}));
  }

  // --- Estado ---
  let montada = true;
  let visible = false;
  let estado: EstadoConsumo | null = null;
  let recibidoEn = 0; // marca de tiempo (ms) en que llegó `estado`, para la cuenta atrás
  let sesiones: SesionContexto[] = [];
  let diag: DiagnosticoConsumo | null = null;
  let diagPedido = false;
  let datos: DatosDisco | null = null;
  let ultimoRefresco = 0;
  let temporizador: number | undefined;
  let entrada = false; // la próxima pintura de anillos arranca desde 0 (animación de entrada)

  // --- Límites ---
  const pintarVentana = (b: BloqueLimite, v: VentanaConsumo, animar: boolean) => {
    const color = COLOR_NIVEL[v.nivel];
    const sinDato = !v.hayDatos;
    b.anillo.colorAnillo(color);
    b.anillo.colorValor(color);
    if (animar) b.anillo.fijar(0, true);
    b.anillo.fijar(sinDato ? null : v.pct, movimientoReducido.matches);
    b.anillo.marca(sinDato ? null : v.esperado);
    b.anillo.tramo(sinDato || v.fin === null ? null : v.fin);
    b.ritmo.textContent = sinDato ? "SIN DATOS" : (v.textoRitmo ?? "");
    b.ritmo.classList.toggle("con__ritmo--encima", v.nivel === "encima");
    b.ritmo.classList.toggle("con__ritmo--riesgo", v.nivel === "riesgo");
    b.semana.textContent = sinDato ? "" : (v.textoSemana ?? v.textoSinLimite ?? "");
    b.exceso.hidden = sinDato || v.exceso <= 0;
    if (!b.exceso.hidden) b.exceso.textContent = `▲ EXCESO +${Math.round(v.exceso)}%`;
    b.falta.textContent = v.textoFalta;
  };

  const pintarEstado = () => {
    if (!estado) return;
    // Con prefers-reduced-motion los anillos se colocan directamente, sin entrada
    const animar = entrada && !movimientoReducido.matches;
    entrada = false;
    pintarVentana(bloqueSem, estado.semanal, animar);
    pintarVentana(bloqueCinco, estado.cincoHoras, animar);
    pintarAviso();
  };

  // Reloj: solo cambia el textContent de la cuenta atrás (1 s, solo con la vista visible)
  const pintarReloj = () => {
    if (!estado) return;
    const pasado = (Date.now() - recibidoEn) / 1000;
    let vencido = false;
    for (const [b, v] of [
      [bloqueSem, estado.semanal],
      [bloqueCinco, estado.cincoHoras],
    ] as const) {
      if (v.segundosHastaReset === null) continue;
      const falta = v.segundosHastaReset - pasado;
      if (falta <= 0) vencido = true;
      const t = formatearFalta(falta);
      if (b.falta.textContent !== t) b.falta.textContent = t;
    }
    // Al llegar un reset, el estado cambia: se relee (como mucho cada 30 s)
    if (vencido && Date.now() - ultimoRefresco > REFRESCO_MIN_MS) refrescarEstado();
  };

  const refrescarEstado = () => {
    ultimoRefresco = Date.now();
    estadoConsumo().then(recibirEstado).catch(() => {});
  };

  const recibirEstado = (e: EstadoConsumo) => {
    if (!montada) return;
    estado = e;
    recibidoEn = Date.now();
    if (visible) {
      pintarEstado();
      pedirDiagnostico();
    }
  };

  // --- Contexto ---
  interface Pastilla {
    boton: HTMLButtonElement;
    relleno: HTMLElement;
    nombre: HTMLElement;
    tokens: HTMLElement;
  }
  const pastillas = new Map<string, Pastilla>();
  const clasesNivel: NivelContexto[] = ["bien", "medio", "alto"];

  const pintarContexto = () => {
    const lista = sesiones.slice(0, MAX_SESIONES);
    cuentaCtx.textContent = String(lista.length);
    sinSesiones.hidden = lista.length > 0;
    sesionesEl.hidden = lista.length === 0;
    const ids = lista.map((s) => s.id);
    // Se quitan las que ya no están y se crean las nuevas; el resto se actualiza en sitio
    for (const [id, p] of pastillas) {
      if (!ids.includes(id)) {
        p.boton.remove();
        pastillas.delete(id);
      }
    }
    lista.forEach((s, i) => {
      let p = pastillas.get(s.id);
      if (!p) {
        const boton = document.createElement("button");
        boton.className = "ctx";
        boton.innerHTML = `
          <span class="ctx__barra"><i class="ctx__relleno"></i></span>
          <span class="ctx__nombre"></span>
          <span class="ctx__tokens"></span>`;
        const id = s.id;
        boton.addEventListener("click", () => {
          if (boton.classList.contains("ctx--chat")) void abrirChatConsumo(id).catch(() => {});
        });
        p = {
          boton,
          relleno: boton.querySelector<HTMLElement>(".ctx__relleno")!,
          nombre: boton.querySelector<HTMLElement>(".ctx__nombre")!,
          tokens: boton.querySelector<HTMLElement>(".ctx__tokens")!,
        };
        pastillas.set(s.id, p);
      }
      const pastilla = p;
      if (sesionesEl.children[i] !== pastilla.boton) sesionesEl.insertBefore(pastilla.boton, sesionesEl.children[i] ?? null);
      pastilla.relleno.style.height = `${Math.round(Math.min(1, Math.max(0, s.fraccion)) * 100)}%`;
      clasesNivel.forEach((n) => pastilla.boton.classList.toggle(`ctx--${n}`, s.nivel === n));
      pastilla.boton.classList.toggle("ctx--chat", s.tieneChat);
      const titulo = `${s.nombre}\n${s.proyecto} · ${s.tokens.toLocaleString("es-ES")} tokens · ${Math.round(s.fraccion * 100)} %`;
      if (pastilla.boton.title !== titulo) pastilla.boton.title = titulo;
      if (pastilla.nombre.textContent !== s.nombre) pastilla.nombre.textContent = s.nombre;
      pastilla.tokens.textContent = tokensCorto(s.tokens);
    });
  };

  const recibirContexto = (lista: SesionContexto[]) => {
    if (!montada) return;
    sesiones = lista;
    if (visible) pintarContexto();
  };

  // --- Aviso / diagnóstico ---
  const pintarAviso = () => {
    const hayAviso = !!estado?.sinDatos || (diag !== null && diag.codigo !== "ok");
    avisoEl.hidden = !hayAviso;
    if (!hayAviso) return;
    avisoTexto.textContent = diag && diag.codigo !== "ok" ? diag.texto : "SIN DATOS DE CLAUDE";
    botonInstalar.hidden = !(diag?.puedeInstalar && diag.codigo !== "ok");
  };

  const pedirDiagnostico = () => {
    if (diagPedido || !estado?.sinDatos) return;
    diagPedido = true;
    diagnosticoConsumo()
      .then((d) => {
        if (!montada) return;
        diag = d;
        if (visible) pintarAviso();
      })
      .catch(() => {});
  };

  botonInstalar.addEventListener("click", () => {
    botonInstalar.disabled = true;
    avisoResultado.hidden = false;
    avisoResultado.textContent = "INSTALANDO…";
    instalarPluginConsumo()
      .then((t) => {
        if (montada) avisoResultado.textContent = t;
      })
      .catch((e) => {
        if (montada) avisoResultado.textContent = String(e);
      })
      .finally(() => {
        if (montada) botonInstalar.disabled = false;
      });
  });

  // --- Datos en disco ---
  const pintarDatos = () => {
    datosEl.textContent = datos?.texto ?? "";
    datosEl.classList.toggle("con__datos--aviso", datos?.nivel === "aviso");
    datosEl.classList.toggle("con__datos--lleno", datos?.nivel === "lleno");
  };
  const recibirDatos = (d: DatosDisco) => {
    if (!montada) return;
    datos = d;
    if (visible) pintarDatos();
  };

  // --- Visibilidad: pestaña montada Y panel visible ---
  const aplicarVisible = (v: boolean) => {
    if (v === visible) return;
    visible = v;
    void vistaConsumo(v).catch(() => {});
    if (!v) {
      clearInterval(temporizador);
      temporizador = undefined;
      return;
    }
    entrada = true;
    pintarEstado();
    pintarContexto();
    pintarDatos();
    pintarReloj();
    temporizador = window.setInterval(pintarReloj, 1000);
    // Datos frescos tras el tiempo oculto
    refrescarEstado();
    contextoConsumo().then(recibirContexto).catch(() => {});
    datosConsumo().then(recibirDatos).catch(() => {});
    pedirDiagnostico();
  };

  // --- Oyentes (se quitan en la limpieza) ---
  const desuscribir = [
    alEstadoConsumo(recibirEstado),
    alContextoConsumo(recibirContexto),
    alDiagnosticoConsumo((d) => {
      if (!montada) return;
      diag = d;
      if (visible) pintarAviso();
    }),
    alDatosConsumo(recibirDatos),
  ];
  const quitarVisibilidad = alCambiarVisibilidadPanel((v) => aplicarVisible(v && montada));

  pintarContexto();
  aplicarVisible(panelVisible());

  return () => {
    montada = false;
    quitarVisibilidad();
    desuscribir.forEach((p) => void p.then((f) => f()).catch(() => {}));
    aplicarVisible(false);
    bloqueSem.anillo.destruir();
    bloqueCinco.anillo.destruir();
  };
}
