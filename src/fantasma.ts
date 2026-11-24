// Ventana fantasma del arrastre de tarjetas (fantasma.html): un marco translúcido que acompaña al
// cursor fuera del panel. El núcleo la muestra, la mueve y le manda el título por evento.
import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/hud.css";
import "./styles/fantasma.css";
import { iniciarTema } from "./lib/tema";
import { alEvento } from "./lib/ipc";

iniciarTema();

const titulo = document.getElementById("titulo")!;
const marco = document.querySelector<HTMLElement>(".fan")!;
// El título llega justo antes de enseñar la ventana: entonces arranca el escaneo, y se para al
// ocultarla (oculta, WebView2 seguiría pintando la animación en bucle).
void alEvento<string>("fantasma://titulo", (t) => {
  titulo.textContent = t;
  marco.classList.add("fan--activo");
});
void alEvento("fantasma://ocultar", () => marco.classList.remove("fan--activo"));
