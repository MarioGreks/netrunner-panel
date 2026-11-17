// Pantalla de arranque: logo que se traza, texto descifrado y líneas de terminal.
import { el, esperar, reducido } from "./util";

const CARACTERES = "#@%&$/|";

async function descifrar(nodo: HTMLElement, texto: string, paso: number): Promise<void> {
  for (let i = 0; i <= texto.length; i++) {
    let r = "";
    for (let k = i; k < texto.length; k++) {
      r += CARACTERES[Math.floor(Math.random() * CARACTERES.length)];
    }
    nodo.textContent = texto.slice(0, i) + r;
    await esperar(paso);
  }
}

export interface PantallaArranque {
  nodo: HTMLElement;
  /** Lanza las animaciones (tras insertar el nodo) y resuelve al terminar. */
  iniciar: () => Promise<void>;
}

export function arranque(actualizacion: boolean, version: string): PantallaArranque {
  const k = reducido() ? 0.2 : actualizacion ? 0.5 : 1;
  const nodo = el("section", "pant arranque");
  nodo.style.setProperty("--k", String(k));
  nodo.innerHTML = `
    <div class="arr-logo">
      <svg viewBox="0 0 120 120" width="96" height="96" fill="none" aria-hidden="true">
        <path class="tr marco" pathLength="1" d="M22 8 H112 V90 L90 112 H8 V22 Z" stroke="var(--primario)" stroke-width="8"/>
        <path class="tr n1" pathLength="1" d="M36 32 V88" stroke="var(--primario)" stroke-width="12"/>
        <path class="tr n2" pathLength="1" d="M42 32 L80 88" stroke="var(--secundario)" stroke-width="12"/>
        <path class="tr n3" pathLength="1" d="M84 32 V88" stroke="var(--primario)" stroke-width="12"/>
      </svg>
    </div>
    <div class="arr-texto">
      <div class="arr-nombre">&nbsp;</div>
      <div class="arr-sub">&nbsp;</div>
      <div class="arr-term"></div>
    </div>`;
  const nombre = nodo.querySelector<HTMLElement>(".arr-nombre")!;
  const sub = nodo.querySelector<HTMLElement>(".arr-sub")!;
  const term = nodo.querySelector<HTMLElement>(".arr-term")!;

  const lineas = actualizacion
    ? [`Paquete ${version} verificado`, "Cerrando Netrunner Panel"]
    : ["Comprobando sistema", "Montando núcleo HUD", "Verificando firma del paquete"];

  const iniciar = async () => {
    const t0 = performance.now();
    // El nombre se descifra cuando el logo ya está trazado (~1,35 s); la terminal, después
    const texto = (async () => {
      await esperar(1350 * k);
      await descifrar(nombre, "NETRUNNER", 45 * Math.max(k, 0.3));
      sub.textContent = actualizacion ? "PANEL // UPDATE" : "PANEL // SETUP";
    })();
    const terminal = (async () => {
      await esperar(1900 * k);
      for (const l of lineas) {
        const f = el("div", "linea");
        f.append(`> ${l} `, el("span", "ok", "[OK]"));
        term.append(f);
        await esperar(320 * k);
      }
    })();
    await Promise.all([texto, terminal]);
    const total = actualizacion ? 1500 : 3000;
    await esperar(Math.max(0, total * (reducido() ? 0.4 : 1) - (performance.now() - t0)));
  };
  return { nodo, iniciar };
}
