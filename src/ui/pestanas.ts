// Barra de pestañas con iconos SVG de estilo HUD. Teclas 1-4 cambian de pestaña.
export interface Pestana {
  id: string;
  nombre: string;
  icono: string;
}

const svg = (contenido: string) =>
  `<svg viewBox="0 0 24 24" aria-hidden="true">${contenido}</svg>`;

export const PESTANAS: Pestana[] = [
  // Orden de la v1.1: tiempo/meteo, accesos, notas, sistema (teclas 1-4 en este orden)
  { id: "tiempo", nombre: "METEO", icono: svg('<circle cx="12" cy="12" r="8"/><path d="M12 7v5l3 2M12 2v2M12 20v2"/>') },
  { id: "accesos", nombre: "ACCESOS", icono: svg('<path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"/>') },
  { id: "notas", nombre: "NOTAS", icono: svg('<path d="M5 3h10l4 4v14H5zM15 3v4h4M8 12h8M8 16h6"/>') },
  { id: "sistema", nombre: "SISTEMA", icono: svg('<path d="M4 4h16v12H4zM9 20h6M12 16v4M7 12l3-3 2 2 4-4"/>') },
];

export function montarPestanas(
  barra: HTMLElement,
  alCambiar: (id: string) => void,
): (id: string) => void {
  const botones = new Map<string, HTMLButtonElement>();

  const activar = (id: string) => {
    botones.forEach((b, k) => b.classList.toggle("pestana--activa", k === id));
    alCambiar(id);
  };

  PESTANAS.forEach((p, i) => {
    const b = document.createElement("button");
    b.className = "pestana";
    b.title = `${p.nombre} (${i + 1})`;
    b.innerHTML = `${p.icono}<span>${p.nombre}</span>`;
    b.addEventListener("click", () => activar(p.id));
    barra.appendChild(b);
    botones.set(p.id, b);
  });

  window.addEventListener("keydown", (e) => {
    // No cambiar de pestaña mientras se escribe
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
    const p = PESTANAS[Number(e.key) - 1];
    if (p && !e.ctrlKey && !e.altKey && !e.metaKey) activar(p.id);
  });

  return activar;
}
