import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/hud.css";
import "./styles/panel.css";
import "./styles/sistema.css";
import "./styles/tiempo.css";
import "./styles/notas.css";
import "./styles/accesos.css";
import "./styles/correo.css";
import "./styles/formularios.css";
import "./styles/consumo.css";
import "./styles/ajustes.css";
import { alEvento, estadoCorreo, obtenerConfig, ocultarPanel, type Borde, type EstadoCorreo } from "./lib/ipc";
import { iniciarTema } from "./lib/tema";
import { bloqueo } from "./lib/bloqueo";
import { fijarPanelVisible } from "./lib/visibilidad";
import { cargarInterfaz, recordarPestana } from "./lib/interfaz";
import { alAbrirAjustes, esSeccionAjustes, pedirIrAEvento, type SeccionAjustes } from "./lib/navegacion";
import { fijarContador, montarPestanas } from "./ui/pestanas";
import { montarEnlace } from "./ui/enlace";
import { montarResumen } from "./ui/resumen";
import { getVersion } from "@tauri-apps/api/app";
import { montar as montarSistema } from "./modulos/sistema/vista";
import { montar as montarTiempo } from "./modulos/tiempo/vista";
import { montar as montarNotas } from "./modulos/notas/vista";
import { montar as montarAccesos } from "./modulos/accesos/vista";
import { montar as montarCorreo } from "./modulos/correo/vista";
import { montar as montarConsumo } from "./modulos/consumo/vista";
import { montar as montarAjustes } from "./modulos/ajustes/vista";

/** Un módulo puede devolver una función de limpieza (timers, listeners) que se llama al cambiar de pestaña. */
type Modulo = (c: HTMLElement) => void | (() => void);

const MODULOS: Record<string, Modulo> = {
  sistema: montarSistema,
  tiempo: montarTiempo,
  correo: montarCorreo,
  notas: montarNotas,
  accesos: montarAccesos,
  consumo: montarConsumo,
};

iniciarTema();

const panel = document.getElementById("panel")!;
const contenido = document.getElementById("contenido")!;
const barra = document.getElementById("pestanas")!;
const reloj = document.getElementById("hora")!;

const aplicarBorde = (borde: Borde) => {
  document.documentElement.dataset.borde = borde;
};
// Valor por defecto hasta que llegue la configuración real
aplicarBorde("right");

// Hora pequeña de la cabecera
const pintarHora = () => {
  reloj.textContent = new Date().toLocaleTimeString("es-ES");
};
pintarHora();
setInterval(pintarHora, 1000);

const botonAjustes = document.getElementById("ajustes")!;
montarEnlace(document.getElementById("enlace")!);

// Pie con la versión de la app
getVersion()
  .then((v) => {
    document.getElementById("version")!.textContent = `NETRUNNER//PANEL  v${v}`;
  })
  .catch(() => {});

// --- Vistas: un módulo de la barra o la pantalla AJUSTES ---
let idActual = "sistema";
let cerrarAjustes: (() => void) | null = null;
let limpiarModulo: (() => void) | null = null;
// La pestaña inicial (restaurada del estado guardado) no cuenta como un cambio del usuario
let pestanaInicialLista = false;

const soltarModulo = () => {
  limpiarModulo?.();
  limpiarModulo = null;
};

const salirDeAjustes = () => {
  cerrarAjustes?.();
  cerrarAjustes = null;
  barra.classList.remove("pestanas--ajustes");
  botonAjustes.classList.remove("cabecera__ajustes--activo");
};

const activar = montarPestanas(barra, (id) => {
  salirDeAjustes();
  soltarModulo();
  idActual = id;
  if (pestanaInicialLista) recordarPestana(id);
  contenido.replaceChildren();
  limpiarModulo = MODULOS[id](contenido) || null;
});

montarResumen(document.getElementById("resumen")!, (id) => activar(id));

