// ¿Se está viendo el panel? Lo fija main.ts con los eventos panel://mostrar y panel://ocultar.
// Los módulos que trabajan en segundo plano (p. ej. CONSUMO) lo usan para parar todo mientras está oculto.
// Empieza oculto: el panel arranca escondido y siempre llega un panel://mostrar al enseñarse
// (también al abrirlo a mano). Así nada trabaja por un visibilityState engañoso de WebView2.
let visible = false;
const oyentes = new Set<(visible: boolean) => void>();

export const panelVisible = () => visible;

export function fijarPanelVisible(v: boolean): void {
  if (v === visible) return;
  visible = v;
  oyentes.forEach((cb) => cb(v));
}

/** Avisa de cada cambio de visibilidad; devuelve la función que quita el oyente. */
export function alCambiarVisibilidadPanel(cb: (visible: boolean) => void): () => void {
  oyentes.add(cb);
  return () => oyentes.delete(cb);
}
