// Navegación entre vistas del panel. Los módulos piden abrir AJUSTES sin conocer a main.ts.
export type SeccionAjustes = "panel" | "calendario" | "meteo" | "notas" | "sistema";

const EVENTO = "ajustes:abrir";

/** Abre la pantalla AJUSTES, opcionalmente desplazada a una sección. */
export const abrirAjustes = (seccion?: SeccionAjustes): void => {
  window.dispatchEvent(new CustomEvent<SeccionAjustes | undefined>(EVENTO, { detail: seccion }));
};

export const alAbrirAjustes = (manejador: (seccion?: SeccionAjustes) => void): void => {
  window.addEventListener(EVENTO, (e) => manejador((e as CustomEvent<SeccionAjustes | undefined>).detail));
};
