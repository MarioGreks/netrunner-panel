// Pantalla AJUSTES: panel, calendario, meteo, notas y sistema. No es una pestaña de la
// barra: se abre con el engranaje de la cabecera y se cierra con VOLVER o Esc.
import { getVersion } from "@tauri-apps/api/app";
import { abrirCarpetaDatos, alEvento, obtenerConfig, type Borde } from "../../lib/ipc";
import { bloqueo } from "../../lib/bloqueo";
import type { SeccionAjustes } from "../../lib/navegacion";
import { actualizarConfig, formularioCarpeta, formularioCiudad, formularioIcs, type Formulario } from "../../ui/formularios";

const BORDES: { valor: Borde; texto: string }[] = [
  { valor: "left", texto: "IZQ" },
  { valor: "top", texto: "ARRIBA" },
  { valor: "right", texto: "DER" },
];

const seccion = (id: SeccionAjustes, titulo: string, etiqueta: string, color = "") => `
  <section class="marco ${color}" data-seccion="${id}">
    <div class="marco__cabecera"><span>${titulo}</span><span class="marco__etiqueta">${etiqueta}</span></div>
    <div class="marco__cuerpo" data-id="${id}"></div>
  </section>`;

export interface OpcionesAjustes {
  volver: () => void;
  seccion?: SeccionAjustes;
}

/** Monta la vista y devuelve la función que la desmonta (escuchas, temporizadores, formularios). */
export function montar(contenedor: HTMLElement, opciones: OpcionesAjustes): () => void {
  contenedor.innerHTML = `
    <div class="ajs">
      <div class="ajs__barra">
        <button class="ajs__volver" data-id="volver" type="button">← VOLVER</button>
        <span class="ajs__titulo">AJUSTES</span>
        <span class="ajs__esc">ESC</span>
      </div>
      ${seccion("panel", "PANEL", "CFG // 0x01")}
      ${seccion("calendario", "CALENDARIO", "ICS // 0x02", "marco--cian")}
      ${seccion("meteo", "METEO", "GEO // 0x03", "marco--cian")}
      ${seccion("notas", "NOTAS", "SYNC // 0x04")}
      ${seccion("sistema", "SISTEMA", "SYS // 0x05")}
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const formularios: Formulario[] = [];
  const unlistens: (() => void)[] = [];
  let vivo = true;

  // --- PANEL: borde y autoarranque ---
  const cuerpoPanel = q("panel");
  cuerpoPanel.innerHTML = `
    <div class="fm">
      <div class="fm__etiqueta">BORDE DE LA PANTALLA</div>
      <div class="ajs__segmentos" role="radiogroup" aria-label="Borde de la pantalla">
        ${BORDES.map((b) => `<button class="ajs__seg" data-borde="${b.valor}" type="button" role="radio" aria-checked="false">${b.texto}</button>`).join("")}
      </div>
      <label class="ajs__casilla">
        <input type="checkbox" data-id="autostart" />
        <span class="ajs__marca" aria-hidden="true"></span>
        <span>Arrancar con Windows</span>
      </label>
      <div class="fm__aviso" data-id="aviso-panel"></div>
    </div>`;
  const segmentos = [...cuerpoPanel.querySelectorAll<HTMLButtonElement>(".ajs__seg")];
  const autostart = q<HTMLInputElement>("autostart");
  const avisoPanel = q("aviso-panel");

  const marcarBorde = (borde: Borde) => {
    segmentos.forEach((s) => {
      const activo = s.dataset.borde === borde;
      s.classList.toggle("ajs__seg--activo", activo);
      s.setAttribute("aria-checked", String(activo));
    });
  };
  const fallo = () => {
    avisoPanel.textContent = "No se pudo guardar la configuración";
    avisoPanel.classList.add("fm__aviso--error");
  };
  const limpiarAviso = () => {
    avisoPanel.textContent = "";
    avisoPanel.classList.remove("fm__aviso--error");
  };

  obtenerConfig()
    .then((c) => {
      if (!vivo) return;
      marcarBorde(c.edge);
      autostart.checked = c.autostart;
    })
    .catch(() => {});
  segmentos.forEach((s) =>
    s.addEventListener("click", async () => {
      const borde = s.dataset.borde as Borde;
      limpiarAviso();
      try {
        await actualizarConfig((c) => (c.edge = borde));
        marcarBorde(borde);
      } catch {
        fallo();
      }
    }),
  );
  autostart.addEventListener("change", async () => {
    limpiarAviso();
    try {
      await actualizarConfig((c) => (c.autostart = autostart.checked));
    } catch {
      autostart.checked = !autostart.checked;
      fallo();
    }
  });
  void alEvento<Borde>("config://cambio", (b) => vivo && marcarBorde(b)).then((un) => {
    if (vivo) unlistens.push(un);
    else un();
  });

  // --- Formularios compartidos ---
  const meter = (id: SeccionAjustes, f: Formulario) => {
    formularios.push(f);
    q(id).appendChild(f.el);
  };
  meter("calendario", formularioIcs());
  meter("meteo", formularioCiudad());
  meter("notas", formularioCarpeta());

  // --- SISTEMA: versión y datos locales ---
  const cuerpoSistema = q("sistema");
  cuerpoSistema.innerHTML = `
    <div class="fm">
      <div class="ajs__dato"><span class="fm__etiqueta">VERSIÓN</span><span class="ajs__valor" data-id="version">…</span></div>
      <div class="fm__etiqueta">DATOS LOCALES</div>
      <div class="fm__ruta" data-id="ruta">…</div>
      <div class="fm__fila">
        <button class="fm__btn" data-id="abrir-datos" type="button">ABRIR CARPETA</button>
      </div>
      <div class="fm__aviso" data-id="aviso-sistema"></div>
    </div>`;
  getVersion()
    .then((v) => vivo && (q("version").textContent = `v${v}`))
    .catch(() => vivo && (q("version").textContent = "—"));
  abrirCarpetaDatos(false)
    .then((r) => vivo && (q("ruta").textContent = r))
    .catch(() => vivo && (q("ruta").textContent = "No disponible"));
  q("abrir-datos").addEventListener("click", () => {
    const aviso = q("aviso-sistema");
    aviso.textContent = "";
    abrirCarpetaDatos(true).catch(() => {
      aviso.textContent = "No se pudo abrir la carpeta";
      aviso.classList.add("fm__aviso--error");
    });
  });

  // --- Cierre ---
  q("volver").addEventListener("click", opciones.volver);
  const alTeclear = (e: KeyboardEvent) => {
    if (e.key === "Escape" && !bloqueo.dialogoAbierto) {
      e.preventDefault();
      opciones.volver();
    }
  };
  window.addEventListener("keydown", alTeclear);

  // Desplazamiento a la sección pedida por un módulo
  if (opciones.seccion) {
    const destino = contenedor.querySelector<HTMLElement>(`[data-seccion="${opciones.seccion}"]`);
    if (destino) {
      destino.classList.add("ajs__resalte");
      requestAnimationFrame(() => destino.scrollIntoView({ block: "start" }));
    }
  }

  return () => {
    if (!vivo) return;
    vivo = false;
    window.removeEventListener("keydown", alTeclear);
    unlistens.forEach((u) => u());
    formularios.forEach((f) => f.destruir());
  };
}
