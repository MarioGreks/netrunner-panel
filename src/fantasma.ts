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
void alEvento<string>("fantasma://titulo", (t) => {
  titulo.textContent = t;
});
