// Módulo CONSUMO: resumen del consumo de Claude (límites semanal y de 5 h, contexto de las sesiones vivas).
// La información completa irá en una ventana flotante (abrirVentanaConsumo).
//
// REGLA DE RECURSOS: la vista solo está «visible» si la pestaña está montada Y el panel se ve.
// Mientras no lo está no hay temporizadores ni animaciones, y el hilo de contexto del núcleo
// se para (vistaConsumo(false)). Los oyentes de eventos solo guardan el dato; pintan si hay vista.
import {
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
  type SesionContexto,
} from "../../lib/ipc";
import { abrirAjustes } from "../../lib/navegacion";
import { alCambiarVisibilidadPanel, panelVisible } from "../../lib/visibilidad";
import {
  crearBloqueLimite,
  crearListaContexto,
  pintarCuentaAtras,
  pintarLimite,
  plantillaLimite,
  ventanasDe,
} from "./piezas";

const MAX_SESIONES = 10;
const REFRESCO_MIN_MS = 30_000; // mínimo entre relecturas del estado al llegar el reset

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
          <button class="fm__btn" data-id="ajustes" type="button">AJUSTES</button>
          <button class="fm__btn fm__btn--ok" data-id="instalar" hidden>INSTALAR PLUGIN</button>
        </div>
        <p class="con__aviso-resultado" data-id="aviso-resultado" hidden></p>
        <div class="banda"></div>
      </div>
      <div class="con__pie">
        <button class="fm__btn" data-id="abrir">ABRIR</button>
        <span class="con__datos" data-id="datos"></span>
      </div>
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const movimientoReducido = window.matchMedia("(prefers-reduced-motion: reduce)");

  const bloqueSem = crearBloqueLimite(contenedor, "sem", 132, 48);
  const bloqueCinco = crearBloqueLimite(contenedor, "cinco", 96, 32);

  const avisoEl = q("aviso");
  const avisoTexto = q("aviso-texto");
  const avisoResultado = q("aviso-resultado");
  const botonInstalar = q<HTMLButtonElement>("instalar");
  q("ajustes").addEventListener("click", () => abrirAjustes("consumo"));
  const botonAbrir = q<HTMLButtonElement>("abrir");
  const datosEl = q("datos");
  const sesionesEl = q("sesiones");
  const sinSesiones = q("sin-sesiones");
  const listaCtx = crearListaContexto(sesionesEl, sinSesiones, q("ctx-cuenta"), MAX_SESIONES);

  botonAbrir.hidden = false;
  botonAbrir.addEventListener("click", () => void abrirVentanaConsumo().catch(() => {}));

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
  const pintarEstado = () => {
    if (!estado) return;
    // Con prefers-reduced-motion los anillos se colocan directamente, sin entrada
    const animar = entrada && !movimientoReducido.matches;
    entrada = false;
    pintarLimite(bloqueSem, estado.semanal, animar, movimientoReducido.matches);
    pintarLimite(bloqueCinco, estado.cincoHoras, animar, movimientoReducido.matches);
    pintarAviso();
  };

  // Reloj: solo cambia el textContent de la cuenta atrás (1 s, solo con la vista visible)
  const pintarReloj = () => {
    if (!estado) return;
    const vencido = pintarCuentaAtras(ventanasDe(estado, bloqueSem, bloqueCinco), (Date.now() - recibidoEn) / 1000);
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
  const pintarContexto = () => listaCtx.pintar(sesiones);

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
    void vistaConsumo("pestana", v).catch(() => {});
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
