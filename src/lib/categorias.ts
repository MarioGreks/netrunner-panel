// Color de categoría compartido por el módulo TIEMPO y la miniventana del borde.

/** Colores de categoría: tres del tema y dos derivados (definidos en tiempo.css). */
const PALETA_CATEGORIAS = ["var(--primario)", "var(--secundario)", "var(--alerta)", "var(--cat-4)", "var(--cat-5)"];

/** Color determinista para un nombre de categoría: mismo nombre, mismo color (FNV-1a). */
export function colorCategoria(nombre: string): string {
  const n = nombre.normalize("NFD").replace(/[\u0300-\u036f]/g, "").trim().toLowerCase();
  let h = 2166136261;
  for (let i = 0; i < n.length; i++) {
    h ^= n.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return PALETA_CATEGORIAS[h % PALETA_CATEGORIAS.length];
}
