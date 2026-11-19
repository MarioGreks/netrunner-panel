// Ventana flotante de CONSUMO (consumo.html): barra de título HUD propia, barra de apartados y el
// apartado activo. El núcleo la crea y la destruye; aquí solo se enseña lo que llega.
//
// REGLA DE RECURSOS
// - Minimizada, tapada u oculta («activa» = false): origen "ventana" apagado en el núcleo, sin
//   intervalo y sin repintados (los lienzos comprueban `activo()` antes de pintar).
// - Solo hay un temporizador: la cuenta atrás de 1 s, y solo con la ventana activa y un apartado
//   que la use. Los repintados ocurren al llegar datos (consumo://estado, consumo://detalle…) o
//   al redimensionar (un fotograma como mucho).
// - Solo existe en el DOM el apartado activo. La guía (módulo cargado al abrirla) y el menú de
//   INFORMES no existen en el DOM hasta que se abren. Los informes solo se generan a petición.
import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/hud.css";
import "./styles/formularios.css";
import "./styles/consumo.css";
import "./styles/consumo-ventana.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { iniciarTema } from "./lib/tema";
import {
  abrirAjustesConsumo,
  alArchivoConsumo,
  alContextoConsumo,
  alDatosConsumo,
  alDetalleConsumo,
  alDiagnosticoConsumo,
  alEstadoConsumo,
  alVentanaConsumoVisible,
  contextoConsumo,
  datosConsumo,
  detalleConsumo,
  diagnosticoConsumo,
  equipoConsumo,
  estadoConsumo,
  generarHistoricoConsumo,
  generarInformeConsumo,
  serieConsumo,
  vistaConsumo,
  type TipoInformeConsumo,
} from "./lib/ipc";
import { APARTADOS, montarApartado, type Apartado, type Cambio, type Datos, type Entorno } from "./modulos/consumo/apartados";
import type { Guia } from "./modulos/consumo/guia";

iniciarTema();

const REFRESCO_MIN_MS = 30_000; // mínimo entre relecturas del estado al llegar un reset
const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const main = q("main");
const nav = q("nav");
const datosEl = q("datos");
const estadoEl = q("estado");

const datos: Datos = { estado: null, recibidoEn: 0, serie: null, detalle: null, sesiones: [], diag: null, disco: null, equipo: "" };
const movimientoReducido = window.matchMedia("(prefers-reduced-motion: reduce)");

let minimizada = false;
let activa = false;
let apartado: Apartado | null = null;
let guia: Guia | null = null;
let cargandoGuia = false;
let indice = 0;
let temporizador: number | undefined;
let ultimoRefresco = 0;

const entorno: Entorno = {
  datos,
  activo: () => activa,
  reducido: () => movimientoReducido.matches,
  abrirAjustes: () => void abrirAjustesConsumo().catch(() => {}),
  mensaje: (t) => mostrarMensaje(t),
};
const esVisible = () => document.visibilityState === "visible" && !minimizada;

// --- Línea de estado del pie: un texto tenue que se borra solo a los pocos segundos ---
const MENSAJE_MS = 8000;
let temporizadorMensaje: number | undefined;
function mostrarMensaje(texto: string) {
  estadoEl.textContent = texto;
  estadoEl.title = texto;
  clearTimeout(temporizadorMensaje);
  temporizadorMensaje = undefined;
  if (texto && !texto.endsWith("…")) {
    temporizadorMensaje = window.setTimeout(() => {
      estadoEl.textContent = "";
      estadoEl.title = "";
      temporizadorMensaje = undefined;
    }, MENSAJE_MS);
  }
}

// --- Datos ---
const pintar = (c: Cambio) => {
  if (activa) apartado?.pintar(c);
};

const cargarSerie = () =>
  serieConsumo()
    .then((s) => {
      datos.serie = s;
      pintar("serie");
    })
    .catch(() => {});

const recibirEstado = (e: typeof datos.estado) => {
  datos.estado = e;
  datos.recibidoEn = Date.now();
  if (!activa) return;
  pintar("estado");
  if (!guia && APARTADOS[indice].id === "ritmo") void cargarSerie();
  if (e?.sinDatos && !datos.diag) void diagnosticoConsumo().then(recibirDiag).catch(() => {});
};
const recibirDiag = (d: typeof datos.diag) => {
  datos.diag = d;
  pintar("diag");
};
const recibirDetalle = () =>
  detalleConsumo()
    .then((d) => {
      datos.detalle = d;
      pintar("detalle");
    })
    .catch(() => {});

const refrescarEstado = () => {
  ultimoRefresco = Date.now();
  estadoConsumo().then(recibirEstado).catch(() => {});
};

