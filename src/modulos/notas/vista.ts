// Módulo NOTAS: de momento solo un marco de relleno.
export function montar(contenedor: HTMLElement): void {
  contenedor.innerHTML = `
    <section class="marco">
      <div class="marco__cabecera"><span>NOTAS</span><span class="marco__etiqueta">NOT_03</span></div>
      <div class="marco__cuerpo marco__vacio">EN CONSTRUCCIÓN</div>
    </section>`;
}
