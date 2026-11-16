// Navegación entre vistas del panel. Los módulos piden abrir AJUSTES sin conocer a main.ts.
export type SeccionAjustes = "apariencia" | "panel" | "calendario" | "correo" | "cuentas" | "meteo" | "notas" | "sistema";

const EVENTO = "ajustes:abrir";

/** Abre la pantalla AJUSTES, opcionalmente desplazada a una sección. */
export const abrirAjustes = (seccion?: SeccionAjustes): void => {
  window.dispatchEvent(new CustomEvent<SeccionAjustes | undefined>(EVENTO, { detail: seccion }));
};

export const alAbrirAjustes = (manejador: (seccion?: SeccionAjustes) => void): void => {
  window.addEventListener(EVENTO, (e) => manejador((e as CustomEvent<SeccionAjustes | undefined>).detail));
};

// --- Ir a un evento de la agenda (desde la miniventana) ---
const EVENTO_IR = "agenda:ir";
let eventoPendiente: string | null = null;

/** Pide a la agenda ir a un evento. Si METEO aún no está montada, queda pendiente hasta que lo esté. */
export const pedirIrAEvento = (id: string): void => {
  eventoPendiente = id;
  window.dispatchEvent(new Event(EVENTO_IR));
};

/** Devuelve (y olvida) el evento pendiente, si lo hay. */
export const tomarEventoPendiente = (): string | null => {
  const id = eventoPendiente;
  eventoPendiente = null;
  return id;
};

export const alIrAEvento = (manejador: () => void): (() => void) => {
  window.addEventListener(EVENTO_IR, manejador);
  return () => window.removeEventListener(EVENTO_IR, manejador);
};