const pintarDisco = () => {
  datosEl.textContent = datos.disco?.texto ?? "";
  datosEl.className = datos.disco?.nivel === "aviso" ? "cv__datos--aviso" : datos.disco?.nivel === "lleno" ? "cv__datos--lleno" : "";
};

// --- Apartados ---
const reloj = () => {
  if (apartado?.reloj?.() && Date.now() - ultimoRefresco > REFRESCO_MIN_MS) refrescarEstado();
};

/** El temporizador de 1 s solo existe con la ventana activa y un apartado con cuenta atrás. */
const ajustarTemporizador = () => {
  const debe = activa && !!apartado?.reloj;
  if (debe && temporizador === undefined) temporizador = window.setInterval(reloj, 1000);
  else if (!debe && temporizador !== undefined) {
    clearInterval(temporizador);
    temporizador = undefined;
  }
};

const mostrar = (i: number) => {
  cerrarGuiaSinVolver();
  indice = i;
  apartado?.destruir();
  apartado = null;
  main.scrollTop = 0;
  nav.querySelectorAll<HTMLButtonElement>(".cv__tab").forEach((b, k) => {
    b.classList.toggle("cv__tab--activa", k === i);
    b.setAttribute("aria-selected", String(k === i));
  });
  apartado = montarApartado(APARTADOS[i], main, entorno);
  if (activa) {
    apartado.pintar("todo");
    if (APARTADOS[i].id === "ritmo") void cargarSerie();
    reloj();
  }
  ajustarTemporizador();
};

APARTADOS.forEach((a, i) => {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "cv__tab";
  b.setAttribute("role", "tab");
  b.textContent = a.titulo;
  b.addEventListener("click", () => (guia || i !== indice) && mostrar(i));
  nav.appendChild(b);
});

// --- Activa: la ventana se ve (ni minimizada ni oculta) ---
const aplicarActiva = () => {
  const v = esVisible();
  if (v === activa) return;
  activa = v;
  if (!v) cerrarMenu();
  void vistaConsumo("ventana", v).catch(() => {});
  if (!v) {
    ajustarTemporizador();
    return;
  }
  // Datos frescos tras el tiempo parado; los eventos solo guardan el dato mientras no se ve
  refrescarEstado();
  void contextoConsumo().then((s) => {
    datos.sesiones = s;
    pintar("contexto");
  }).catch(() => {});
  void recibirDetalle();
  void datosConsumo().then((d) => {
    datos.disco = d;
    pintarDisco();
  }).catch(() => {});
  apartado?.pintar("todo");
  if (!guia && APARTADOS[indice].id === "ritmo") void cargarSerie();
  ajustarTemporizador();
};

document.addEventListener("visibilitychange", aplicarActiva);
void alVentanaConsumoVisible((v) => {
  minimizada = !v;
  aplicarActiva();
});
void alEstadoConsumo(recibirEstado);
void alContextoConsumo((s) => {
  datos.sesiones = s;
  pintar("contexto");
});
void alDetalleConsumo(() => void recibirDetalle());
void alDiagnosticoConsumo(recibirDiag);
// Se escribió o llegó un archivo semanal: solo al HISTÓRICO le interesa, y solo si es el apartado activo
void alArchivoConsumo(() => {
  if (activa && APARTADOS[indice].id === "historico") pintar("archivo");
});
// El nombre de este equipo no cambia: se pide una vez y se repinta lo que lo usa (EQUIPOS)
void equipoConsumo()
  .then((n) => {
    datos.equipo = n;
    pintar("detalle");
  })
  .catch(() => {});
void alDatosConsumo((d) => {
  datos.disco = d;
  if (activa) pintarDisco();
});
// Por si la página se descarga sin que el núcleo lo note (el núcleo también lo apaga al destruirse)
window.addEventListener("pagehide", () => void vistaConsumo("ventana", false).catch(() => {}));

// --- Guía (F1 y botón «?»): una vista dentro de esta ventana, sin ventana nueva ---
const botonGuia = q<HTMLButtonElement>("guia");

function marcarBotonGuia(abierta: boolean) {
  botonGuia.setAttribute("aria-pressed", String(abierta));
  botonGuia.classList.toggle("cv__btn--activo", abierta);
}

/** Destruye la guía (si está abierta) sin remontar el apartado: lo hace `mostrar`. */
function cerrarGuiaSinVolver() {
  if (!guia) return;
  guia.destruir();
  guia = null;
  marcarBotonGuia(false);
}

/** Vuelve al apartado en que se estaba. */
const cerrarGuia = () => {
  if (guia) mostrar(indice);
};

