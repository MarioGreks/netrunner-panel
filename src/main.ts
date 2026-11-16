import "./styles/tokens.css";
import "./styles/hud.css";
import "./styles/panel.css";
import "./styles/sistema.css";
import "./styles/tiempo.css";
import "./styles/notas.css";
import "./styles/accesos.css";
import "./styles/formularios.css";
import "./styles/ajustes.css";
import { alEvento, obtenerConfig, ocultarPanel, type Borde } from "./lib/ipc";
import { bloqueo } from "./lib/bloqueo";
import { alAbrirAjustes, type SeccionAjustes } from "./lib/navegacion";
import { montarPestanas } from "./ui/pestanas";
import { montarEnlace } from "./ui/enlace";
import { crearWireframe } from "./ui/wireframe";
import { getVersion } from "@tauri-apps/api/app";
import { montar as montarSistema } from "./modulos/sistema/vista";
import { montar as montarTiempo } from "./modulos/tiempo/vista";
import { montar as montarNotas } from "./modulos/notas/vista";
import { montar as montarAccesos } from "./modulos/accesos/vista";
import { montar as montarAjustes } from "./modulos/ajustes/vista";

const MODULOS: Record<string, (c: HTMLElement) => void> = {
  sistema: montarSistema,
  tiempo: montarTiempo,
  notas: montarNotas,
  accesos: montarAccesos,
};

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
const wire = crearWireframe(document.getElementById("wire") as HTMLCanvasElement);
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

const salirDeAjustes = () => {
  cerrarAjustes?.();
  cerrarAjustes = null;
  barra.classList.remove("pestanas--ajustes");
  botonAjustes.classList.remove("cabecera__ajustes--activo");
};

const activar = montarPestanas(barra, (id) => {
  salirDeAjustes();
  idActual = id;
  contenido.replaceChildren();
  MODULOS[id](contenido);
});

const abrirVistaAjustes = (seccion?: SeccionAjustes) => {
  salirDeAjustes();
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

// --- Animación de entrada (~400 ms): escaneo, glitch del título y marcos escalonados ---
const movimientoReducido = window.matchMedia("(prefers-reduced-motion: reduce)");
let temporizadorEntrada: number | undefined;

const iniciarEntrada = () => {
  clearTimeout(temporizadorEntrada);
  panel.classList.remove("entrando");
  if (movimientoReducido.matches) return;
  contenido.querySelectorAll<HTMLElement>(".marco").forEach((m, i) => m.style.setProperty("--i", String(Math.min(i, 8))));
  void panel.offsetWidth; // reinicia las animaciones CSS
  panel.classList.add("entrando");
  temporizadorEntrada = window.setTimeout(() => panel.classList.remove("entrando"), 450);
};

let temporizadorSalida: number | undefined;

alEvento("panel://mostrar", () => {
  clearTimeout(temporizadorSalida);
  // Si estaba saliendo (el cursor volvió durante el cierre), solo se revierte la
  // salida: sin escaneo, glitch ni escalonado otra vez.
  const cancelaCierre = panel.classList.contains("saliendo");
  panel.classList.remove("saliendo");
  wire.iniciar();
  if (cancelaCierre) {
    panel.classList.remove("oculto");
    return;
  }
  // Un fotograma con el estado oculto antes de animar la entrada
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      panel.classList.remove("oculto");
      iniciarEntrada();
    }),
  );
});

alEvento("panel://ocultar", () => {
  if (bloqueo.dialogoAbierto) return; // selector de carpetas abierto
  wire.parar(); // el requestAnimationFrame solo corre con el panel visible
  clearTimeout(temporizadorEntrada);
  panel.classList.remove("entrando");
  panel.classList.add("saliendo", "oculto");
  clearTimeout(temporizadorSalida);
  temporizadorSalida = window.setTimeout(() => {
    void ocultarPanel();
  }, 230); // un poco más que la transición de salida (220 ms)
});

alEvento<Borde>("config://cambio", aplicarBorde);

obtenerConfig().then((c) => aplicarBorde(c.edge));
activar("sistema");
