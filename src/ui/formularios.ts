// Formularios de configuración reutilizables (URL ICS, ciudad y carpeta de OneDrive).
// Todos leen la configuración, cambian solo su campo y la guardan con `set_config`
// conservando el resto. Los usa la pantalla AJUSTES.
import { open } from "@tauri-apps/plugin-dialog";
import {
  buscarCiudad,
  carpetaOnedrive,
  guardarConfig,
  obtenerConfig,
  sincronizarNotas,
  type Ciudad,
  type Config,
} from "../lib/ipc";
import { bloqueo } from "../lib/bloqueo";

export interface Formulario {
  el: HTMLElement;
  /** Cancela temporizadores y búsquedas pendientes. */
  destruir: () => void;
}

/** Lee la configuración actual, aplica el cambio y la guarda entera. */
export async function actualizarConfig(cambio: (c: Config) => void): Promise<void> {
  const c = await obtenerConfig();
  cambio(c);
  await guardarConfig(c);
}

function crear(html: string): HTMLElement {
  const el = document.createElement("div");
  el.className = "fm";
  el.innerHTML = html;
  return el;
}

function avisador(el: HTMLElement) {
  const aviso = el.querySelector<HTMLElement>(".fm__aviso")!;
  return (texto: string, error = false) => {
    aviso.textContent = texto;
    aviso.classList.toggle("fm__aviso--error", error);
  };
}

const q = <T extends HTMLElement>(el: HTMLElement, id: string) => el.querySelector<T>(`[data-id="${id}"]`)!;

