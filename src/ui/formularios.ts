// Formularios de configuración reutilizables (URL ICS, ciudad y carpeta de OneDrive).
// Todos leen la configuración, cambian solo su campo y la guardan con `set_config`
// conservando el resto. Los usa la pantalla AJUSTES.
import { open } from "@tauri-apps/plugin-dialog";
import {
  alEvento,
  buscarCiudad,
  carpetaOnedrive,
  conectarCuentaMs,
  desconectarCuentaMs,
  estadoCuentasMs,
  probarCuentaMs,
  renombrarCuentaMs,
  type CuentaMs,
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

// --- Cuentas Microsoft ---
const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function formularioCuentasMs(): Formulario {
  const el = crear(`
    <label class="fm__etiqueta" for="fm-msid">ID DE CLIENTE (ENTRA)</label>
    <div class="fm__fila">
      <input id="fm-msid" class="fm__input" data-id="msid" type="text" autocomplete="off" spellcheck="false" placeholder="00000000-0000-0000-0000-000000000000" />
      <button class="fm__btn fm__btn--ok" data-id="guardar" type="button">GUARDAR</button>
    </div>
    <div class="fm__ayuda">Registro de app en portal.azure.com · tipo: cualquier directorio + cuentas personales · redirección: cliente público http://localhost</div>
    <div class="fm__fila">
      <button class="fm__btn fm__btn--ok" data-id="conectar" type="button" disabled>+ CONECTAR CUENTA MICROSOFT</button>
    </div>
    <div class="cta" data-id="lista"></div>
    <div class="fm__aviso"></div>`);
  const aviso = avisador(el);
  const campo = q<HTMLInputElement>(el, "msid");
  const conectar = q<HTMLButtonElement>(el, "conectar");
  const lista = q(el, "lista");
  let idGuardado = "";
  let ocupado = false;
  let cuentas: CuentaMs[] = [];
  let vivo = true;
  let confirmando: string | null = null;
  const probando = new Set<string>();

  const pintarConectar = () => {
    conectar.disabled = ocupado || !idGuardado;
    conectar.textContent = ocupado ? "ESPERANDO AL NAVEGADOR…" : "+ CONECTAR CUENTA MICROSOFT";
  };
  const boton = (texto: string, clase: string, accion: () => void, desactivado = false) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `fm__btn ${clase}`.trim();
    b.textContent = texto;
    b.disabled = desactivado;
    b.addEventListener("click", accion);
    return b;
  };

  const iniciarSesion = async () => {
    if (ocupado) return;
    ocupado = true;
    bloqueo.dialogoAbierto = true; // el navegador le quita el foco al panel
    pintarConectar();
    aviso("");
    try {
      await conectarCuentaMs();
      aviso("Cuenta conectada");
    } catch (e) {
      aviso(typeof e === "string" ? e : "No se pudo conectar la cuenta", true);
    } finally {
      ocupado = false;
      bloqueo.dialogoAbierto = false;
      if (vivo) pintarConectar();
    }
  };

  const pintar = () => {
    if (!vivo) return;
    const activo = document.activeElement;
    if (activo instanceof HTMLInputElement && lista.contains(activo)) return; // no pisar una edición
    lista.replaceChildren(
      ...cuentas.map((c) => {
        const fila = document.createElement("div");
        fila.className = "cta__cuenta";
        const cab = document.createElement("div");
        cab.className = "cta__cab";
        const etq = document.createElement("input");
        etq.className = "fm__input cta__etiqueta";
        etq.maxLength = 24;
        etq.value = c.etiqueta;
        etq.setAttribute("aria-label", "Etiqueta de la cuenta");
        etq.addEventListener("change", () => {
          renombrarCuentaMs(c.id, etq.value).catch(() => aviso("No se pudo renombrar la cuenta", true));
        });
        etq.addEventListener("focusout", () => setTimeout(pintar, 0));
        etq.addEventListener("keydown", (e) => {
          if (e.key === "Enter") etq.blur();
        });
        const est = document.createElement("span");
        est.className = `cta__estado cta__estado--${c.estado}`;
        est.textContent = c.estado.toUpperCase();
        cab.append(etq, est);
        const nombre = document.createElement("div");
        nombre.className = "cta__nombre";
        nombre.textContent = c.nombre;
        const correo = document.createElement("div");
        correo.className = "cta__correo";
        correo.textContent = c.correo;
        fila.append(cab, nombre, correo);
        if (c.detalle) {
          const d = document.createElement("div");
          d.className = `cta__detalle cta__detalle--${c.estado}`;
          d.textContent = c.detalle;
          fila.appendChild(d);
        }
        const acc = document.createElement("div");
        acc.className = "fm__fila fm__fila--envolver";
        if (confirmando === c.id) {
          const t = document.createElement("span");
          t.className = "cta__confirma";
          t.textContent = "¿DESCONECTAR?";
          acc.append(
            t,
            boton("SÍ", "fm__btn--peligro", async () => {
              confirmando = null;
              try {
                await desconectarCuentaMs(c.id);
                aviso("Cuenta desconectada");
              } catch (e) {
                aviso(typeof e === "string" ? e : "No se pudo desconectar", true);
              }
            }),
            boton("NO", "", () => {
              confirmando = null;
              pintar();
            }),
          );
        } else {
          acc.append(
            boton(
              probando.has(c.id) ? "PROBANDO…" : "PROBAR",
              "",
              async () => {
                probando.add(c.id);
                aviso("");
                pintar();
                try {
                  aviso(await probarCuentaMs(c.id));
                } catch (e) {
                  aviso(typeof e === "string" ? e : "La prueba falló", true);
                } finally {
                  probando.delete(c.id);
                  pintar();
                }
              },
              probando.has(c.id),
            ),
            boton("RECONECTAR", "fm__btn--ok", () => void iniciarSesion(), ocupado || !idGuardado),
            boton("DESCONECTAR", "fm__btn--peligro", () => {
              confirmando = c.id;
              pintar();
            }),
          );
        }
        fila.appendChild(acc);
        return fila;
      }),
    );
  };

  obtenerConfig()
    .then((c) => {
      idGuardado = c.ms_client_id ?? "";
      campo.value = idGuardado;
      pintarConectar();
    })
    .catch(() => {});
  estadoCuentasMs()
    .then((l) => {
      cuentas = l;
      pintar();
    })
    .catch(() => {});
  let desescuchar: (() => void) | null = null;
  void alEvento<CuentaMs[]>("cuentas_ms://estado", (l) => {
    cuentas = l;
    pintar();
  }).then((un) => {
    if (vivo) desescuchar = un;
    else un();
  });

  q(el, "guardar").addEventListener("click", async () => {
    const id = campo.value.trim();
    if (id && !GUID.test(id)) return aviso("El ID de cliente debe ser un GUID", true);
    try {
      await actualizarConfig((c) => (c.ms_client_id = id || null));
      idGuardado = id;
      pintarConectar();
      pintar();
      aviso(id ? "ID de cliente guardado" : "ID de cliente borrado");
    } catch {
      aviso("No se pudo guardar la configuración", true);
    }
  });
  conectar.addEventListener("click", () => void iniciarSesion());

  return {
    el,
    destruir: () => {
      vivo = false;
      desescuchar?.();
    },
  };
}
