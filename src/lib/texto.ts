// Escapa HTML para meter texto en plantillas con innerHTML.
export const esc = (t: string) =>
  t.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

// Número a dos dígitos con cero a la izquierda.
export const dos = (n: number) => String(n).padStart(2, "0");

// Acorta a `max` caracteres añadiendo «…».
export const recortar = (t: string, max: number) => (t.length > max ? `${t.slice(0, max - 1).trimEnd()}…` : t);

// Bytes a GiB con un decimal («15.9»).
export const gb = (bytes: number) => (bytes / 1024 ** 3).toFixed(1);