// --- URL ICS ---
export function formularioIcs(): Formulario {
  const el = crear(`
    <label class="fm__etiqueta" for="fm-ics">URL ICS DEL CALENDARIO</label>
    <div class="fm__fila">
      <input id="fm-ics" class="fm__input" data-id="ics" type="password" autocomplete="off" spellcheck="false" placeholder="https://… o webcal://…" />
      <button class="fm__btn" data-id="ver" type="button">VER</button>
    </div>
    <div class="fm__fila fm__fila--der">
      <button class="fm__btn fm__btn--peligro" data-id="borrar" type="button">BORRAR</button>
      <button class="fm__btn fm__btn--ok" data-id="guardar" type="button">GUARDAR</button>
    </div>
    <div class="fm__aviso"></div>`);
  const aviso = avisador(el);
  const campo = q<HTMLInputElement>(el, "ics");

  obtenerConfig()
    .then((c) => {
      campo.value = c.ics_url ?? "";
    })
    .catch(() => {});

  q(el, "ver").addEventListener("click", (e) => {
    const visible = campo.type === "text";
    campo.type = visible ? "password" : "text";
    (e.currentTarget as HTMLElement).textContent = visible ? "VER" : "OCULTAR";
  });
  q(el, "guardar").addEventListener("click", async () => {
    const url = campo.value.trim();
    if (!url) return aviso("Escribe una URL o usa BORRAR", true);
    if (!/^(https?|webcals?):\/\//i.test(url)) return aviso("La URL debe empezar por https:// o webcal://", true);
    try {
      await actualizarConfig((c) => (c.ics_url = url));
      aviso("Calendario guardado. Sincronizando…");
    } catch {
      aviso("No se pudo guardar la configuración", true);
    }
  });
  q(el, "borrar").addEventListener("click", async () => {
    campo.value = "";
    try {
      await actualizarConfig((c) => (c.ics_url = null));
      aviso("Calendario eliminado");
    } catch {
      aviso("No se pudo guardar la configuración", true);
    }
  });

  return { el, destruir: () => {} };
}

// --- Ciudad del tiempo ---
const textoCiudad = (c: Ciudad | null) =>
  c ? `ACTUAL: ${c.nombre}${c.pais ? `, ${c.pais}` : ""}`.toUpperCase() : "SIN CIUDAD";

export function formularioCiudad(): Formulario {
  const el = crear(`
    <label class="fm__etiqueta" for="fm-ciudad">CIUDAD</label>
    <div class="fm__actual" data-id="actual"></div>
    <input id="fm-ciudad" class="fm__input fm__input--ancho" data-id="ciudad" type="text" autocomplete="off" spellcheck="false" placeholder="Buscar ciudad…" />
    <ul class="fm__resultados" data-id="resultados"></ul>
    <div class="fm__aviso"></div>`);
  const aviso = avisador(el);
  const actual = q(el, "actual");
  const campo = q<HTMLInputElement>(el, "ciudad");
  const resultados = q<HTMLUListElement>(el, "resultados");
  let temporizador = 0;
  let busqueda = 0;

  actual.textContent = textoCiudad(null);
  obtenerConfig()
    .then((c) => {
      actual.textContent = textoCiudad(c.ciudad);
    })
    .catch(() => {});

  const buscar = async () => {
    const texto = campo.value.trim();
    const mia = ++busqueda;
    if (texto.length < 2) {
      resultados.replaceChildren();
      return;
    }
    try {
      const lista = await buscarCiudad(texto);
      if (mia !== busqueda) return;
      resultados.replaceChildren(
        ...lista.map((c) => {
          const li = document.createElement("li");
          const b = document.createElement("button");
          b.type = "button";
          b.className = "fm__resultado";
          b.textContent = `${c.nombre}${c.pais ? `, ${c.pais}` : ""}`;
          b.addEventListener("click", async () => {
            try {
              await actualizarConfig((cfg) => (cfg.ciudad = c));
              actual.textContent = textoCiudad(c);
              resultados.replaceChildren();
              campo.value = "";
              aviso("Ciudad guardada");
            } catch {
              aviso("No se pudo guardar la configuración", true);
            }
          });
          li.appendChild(b);
          return li;
        }),
      );
      if (lista.length === 0) aviso("Sin resultados", true);
      else aviso("");
    } catch (e) {
      if (mia === busqueda) aviso(typeof e === "string" ? e : "No se pudo buscar la ciudad", true);
    }
  };
  campo.addEventListener("input", () => {
    clearTimeout(temporizador);
    temporizador = window.setTimeout(() => void buscar(), 350);
  });

  return {
    el,
    destruir: () => {
      clearTimeout(temporizador);
      busqueda++;
    },
  };
}

// --- Carpeta de OneDrive para las notas ---
export function formularioCarpeta(): Formulario {
  const el = crear(`
    <div class="fm__etiqueta">CARPETA DE ONEDRIVE</div>
    <div class="fm__ruta" data-id="carpeta">SIN CARPETA (SOLO LOCAL)</div>
    <div class="fm__fila fm__fila--envolver">
      <button class="fm__btn" data-id="elegir" type="button">ELEGIR</button>
      <button class="fm__btn fm__btn--peligro" data-id="desactivar" type="button">DESACTIVAR</button>
      <button class="fm__btn fm__btn--ok" data-id="ahora" type="button">SINCRONIZAR AHORA</button>
    </div>
    <div class="fm__aviso"></div>`);
  const aviso = avisador(el);
  const ruta = q(el, "carpeta");
  let actual: string | null = null;

  const pintar = () => {
    ruta.textContent = actual ?? "SIN CARPETA (SOLO LOCAL)";
  };
  obtenerConfig()
    .then((c) => {
      actual = c.carpeta_sync;
      pintar();
    })
    .catch(() => {});

  const cambiar = async (carpeta: string | null) => {
    try {
      await actualizarConfig((c) => (c.carpeta_sync = carpeta));
      actual = carpeta;
      pintar();
      aviso(carpeta ? "Carpeta guardada. Sincronizando…" : "Sincronización desactivada");
    } catch {
      aviso("No se pudo guardar la configuración", true);
    }
  };

  q(el, "elegir").addEventListener("click", async () => {
    try {
      const inicio = actual ?? (await carpetaOnedrive()) ?? undefined;
      // El selector nativo roba el foco: se avisa para que el panel no se oculte
      bloqueo.dialogoAbierto = true;
      const elegida = await open({ directory: true, multiple: false, defaultPath: inicio, title: "Carpeta de OneDrive para las notas" });
      if (typeof elegida === "string" && elegida) await cambiar(elegida);
    } catch {
      aviso("No se pudo abrir el selector de carpetas", true);
    } finally {
      bloqueo.dialogoAbierto = false;
    }
  });
  q(el, "desactivar").addEventListener("click", () => void cambiar(null));
  q(el, "ahora").addEventListener("click", () => {
    if (!actual) return aviso("Elige antes una carpeta", true);
    aviso("");
    void sincronizarNotas();
  });

  return { el, destruir: () => {} };
}
