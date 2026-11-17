export const reducido = (): boolean =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export const esperar = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Crea un elemento con clase y texto opcionales. */
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  clase = "",
  texto = "",
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (clase) e.className = clase;
  if (texto) e.textContent = texto;
  return e;
}

/** Botón «texto + segmentos». */
export function boton(texto: string, principal: boolean, alClick: () => void): HTMLButtonElement {
  const b = el("button", `btn ${principal ? "principal" : "secundario"}`);
  b.type = "button";
  b.append(el("span", "t", texto));
  const seg = el("span", "seg");
  for (let i = 0; i < (principal ? 8 : 4); i++) seg.append(el("i"));
  b.append(seg);
  b.addEventListener("click", alClick);
  return b;
}

/** Casilla con ✓. */
export function casilla(
  texto: string,
  marcada: boolean,
  alCambiar: (v: boolean) => void,
): HTMLButtonElement {
  const b = el("button", "casilla");
  b.type = "button";
  b.setAttribute("role", "checkbox");
  b.append(el("span", "caja", "✓"), el("span", "", texto));
  const poner = (v: boolean) => {
    b.classList.toggle("on", v);
    b.setAttribute("aria-checked", String(v));
  };
  poner(marcada);
  b.addEventListener("click", () => {
    const v = !b.classList.contains("on");
    poner(v);
    alCambiar(v);
  });
  return b;
}
