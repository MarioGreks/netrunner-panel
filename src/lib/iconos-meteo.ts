// Iconos meteorológicos SVG de trazo fino, compartidos por el módulo TIEMPO y el resumen de la cabecera.
const NUBE = "M7 15h10a3.5 3.5 0 0 0 .4-6.98A5 5 0 0 0 7.5 7.5 3.75 3.75 0 0 0 7 15z";
const ICONOS: Record<string, string> = {
  sol: `<circle cx="12" cy="12" r="4"/><path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M4.9 19.1l1.8-1.8M17.3 6.7l1.8-1.8"/>`,
  luna: `<path d="M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5z"/>`,
  nubes: `<path d="M7 18h10a4 4 0 0 0 .5-7.97A5.5 5.5 0 0 0 7 9.5 4.25 4.25 0 0 0 7 18z"/>`,
  lluvia: `<path d="${NUBE}"/><path d="M8 18l-1 2.5M12 18l-1 2.5M16 18l-1 2.5"/>`,
  nieve: `<path d="${NUBE}"/><path d="M8 18.5v.01M12 20v.01M16 18.5v.01M10 21v.01M14 21.5v.01" stroke-width="2" stroke-linecap="round"/>`,
  tormenta: `<path d="${NUBE}"/><path d="M12.5 15.5l-2.2 3.2h3L11.3 22"/>`,
  niebla: `<path d="M4 8h16M6 12h14M4 16h14M8 20h10"/>`,
};

/** SVG del icono meteorológico `nombre` (el de las nubes si no existe) con el tamaño y la clase dados. */
export const iconoMeteo = (nombre: string, tam = 24, clase = "tmp__icono") =>
  `<svg class="${clase}" viewBox="0 0 24 24" width="${tam}" height="${tam}" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONOS[nombre] ?? ICONOS.nubes}</svg>`;
