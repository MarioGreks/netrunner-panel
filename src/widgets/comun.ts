// Utilidades de los contenidos de widget.

/** Reúne las escuchas de eventos para soltarlas todas al limpiar (también las que llegan tarde). */
export function escuchas() {
  const sueltas: Array<() => void> = [];
  let vivo = true;
  return {
    anadir(p: Promise<() => void>) {
      p.then((u) => (vivo ? sueltas.push(u) : u())).catch(() => {});
    },
    limpiar() {
      vivo = false;
      sueltas.splice(0).forEach((u) => u());
    },
  };
}

/** Marca el cuerpo como contenedor de tamaño, para los container queries de cada widget. */
export function prepararCuerpo(cuerpo: HTMLElement, clase: string) {
  cuerpo.classList.add("wgc", clase);
}
