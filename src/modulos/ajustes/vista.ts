// Pantalla AJUSTES: apariencia, panel, calendario, correo, cuentas (avanzado), meteo, sincronización entre equipos, consumo de Claude y sistema. No es una pestaña de la
// barra: se abre con el engranaje de la cabecera y se cierra con VOLVER o Esc.
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import {
  abrirCarpetaDatos,
  ajustesConsumo,
  alAjustesConsumo,
  alEvento,
  buscarActualizacion,
  datosConsumo,
  diagnosticoConsumo,
  guardarAjustesConsumo,
  instalarPluginConsumo,
  obtenerConfig,
  type AjustesConsumo,
  type Borde,
  type CtxMaxConsumo,
} from "../../lib/ipc";
import { bloqueo } from "../../lib/bloqueo";
import type { SeccionAjustes } from "../../lib/navegacion";
import { aplicarTema, limitarOpacidad, OPACIDAD_MAX, OPACIDAD_MIN, TEMAS, temaValido } from "../../lib/tema";
import { actualizarConfig, formularioCarpeta, formularioCiudad, formularioCorreo, formularioCuentasMs, formularioIcs, type Formulario } from "../../ui/formularios";

const BORDES: { valor: Borde; texto: string }[] = [
  { valor: "left", texto: "IZQ" },
  { valor: "top", texto: "ARRIBA" },
  { valor: "right", texto: "DER" },
];

