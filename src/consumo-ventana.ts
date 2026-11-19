// Ventana flotante de CONSUMO (consumo.html): barra de título HUD propia, barra de apartados y el
// apartado activo. El núcleo la crea y la destruye; aquí solo se enseña lo que llega.
//
// REGLA DE RECURSOS
// - Minimizada, tapada u oculta («activa» = false): origen "ventana" apagado en el núcleo, sin
//   intervalo y sin repintados (los lienzos comprueban `activo()` antes de pintar).
// - Solo hay un temporizador: la cuenta atrás de 1 s, y solo con la ventana activa y un apartado
//   que la use. Los repintados ocurren al llegar datos (consumo://estado, consumo://detalle…) o
//   al redimensionar (un fotograma como mucho).
// - Solo existe en el DOM el apartado activo.
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
  serieConsumo,
  vistaConsumo,
} from "./lib/ipc";
import { APARTADOS, montarApartado, type Apartado, type Cambio, type Datos, type Entorno } from "./modulos/consumo/apartados";

iniciarTema();

const REFRESCO_MIN_MS = 30_000; // mínimo entre relecturas del estado al llegar un reset
const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const main = q("main");
const nav = q("nav");
const datosEl = q("datos");

const datos: Datos = { estado: null, recibidoEn: 0, serie: null, detalle: null, sesiones: [], diag: null, disco: null, equipo: "" };
const movimientoReducido = window.matchMedia("(prefers-reduced-motion: reduce)");

let minimizada = false;
let activa = false;
let apartado: Apartado | null = null;
let indice = 0;
let temporizador: number | undefined;
let ultimoRefresco = 0;

const entorno: Entorno = {
  datos,
  activo: () => activa,
  reducido: () => movimientoReducido.matches,
  abrirAjustes: () => void abrirAjustesConsumo().catch(() => {}),
};
const esVisible = () => document.visibilityState === "visible" && !minimizada;

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
  if (APARTADOS[indice].id === "ritmo") void cargarSerie();
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
  b.addEventListener("click", () => i !== indice && mostrar(i));
  nav.appendChild(b);
});

// --- Activa: la ventana se ve (ni minimizada ni oculta) ---
const aplicarActiva = () => {
  const v = esVisible();
  if (v === activa) return;
  activa = v;
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
  if (APARTADOS[indice].id === "ritmo") void cargarSerie();
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

// --- Barra de título ---
const ventana = getCurrentWindow();
q("minimizar").addEventListener("click", () => void ventana.minimize().catch(() => {}));
q("ajustes").addEventListener("click", entorno.abrirAjustes);
q("cerrar").addEventListener("click", () => void ventana.close().catch(() => {}));

// Esc no cierra (es una ventana normal); las flechas cambian de apartado
window.addEventListener("keydown", (e) => {
  if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
  if ((e.target as HTMLElement | null)?.closest("input, textarea")) return;
  const n = APARTADOS.length;
  mostrar((indice + (e.key === "ArrowRight" ? 1 : n - 1)) % n);
});

mostrar(0);
aplicarActiva();
