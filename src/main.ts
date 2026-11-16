import "./styles/tokens.css";
import "./styles/hud.css";
import "./styles/panel.css";
import "./styles/sistema.css";
import { alEvento, obtenerConfig, ocultarPanel, type Borde } from "./lib/ipc";
import { montarPestanas } from "./ui/pestanas";
import { montar as montarSistema } from "./modulos/sistema/vista";
import { montar as montarTiempo } from "./modulos/tiempo/vista";
import { montar as montarNotas } from "./modulos/notas/vista";
import { montar as montarAccesos } from "./modulos/accesos/vista";

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

const activar = montarPestanas(barra, (id) => {
  contenido.replaceChildren();
  MODULOS[id](contenido);
});

let temporizadorSalida: number | undefined;

alEvento("panel://mostrar", () => {
  clearTimeout(temporizadorSalida);
  panel.classList.remove("saliendo");
  // Un fotograma con el estado oculto antes de animar la entrada
  requestAnimationFrame(() => requestAnimationFrame(() => panel.classList.remove("oculto")));
});

alEvento("panel://ocultar", () => {
  panel.classList.add("saliendo", "oculto");
  clearTimeout(temporizadorSalida);
  temporizadorSalida = window.setTimeout(() => {
    void ocultarPanel();
  }, 230); // un poco más que la transición de salida (220 ms)
});

alEvento<Borde>("config://cambio", aplicarBorde);

obtenerConfig().then((c) => aplicarBorde(c.edge));
activar("sistema");