const CTX_OPCIONES: { valor: CtxMaxConsumo; texto: string }[] = [
  { valor: 200000, texto: "200 K" },
  { valor: 500000, texto: "500 K" },
  { valor: 1000000, texto: "1 M" },
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
      ${seccion("notas", "SINCRONIZACIÓN ENTRE EQUIPOS", "SYNC // 0x05")}
      ${seccion("consumo", "CONSUMO DE CLAUDE", "CLD // 0x08")}
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
  const montados = new Map<SeccionAjustes, Formulario>();
  const meter = (id: SeccionAjustes, f: Formulario) => {
    formularios.push(f);
    montados.set(id, f);
    q(id).appendChild(f.el);
  };
  const FABRICAS: [SeccionAjustes, () => Formulario][] = [
    ["calendario", formularioIcs],
    ["correo", formularioCorreo],
    ["cuentas", formularioCuentasMs],
    ["meteo", formularioCiudad],
    ["notas", formularioCarpeta],
  ];
  FABRICAS.forEach(([id, fabrica]) => meter(id, fabrica()));

  // Llegan ajustes de otro equipo (URL del calendario, ciudad, filtros de correo): se
  // recargan los formularios afectados, salvo el que el usuario esté editando.
  const REMOTOS: SeccionAjustes[] = ["calendario", "correo", "meteo"];
  void alEvento("ajustes://cambio", () => {
    if (!vivo) return;
    for (const id of REMOTOS) {
      const cuerpo = q(id);
      if (cuerpo.contains(document.activeElement)) continue;
      const viejo = montados.get(id);
      const fabrica = FABRICAS.find(([i]) => i === id)?.[1];
      if (!viejo || !fabrica) continue;
      viejo.destruir();
      formularios.splice(formularios.indexOf(viejo), 1);
      cuerpo.replaceChildren();
      meter(id, fabrica());
    }
  }).then((un) => {
    if (vivo) unlistens.push(un);
    else un();
  });

  // --- SISTEMA: versión y datos locales ---
  const cuerpoSistema = q("sistema");
  cuerpoSistema.innerHTML = `
    <div class="fm">
      <div class="ajs__dato"><span class="fm__etiqueta">VERSIÓN</span><span class="ajs__valor" data-id="version">…</span></div>
      <div class="fm__fila">
        <button class="fm__btn" data-id="buscar-act" type="button">BUSCAR ACTUALIZACIONES</button>
      </div>
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

  q("buscar-act").addEventListener("click", () => {
    const boton = q("buscar-act") as HTMLButtonElement;
    const aviso = q("aviso-sistema");
    boton.disabled = true;
    boton.textContent = "COMPROBANDO…";
    aviso.textContent = "";
    aviso.classList.remove("fm__aviso--error");
    buscarActualizacion()
      .then((info) => {
        if (!vivo) return;
        aviso.textContent = info ? `Hay una versión nueva: v${info.version}` : "Estás en la última versión";
      })
      .catch((e) => {
        if (!vivo) return;
        aviso.textContent = String(e);
        aviso.classList.add("fm__aviso--error");
      })
      .finally(() => {
        if (!vivo) return;
        boton.disabled = false;
        boton.textContent = "BUSCAR ACTUALIZACIONES";
      });
  });

  // --- CONSUMO DE CLAUDE: contexto, avisos, carpeta de sincronización, espacio y plugin ---
  const cuerpoConsumo = q("consumo");
  cuerpoConsumo.innerHTML = `
    <div class="fm">
      <div class="fm__etiqueta">CONTEXTO MÁXIMO</div>
      <div class="ajs__segmentos" role="radiogroup" aria-label="Contexto máximo">
        ${CTX_OPCIONES.map((o) => `<button class="ajs__seg" data-ctx="${o.valor}" type="button" role="radio" aria-checked="false">${o.texto}</button>`).join("")}
      </div>
      <label class="ajs__casilla">
        <input type="checkbox" data-id="cns-avisos" />
        <span class="ajs__marca" aria-hidden="true"></span>
        <span>AVISOS AL 80 % Y AL 100 %</span>
      </label>
      <div class="fm__etiqueta">CARPETA DE SINCRONIZACIÓN</div>
      <div class="fm__ruta" data-id="cns-carpeta">…</div>
      <div class="fm__ruta ajs__tenue" data-id="cns-extras" hidden></div>
      <div class="fm__fila fm__fila--envolver">
        <button class="fm__btn" data-id="cns-elegir" type="button">ELEGIR</button>
        <button class="fm__btn" data-id="cns-defecto" type="button">POR DEFECTO</button>
      </div>
      <div class="fm__etiqueta">MÁXIMO DE ESPACIO (GB)</div>
      <div class="fm__fila">
        <input class="fm__input" data-id="cns-gb" type="number" min="0" max="1000000" step="0.5" value="0" aria-label="Máximo de espacio en GB (0 = sin máximo)" />
      </div>
      <div class="fm__ayuda">0 = SIN MÁXIMO</div>
      <div class="fm__ruta" data-id="cns-datos">…</div>
      <div class="fm__etiqueta">PLUGIN</div>
      <div class="fm__ayuda ajs__pre" data-id="cns-plugin"></div>
      <div class="fm__fila fm__fila--envolver">
        <button class="fm__btn" data-id="cns-comprobar" type="button">COMPROBAR</button>
        <button class="fm__btn fm__btn--ok" data-id="cns-instalar" type="button" hidden>INSTALAR / REPARAR PLUGIN</button>
      </div>
      <div class="fm__aviso" data-id="cns-aviso"></div>
    </div>`;
  const cnsSegmentos = [...cuerpoConsumo.querySelectorAll<HTMLButtonElement>(".ajs__seg")];
  const cnsAvisos = q<HTMLInputElement>("cns-avisos");
  const cnsCarpeta = q("cns-carpeta");
  const cnsExtras = q("cns-extras");
  const cnsGb = q<HTMLInputElement>("cns-gb");
  const cnsDatos = q("cns-datos");
  const cnsPlugin = q("cns-plugin");
  const cnsComprobar = q<HTMLButtonElement>("cns-comprobar");
  const cnsInstalar = q<HTMLButtonElement>("cns-instalar");
  const cnsAviso = q("cns-aviso");
  let cns: AjustesConsumo | null = null;

  const avisoCns = (texto: string, error = false) => {
    cnsAviso.textContent = texto;
    cnsAviso.classList.toggle("fm__aviso--error", error);
  };
  const pintarConsumo = () => {
    if (!cns) return;
    cnsSegmentos.forEach((s) => {
      const activo = Number(s.dataset.ctx) === cns!.ctxMax;
      s.classList.toggle("ajs__seg--activo", activo);
      s.setAttribute("aria-checked", String(activo));
    });
    cnsAvisos.checked = cns.avisos;
    cnsCarpeta.textContent = cns.carpetasSync[0] ?? cns.carpetaDefecto;
    const extras = cns.carpetasSync.slice(1);
    cnsExtras.hidden = extras.length === 0;
    cnsExtras.textContent = extras.length ? `+ ${extras.join("  ·  ")}` : "";
    if (document.activeElement !== cnsGb) cnsGb.value = String(cns.gbMax);
  };
  const pintarDatos = () => {
    datosConsumo()
      .then((d) => {
        if (!vivo) return;
        const libre = d.libre !== null ? ` · ${d.libre.toFixed(0)} GB LIBRES` : "";
        cnsDatos.textContent = d.gb === null ? "AÚN SIN MEDIR (UNOS SEGUNDOS TRAS ARRANCAR)" : `${d.texto}${libre}`;
      })
      .catch(() => vivo && (cnsDatos.textContent = "—"));
  };
  const guardarConsumo = async (cambio: (a: AjustesConsumo) => void) => {
    if (!cns) return;
    const nuevo = { ...cns, carpetasSync: [...cns.carpetasSync] };
    cambio(nuevo);
    avisoCns("");
    try {
      cns = await guardarAjustesConsumo(nuevo);
      if (vivo) pintarDatos();
    } catch (e) {
      avisoCns(typeof e === "string" ? e : "No se pudo guardar los ajustes de Consumo", true);
    }
    if (vivo) pintarConsumo();
  };
  const cargarConsumo = () =>
    ajustesConsumo()
      .then((a) => {
        if (!vivo) return;
        cns = a;
        pintarConsumo();
      })
      .catch(() => vivo && avisoCns("No se pudieron leer los ajustes de Consumo", true));
  void cargarConsumo();
  pintarDatos();

  cnsSegmentos.forEach((s) => s.addEventListener("click", () => void guardarConsumo((a) => (a.ctxMax = Number(s.dataset.ctx) as CtxMaxConsumo))));
  cnsAvisos.addEventListener("change", () => void guardarConsumo((a) => (a.avisos = cnsAvisos.checked)));
  cnsGb.addEventListener("change", () => {
    const gb = Number(cnsGb.value);
    void guardarConsumo((a) => (a.gbMax = Number.isFinite(gb) && gb > 0 ? Math.min(gb, 1_000_000) : 0));
  });
  q("cns-defecto").addEventListener("click", () =>
    void guardarConsumo((a) => (a.carpetasSync = [a.carpetaDefecto, ...a.carpetasSync.slice(1)])),
  );
  q("cns-elegir").addEventListener("click", async () => {
    if (!cns) return;
    try {
      // El selector nativo roba el foco: se avisa para que el panel no se oculte
      bloqueo.dialogoAbierto = true;
      const elegida = await open({ directory: true, multiple: false, defaultPath: cns.carpetasSync[0] ?? cns.carpetaDefecto, title: "Carpeta de sincronización de Consumo" });
      if (typeof elegida === "string" && elegida) await guardarConsumo((a) => (a.carpetasSync = [elegida, ...a.carpetasSync.slice(1)]));
    } catch {
      avisoCns("No se pudo abrir el selector de carpetas", true);
    } finally {
      bloqueo.dialogoAbierto = false;
    }
  });
  cnsComprobar.addEventListener("click", () => {
    cnsComprobar.disabled = true;
    cnsComprobar.textContent = "COMPROBANDO…";
    cnsPlugin.textContent = "";
    diagnosticoConsumo()
      .then((d) => {
        if (!vivo) return;
        cnsPlugin.textContent = d.texto;
        cnsInstalar.hidden = !d.puedeInstalar;
      })
      .catch((e) => vivo && (cnsPlugin.textContent = String(e)))
      .finally(() => {
        if (!vivo) return;
        cnsComprobar.disabled = false;
        cnsComprobar.textContent = "COMPROBAR";
      });
  });
  cnsInstalar.addEventListener("click", () => {
    cnsInstalar.disabled = true;
    cnsPlugin.textContent = "INSTALANDO…";
    instalarPluginConsumo()
      .then((t) => vivo && (cnsPlugin.textContent = t))
      .catch((e) => vivo && (cnsPlugin.textContent = String(e)))
      .finally(() => vivo && (cnsInstalar.disabled = false));
  });
  void alAjustesConsumo(() => vivo && void cargarConsumo()).then((un) => {
    if (vivo) unlistens.push(un);
    else un();
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