const abrirVistaAjustes = (seccion?: SeccionAjustes) => {
  salirDeAjustes();
  soltarModulo();
  contenido.replaceChildren();
  barra.classList.add("pestanas--ajustes");
  botonAjustes.classList.add("cabecera__ajustes--activo");
  cerrarAjustes = montarAjustes(contenido, { volver: () => activar(idActual), seccion });
};
botonAjustes.addEventListener("click", () => {
  if (cerrarAjustes) activar(idActual);
  else abrirVistaAjustes();
});
alAbrirAjustes(abrirVistaAjustes);

// --- Animación «holograma»: entrada (~550 ms) y apagado tipo CRT (~380 ms) ---
const movimientoReducido = window.matchMedia("(prefers-reduced-motion: reduce)");
const DURACION_ENTRADA = 550;
const DURACION_SALIDA = 380;
const DURACION_FUNDIDO = 150;
let temporizadorEntrada: number | undefined;
let temporizadorSalida: number | undefined;
let temporizadorVuelta: number | undefined;

const iniciarEntrada = () => {
  clearTimeout(temporizadorEntrada);
  panel.classList.remove("entrando");
  void panel.offsetWidth; // reinicia las animaciones CSS
  panel.classList.add("entrando");
  temporizadorEntrada = window.setTimeout(
    () => panel.classList.remove("entrando"),
    (movimientoReducido.matches ? DURACION_FUNDIDO : DURACION_ENTRADA) + 20,
  );
};

alEvento("panel://mostrar", () => {
  fijarPanelVisible(true);
  clearTimeout(temporizadorSalida);
  // Si estaba saliendo (el cursor volvió durante el cierre), el panel vuelve a verse
  // sin repetir el parpadeo de entrada.
  if (panel.classList.contains("saliendo")) {
    panel.classList.remove("saliendo", "oculto");
    clearTimeout(temporizadorVuelta);
    void panel.offsetWidth;
    panel.classList.add("vuelve");
    temporizadorVuelta = window.setTimeout(() => panel.classList.remove("vuelve"), 160);
    return;
  }
  // Un fotograma con el estado oculto antes de animar la entrada
  panel.classList.add("oculto");
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      panel.classList.remove("oculto");
      iniciarEntrada();
    }),
  );
});

alEvento("panel://ocultar", () => {
  if (bloqueo.dialogoAbierto) return; // selector de carpetas abierto
  fijarPanelVisible(false);
  clearTimeout(temporizadorEntrada);
  clearTimeout(temporizadorVuelta);
  panel.classList.remove("entrando", "vuelve", "oculto");
  void panel.offsetWidth;
  panel.classList.add("saliendo");
  clearTimeout(temporizadorSalida);
  temporizadorSalida = window.setTimeout(
    () => {
      // Se deja oculto antes de esconder la ventana para que no quede un destello
      panel.classList.add("oculto");
      panel.classList.remove("saliendo");
      void ocultarPanel();
    },
    (movimientoReducido.matches ? DURACION_FUNDIDO : DURACION_SALIDA) + 10,
  );
});

// Sin carga (bandeja) abre AJUSTES; con una sección («consumo», desde la ventana de CONSUMO), desplazado a ella
alEvento<string | null>("panel://ajustes", (seccion) => abrirVistaAjustes(esSeccionAjustes(seccion) ? seccion : undefined));

// La miniventana pide ir a un evento: pestaña METEO (saliendo de AJUSTES si hace falta) y scroll hasta él
alEvento<string>("panel://ir_evento", (id) => {
  if (cerrarAjustes || idActual !== "tiempo") activar("tiempo");
  pedirIrAEvento(id);
});

// Contador de correos importantes sin leer en la pestaña CORREO
const pintarContador = (e: EstadoCorreo) => fijarContador("correo", e.correos.filter((c) => c.no_leido).length);
alEvento<EstadoCorreo>("correo://estado", pintarContador);
estadoCorreo().then(pintarContador).catch(() => {});

alEvento<Borde>("config://cambio", aplicarBorde);

obtenerConfig().then((c) => aplicarBorde(c.edge));
// Pestaña y tira METEO guardadas (compartidas entre equipos); sin estado válido, METEO
void cargarInterfaz().then((id) => {
  activar(Object.prototype.hasOwnProperty.call(MODULOS, id) ? id : "tiempo");
  pestanaInicialLista = true;
});
