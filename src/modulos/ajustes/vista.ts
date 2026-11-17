// Pantalla AJUSTES: apariencia, panel, calendario, correo, cuentas (avanzado), meteo, notas y sistema. No es una pestaña de la
// barra: se abre con el engranaje de la cabecera y se cierra con VOLVER o Esc.
import { getVersion } from "@tauri-apps/api/app";
import { abrirCarpetaDatos, alEvento, obtenerConfig, type Borde } from "../../lib/ipc";
import { bloqueo } from "../../lib/bloqueo";
import type { SeccionAjustes } from "../../lib/navegacion";
import { aplicarTema, limitarOpacidad, OPACIDAD_MAX, OPACIDAD_MIN, TEMAS, temaValido } from "../../lib/tema";
import { actualizarConfig, formularioCarpeta, formularioCiudad, formularioCorreo, formularioCuentasMs, formularioIcs, type Formulario } from "../../ui/formularios";

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
      ${seccion("apariencia", "APARIENCIA", "SKIN // 0x00")}
      ${seccion("panel", "PANEL", "CFG // 0x01")}
      ${seccion("calendario", "CALENDARIO", "ICS // 0x02", "marco--cian")}
      ${seccion("correo", "CORREO", "MAIL // 0x03", "marco--cian")}
      <details class="ajs__avanzado" data-id="avanzado">
        <summary class="ajs__avanzado-t">AVANZADO · CONEXIÓN DIRECTA MICROSOFT</summary>
        ${seccion("cuentas", "CUENTAS", "MS // 0x07")}
      </details>
      ${seccion("meteo", "METEO", "GEO // 0x04", "marco--cian")}
      ${seccion("notas", "NOTAS", "SYNC // 0x05")}
      ${seccion("sistema", "SISTEMA", "SYS // 0x06")}
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const formularios: Formulario[] = [];
  const unlistens: (() => void)[] = [];
  let vivo = true;

  // --- APARIENCIA: temas de color y transparencia ---
  const cuerpoApariencia = q("apariencia");
  cuerpoApariencia.innerHTML = `
    <div class="fm">
      <div class="fm__etiqueta">TEMA DE COLOR</div>
      <div class="ajs__temas" role="radiogroup" aria-label="Tema de color">
        ${TEMAS.map(
          (t) => `<button class="ajs__tema" data-tema="${t.id}" type="button" role="radio" aria-checked="false">
            <span class="ajs__tema-bloque"></span>
            <span class="ajs__tema-linea"></span>
            <span class="ajs__tema-nombre">${t.nombre}</span>
            <span class="ajs__tema-activo">● ACTIVO</span>
          </button>`,
        ).join("")}
      </div>
      <div class="ajs__dato ajs__dato--slider"><label class="fm__etiqueta" for="ajs-opacidad">TRANSPARENCIA</label><span class="ajs__valor" data-id="opacidad-valor">94%</span></div>
      <input id="ajs-opacidad" class="ajs__deslizador" data-id="opacidad" type="range" min="${Math.round(OPACIDAD_MIN * 100)}" max="${Math.round(OPACIDAD_MAX * 100)}" step="1" value="94" />
      <div class="fm__aviso" data-id="aviso-apariencia"></div>
    </div>`;
  const muestras = [...cuerpoApariencia.querySelectorAll<HTMLButtonElement>(".ajs__tema")];
  const deslizador = q<HTMLInputElement>("opacidad");
  const valorOpacidad = q("opacidad-valor");
  const avisoApariencia = q("aviso-apariencia");
  let temaActual = temaValido(document.documentElement.dataset.tema);
  let opacidadActual = limitarOpacidad(parseFloat(document.documentElement.style.getPropertyValue("--opacidad-panel")));

  const marcarTema = (tema: string) => {
    temaActual = tema;
    muestras.forEach((m) => {
      const activo = m.dataset.tema === tema;
      m.classList.toggle("ajs__tema--activo", activo);
      m.setAttribute("aria-checked", String(activo));
    });
  };
  const pintarOpacidad = (o: number) => {
    const pct = Math.round(o * 100);
    deslizador.value = String(pct);
    deslizador.style.setProperty("--pct", `${((pct - OPACIDAD_MIN * 100) / ((OPACIDAD_MAX - OPACIDAD_MIN) * 100)) * 100}%`);
    valorOpacidad.textContent = `${pct}%`;
  };
  const falloApariencia = () => {
    avisoApariencia.textContent = "No se pudo guardar la configuración";
    avisoApariencia.classList.add("fm__aviso--error");
  };
  marcarTema(temaActual);
  pintarOpacidad(opacidadActual);

  obtenerConfig()
    .then((c) => {
      if (!vivo) return;
      marcarTema(temaValido(c.tema));
      opacidadActual = limitarOpacidad(c.opacidad);
      pintarOpacidad(opacidadActual);
    })
    .catch(() => {});
  muestras.forEach((m) =>
    m.addEventListener("click", async () => {
      const tema = m.dataset.tema!;
      avisoApariencia.textContent = "";
      avisoApariencia.classList.remove("fm__aviso--error");
      marcarTema(tema);
      aplicarTema(tema, opacidadActual);
      try {
        await actualizarConfig((c) => (c.tema = tema));
      } catch {
        falloApariencia();
      }
    }),
  );
  // Vista previa en vivo al arrastrar; se guarda al soltar
  deslizador.addEventListener("input", () => {
    opacidadActual = limitarOpacidad(Number(deslizador.value) / 100);
    pintarOpacidad(opacidadActual);
    aplicarTema(temaActual, opacidadActual);
  });
  deslizador.addEventListener("change", async () => {
    avisoApariencia.textContent = "";
    avisoApariencia.classList.remove("fm__aviso--error");
    try {
      await actualizarConfig((c) => (c.opacidad = opacidadActual));
    } catch {
      falloApariencia();
    }
  });

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
      <label class="ajs__casilla">
        <input type="checkbox" data-id="ocultar-pc" />
        <span class="ajs__marca" aria-hidden="true"></span>
        <span>Ocultar en pantalla completa (juegos, vídeos, presentaciones)</span>
      </label>
      <div class="fm__aviso" data-id="aviso-panel"></div>
    </div>`;
  const segmentos = [...cuerpoPanel.querySelectorAll<HTMLButtonElement>(".ajs__seg")];
  const autostart = q<HTMLInputElement>("autostart");
  const ocultarPc = q<HTMLInputElement>("ocultar-pc");
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
      ocultarPc.checked = c.ocultar_pantalla_completa;
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
  ocultarPc.addEventListener("change", async () => {
    limpiarAviso();
    try {
      await actualizarConfig((c) => (c.ocultar_pantalla_completa = ocultarPc.checked));
    } catch {
      ocultarPc.checked = !ocultarPc.checked;
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
  meter("correo", formularioCorreo());
  meter("cuentas", formularioCuentasMs());
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
      if (opciones.seccion === "cuentas") q<HTMLDetailsElement>("avanzado").open = true;
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