/** El módulo de la guía se carga al abrirla por primera vez: hasta entonces no existe. */
const abrirGuia = async () => {
  if (guia || cargandoGuia) return;
  cargandoGuia = true;
  try {
    const m = await import("./modulos/consumo/guia");
    if (guia) return;
    cerrarMenu();
    apartado?.destruir();
    apartado = null;
    ajustarTemporizador();
    nav.querySelectorAll<HTMLButtonElement>(".cv__tab").forEach((b) => {
      b.classList.remove("cv__tab--activa");
      b.setAttribute("aria-selected", "false");
    });
    main.scrollTop = 0;
    guia = m.montarGuia(main, cerrarGuia);
    marcarBotonGuia(true);
  } catch {
    mostrarMensaje("NO SE PUDO ABRIR LA GUÍA");
  } finally {
    cargandoGuia = false;
  }
};
const alternarGuia = () => (guia ? cerrarGuia() : void abrirGuia());

// --- Menú INFORMES: solo existe en el DOM mientras está abierto ---
const botonInformes = q<HTMLButtonElement>("informes");
type ItemInforme = [texto: string, tipo: TipoInformeConsumo | "historico", anterior: boolean];
const ITEMS_INFORME: ItemInforme[] = [
  ["ESTA SEMANA", "semana", false],
  ["SEMANA PASADA", "semana", true],
  ["ESTE MES", "mes", false],
  ["MES PASADO", "mes", true],
  ["TRIMESTRE", "trimestre", false],
  ["SEMESTRE", "semestre", false],
  ["AÑO", "anio", false],
  ["HISTÓRICO TOTAL", "historico", false],
];
let menu: HTMLElement | null = null;
let generando = false;

const mensajeError = (e: unknown) => (typeof e === "string" && e ? e : "NO SE PUDO GENERAR EL INFORME");

function generarInforme([, tipo, anterior]: ItemInforme) {
  if (generando) return;
  generando = true;
  botonInformes.disabled = true;
  mostrarMensaje("GENERANDO INFORME…");
  (tipo === "historico" ? generarHistoricoConsumo() : generarInformeConsumo(tipo, anterior))
    .then(mostrarMensaje)
    .catch((e) => mostrarMensaje(mensajeError(e)))
    .finally(() => {
      generando = false;
      botonInformes.disabled = false;
    });
}

const fueraDelMenu = (ev: Event) => {
  if (menu && !menu.contains(ev.target as Node) && !botonInformes.contains(ev.target as Node)) cerrarMenu();
};

function cerrarMenu() {
  if (!menu) return;
  menu.remove();
  menu = null;
  botonInformes.setAttribute("aria-expanded", "false");
  botonInformes.classList.remove("cv__btn--activo");
  document.removeEventListener("pointerdown", fueraDelMenu, true);
  window.removeEventListener("blur", cerrarMenu);
}

function abrirMenu() {
  if (menu || generando) return;
  menu = document.createElement("div");
  menu.className = "cv__menu";
  menu.setAttribute("role", "menu");
  for (const item of ITEMS_INFORME) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "cv__menu-item";
    b.setAttribute("role", "menuitem");
    b.textContent = item[0];
    b.addEventListener("click", () => {
      cerrarMenu();
      generarInforme(item);
    });
    menu.appendChild(b);
  }
  botonInformes.parentElement!.appendChild(menu);
  botonInformes.setAttribute("aria-expanded", "true");
  botonInformes.classList.add("cv__btn--activo");
  document.addEventListener("pointerdown", fueraDelMenu, true);
  window.addEventListener("blur", cerrarMenu);
  menu.querySelector<HTMLButtonElement>(".cv__menu-item")?.focus();
}

// --- Barra de título ---
const ventana = getCurrentWindow();
q("minimizar").addEventListener("click", () => void ventana.minimize().catch(() => {}));
q("ajustes").addEventListener("click", entorno.abrirAjustes);
q("cerrar").addEventListener("click", () => void ventana.close().catch(() => {}));
botonGuia.addEventListener("click", alternarGuia);
botonInformes.addEventListener("click", () => (menu ? cerrarMenu() : abrirMenu()));

// F1: la guía; Esc: cierra el menú o vuelve de la guía (no cierra la ventana); las flechas cambian de apartado
window.addEventListener("keydown", (e) => {
  if (e.key === "F1") {
    e.preventDefault();
    alternarGuia();
    return;
  }
  if (e.key === "Escape") {
    if (menu) {
      cerrarMenu();
      botonInformes.focus();
    } else cerrarGuia();
    return;
  }
  if (menu && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
    e.preventDefault();
    const items = [...menu.querySelectorAll<HTMLButtonElement>(".cv__menu-item")];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    items[(i + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
    return;
  }
  if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
  if (menu || guia || (e.target as HTMLElement | null)?.closest("input, textarea")) return;
  const n = APARTADOS.length;
  mostrar((indice + (e.key === "ArrowRight" ? 1 : n - 1)) % n);
});

mostrar(0);
aplicarActiva();
