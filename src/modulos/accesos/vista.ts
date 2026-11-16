// Módulo ACCESOS: de momento solo un marco de relleno.
export function montar(contenedor: HTMLElement): void {
  contenedor.innerHTML = `
    <section class="marco marco--cian">
      <div class="marco__cabecera"><span>ACCESOS</span><span class="marco__etiqueta">ACC_04</span></div>
      <div class="marco__cuerpo marco__vacio">EN CONSTRUCCIÓN</div>
    </section>`;
}
