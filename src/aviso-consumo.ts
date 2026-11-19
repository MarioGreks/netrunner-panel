// Ventana «aviso-consumo»: aviso del 80 % / 100 % del límite de Claude. El núcleo la abre cuando
// hace falta y la destruye con ENTENDIDO (o Esc); si llega otro aviso con ella abierta, cambia el texto.
import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/aviso.css";
import { iniciarTema } from "./lib/tema";
import { alAvisoConsumo, avisoConsumoActual, cerrarAvisoConsumo } from "./lib/ipc";

iniciarTema();

const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const elMarco = q("marco");
const elTitulo = q("titulo");
const elTexto = q("texto");
const btn = q<HTMLButtonElement>("entendido");

// Texto plano (textContent escapa el HTML)
const pintar = () =>
  avisoConsumoActual()
    .then((a) => {
      if (!a) return;
      elTitulo.textContent = a.titulo;
      elTexto.textContent = a.texto;
      elMarco.classList.toggle("aviso--alerta", a.alerta);
    })
    .catch(() => {});

const cerrar = () => void cerrarAvisoConsumo().catch(() => {});

void pintar();
void alAvisoConsumo(() => void pintar());
btn.addEventListener("click", cerrar);
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape" || e.key === "Enter") {
    e.preventDefault();
    cerrar();
  }
});
