// Pestañas bajo la cabecera, estilo carpeta: la activa es un bloque macizo con la esquina cortada
// que se desliza bajo el texto. Teclas 1-4 cambian de pestaña.
export interface Pestana {
  id: string;
  nombre: string;
}

export const PESTANAS: Pestana[] = [
  // Orden de la v1.1: tiempo/meteo, accesos, notas, sistema (teclas 1-4 en este orden)
  { id: "tiempo", nombre: "METEO" },
  { id: "accesos", nombre: "ACCESOS" },
  { id: "notas", nombre: "NOTAS" },
  { id: "sistema", nombre: "SISTEMA" },
];

export function montarPestanas(
  barra: HTMLElement,
  alCambiar: (id: string) => void,
): (id: string) => void {
  const botones = new Map<string, HTMLButtonElement>();
  let actual: string | null = null;

  // Indicador amarillo que se desliza bajo la pestaña activa
  const indicador = document.createElement("span");
  indicador.className = "pestanas__indicador pestanas__indicador--quieto";
  indicador.setAttribute("aria-hidden", "true");
  barra.appendChild(indicador);

  const colocar = () => {
    const b = actual ? botones.get(actual) : undefined;
    if (!b || b.offsetWidth === 0) return;
    indicador.style.width = `${b.offsetWidth}px`;
    indicador.style.transform = `translateX(${b.offsetLeft}px)`;
  };

  const activar = (id: string) => {
    botones.forEach((b, k) => b.classList.toggle("pestana--activa", k === id));
    const primera = actual === null;
    actual = id;
    colocar();
    if (primera) {
      // La primera colocación no se anima; las siguientes sí
      void indicador.offsetWidth;
      indicador.classList.remove("pestanas__indicador--quieto");
    }
    alCambiar(id);
  };

  PESTANAS.forEach((p, i) => {
    const b = document.createElement("button");
    b.className = "pestana";
    b.title = `${p.nombre} (${i + 1})`;
    b.innerHTML = `<small class="pestana__num">0${i + 1}</small><span>${p.nombre}</span>`;
    b.addEventListener("click", () => activar(p.id));
    barra.appendChild(b);
    botones.set(p.id, b);
  });

  // Reajusta el indicador si cambia el ancho o se cargan las fuentes
  new ResizeObserver(colocar).observe(barra);
  void document.fonts?.ready.then(colocar);

  window.addEventListener("keydown", (e) => {
    // No cambiar de pestaña mientras se escribe
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
    const p = PESTANAS[Number(e.key) - 1];
    if (p && !e.ctrlKey && !e.altKey && !e.metaKey) activar(p.id);
  });

  return activar;
}
