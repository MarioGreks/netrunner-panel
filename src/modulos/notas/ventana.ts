// Vista ampliada de NOTAS (ventana desacoplada): cuaderno con tareas y lista de notas a la izquierda
// y un editor de texto plano a la derecha. Mientras está abierta, el panel muestra el hueco
// DESACOPLADO, así que aquí nunca se edita a la vez que en el panel.
import "../../styles/notas-ventana.css";
import {
  alEvento,
  borrarNota,
  borrarTarea,
  estadoNotas,
  guardarNota,
  guardarTarea,
  sincronizarNotas,
  type EstadoNotas,
  type Nota,
  type Tarea,
} from "../../lib/ipc";
import { dos, esc } from "../../lib/texto";
import { hace, MARCA, NUBE, primeraLinea } from "./vista";

const ANTIRREBOTE_MS = 800;
/** Por debajo de este ancho la ventana pasa a una columna con la lista plegable. */
const ANCHO_UNA_COLUMNA = 620;

export function montarVentanaNotas(cont: HTMLElement): () => void {
  cont.innerHTML = `
    <div class="nvm" data-id="raiz">
      <div class="nvm__barra">
        <button class="nvm__btn nvm__plegar" data-id="plegar" type="button" aria-expanded="true">LISTA ▾</button>
        <span class="nvm__sync" data-id="sync">${NUBE}<span data-id="sync-texto"></span></span>
        <button class="nvm__btn" data-id="sincronizar" type="button" hidden>SINCRONIZAR</button>
      </div>
      <div class="nvm__error" data-id="error" hidden></div>

      <aside class="nvm__lado" data-id="lado">
        <section class="marco nvm__marco-tareas">
          <div class="marco__cabecera"><span>TAREAS</span><span class="marco__etiqueta" data-id="contador">00 PENDIENTES</span></div>
          <div class="marco__cuerpo">
            <input class="nvm__input" data-id="nueva-tarea" type="text" autocomplete="off" spellcheck="false" maxlength="500" placeholder="+ NUEVA TAREA_" />
            <div class="nvm__scroll nvm__scroll--tareas" data-id="lista-tareas"></div>
          </div>
        </section>
        <section class="marco nvm__marco-notas">
          <div class="marco__cabecera">
            <span>NOTAS</span>
            <button class="nvm__btn nvm__btn--ok" data-id="nueva-nota" type="button">+ NOTA</button>
          </div>
          <div class="marco__cuerpo">
            <input class="nvm__input" data-id="buscar" type="search" autocomplete="off" spellcheck="false" placeholder="BUSCAR_" aria-label="Buscar en las notas" />
            <div class="nvm__scroll" data-id="lista-notas"></div>
          </div>
        </section>
      </aside>

      <section class="marco marco--cian nvm__editor">
        <div class="marco__cabecera">
          <span>EDITOR</span>
          <span class="marco__etiqueta" data-id="guardado"></span>
        </div>
        <div class="marco__cuerpo nvm__editor-cuerpo">
          <div class="nvm__vacio" data-id="sin-nota">SELECCIONA O CREA UNA NOTA</div>
          <div class="nvm__ficha" data-id="ficha" hidden>
            <input class="nvm__titulo" data-id="titulo" type="text" maxlength="200" autocomplete="off" spellcheck="false" placeholder="TÍTULO" />
            <textarea class="nvm__cuerpo" data-id="cuerpo" spellcheck="false" placeholder="Escribe aquí…"></textarea>
            <div class="nvm__pie">
              <span data-id="cuenta">0 PALABRAS · 0 CARACTERES</span>
              <button class="nvm__btn nvm__btn--borrar" data-id="borrar" type="button">BORRAR</button>
            </div>
          </div>
        </div>
      </section>
    </div>`;

  const q = <T extends HTMLElement>(id: string) => cont.querySelector<T>(`[data-id="${id}"]`)!;
  const raiz = q("raiz");
  const sync = q("sync");
  const syncTexto = q("sync-texto");
  const btnSync = q<HTMLButtonElement>("sincronizar");
  const errorEl = q("error");
  const listaTareas = q("lista-tareas");
  const listaNotas = q("lista-notas");
  const contador = q("contador");
  const campoTarea = q<HTMLInputElement>("nueva-tarea");
  const buscar = q<HTMLInputElement>("buscar");
  const sinNota = q("sin-nota");
  const ficha = q("ficha");
  const titulo = q<HTMLInputElement>("titulo");
  const cuerpo = q<HTMLTextAreaElement>("cuerpo");
  const cuenta = q("cuenta");
  const guardado = q("guardado");
  const btnBorrar = q<HTMLButtonElement>("borrar");

  let vivo = true;
  let est: EstadoNotas = {
    tareas: [],
    notas: [],
    ultima_sync: null,
    sincronizando: false,
    error: null,
    carpeta_sync: null,
  };
  /** Nota abierta en el editor; `nueva` = aún no existe en el núcleo (no se guarda vacía). */
  let actual: Nota | null = null;
  let nueva = false;
  let temporizador = 0;
  let temporizadorBorrado = 0;
  let confirmando = false;
  const suscripciones: (() => void)[] = [];

  // --- Editor ---
  const pintarCuenta = () => {
    const t = cuerpo.value;
    const palabras = t.trim() ? t.trim().split(/\s+/).length : 0;
    cuenta.textContent = `${palabras} ${palabras === 1 ? "PALABRA" : "PALABRAS"} · ${t.length} CARACTERES`;
  };

  const reiniciarBorrado = () => {
    clearTimeout(temporizadorBorrado);
    confirmando = false;
    btnBorrar.classList.remove("nvm__btn--peligro");
    btnBorrar.textContent = "BORRAR";
  };

  /** Guarda lo escrito, si hay algo que guardar (misma lógica que volcarEditor del panel). */
  const volcar = (): Promise<void> => {
    clearTimeout(temporizador);
    if (!actual) return Promise.resolve();
    const t = titulo.value;
    const c = cuerpo.value;
    if (nueva && !t.trim() && !c.trim()) return Promise.resolve();
    if (t === actual.titulo && c === actual.cuerpo) {
      if (vivo) guardado.textContent = actual ? "GUARDADO" : "";
      return Promise.resolve();
    }
    actual = { ...actual, titulo: t, cuerpo: c };
    nueva = false;
    return guardarNota(actual)
      .then(() => {
        if (vivo && !temporizador) guardado.textContent = "GUARDADO";
      })
      .catch(() => {
        if (vivo) guardado.textContent = "ERROR AL GUARDAR";
      });
  };

  const mostrarEditor = () => {
    reiniciarBorrado();
    sinNota.hidden = !!actual;
    ficha.hidden = !actual;
    guardado.textContent = actual ? "GUARDADO" : "";
    if (!actual) return;
    titulo.value = actual.titulo;
    cuerpo.value = actual.cuerpo;
    pintarCuenta();
  };

  const abrir = (n: Nota, esNueva: boolean) => {
    volcar();
    temporizador = 0;
    actual = n;
    nueva = esNueva;
    mostrarEditor();
    pintarNotas();
    // En una columna la lista se pliega para dejar sitio al editor
    if (raiz.clientWidth < ANCHO_UNA_COLUMNA) plegar(true);
    (esNueva ? titulo : cuerpo).focus();
  };

  const nuevaNota = () =>
    abrir({ id: crypto.randomUUID(), titulo: "", cuerpo: "", creada: 0, modificada: 0, borrada: false }, true);

  const programar = () => {
    guardado.textContent = "GUARDANDO…";
    clearTimeout(temporizador);
    temporizador = window.setTimeout(() => {
      temporizador = 0;
      volcar();
    }, ANTIRREBOTE_MS);
  };
  titulo.addEventListener("input", programar);
  cuerpo.addEventListener("input", () => {
    pintarCuenta();
    programar();
  });

  btnBorrar.addEventListener("click", () => {
    if (!actual) return;
    if (!confirmando) {
      confirmando = true;
      btnBorrar.classList.add("nvm__btn--peligro");
      btnBorrar.textContent = "¿BORRAR?";
      // Si no se confirma, vuelve solo al estado normal
      temporizadorBorrado = window.setTimeout(reiniciarBorrado, 3000);
      return;
    }
    clearTimeout(temporizador);
    temporizador = 0;
    const id = actual.id;
    const existia = !nueva;
    actual = null;
    nueva = false;
    mostrarEditor();
    if (existia) void borrarNota(id).catch(() => {});
    pintarNotas();
  });
  btnBorrar.addEventListener("blur", reiniciarBorrado);

  // --- Lista de notas ---
  function pintarNotas() {
    const filtro = buscar.value.trim().toLowerCase();
    const notas = est.notas
      .filter((n) => !filtro || n.titulo.toLowerCase().includes(filtro) || n.cuerpo.toLowerCase().includes(filtro))
      .sort((a, b) => b.modificada - a.modificada);
    if (!notas.length) {
      listaNotas.innerHTML = `<div class="nvm__vacio-lista">${filtro ? "SIN RESULTADOS" : "SIN NOTAS"}</div>`;
      return;
    }
    listaNotas.innerHTML = notas
      .map(
        (n) => `
      <button class="nvm__nota ${actual && n.id === actual.id ? "nvm__nota--activa" : ""}" data-nota="${esc(n.id)}" type="button">
        <span class="nvm__nota-titulo">${esc(n.titulo.trim() || primeraLinea(n.cuerpo) || "SIN TÍTULO")}</span>
        <span class="nvm__nota-hace">${hace(n.modificada)}</span>
      </button>`,
      )
      .join("");
  }
  listaNotas.addEventListener("click", (e) => {
    const id = (e.target as HTMLElement).closest<HTMLElement>("[data-nota]")?.dataset.nota;
    const n = est.notas.find((x) => x.id === id);
    if (n && n.id !== actual?.id) abrir(n, false);
  });
  buscar.addEventListener("input", pintarNotas);
  q("nueva-nota").addEventListener("click", nuevaNota);

  // --- Tareas ---
  const guardarT = (t: Tarea) => void guardarTarea(t).catch(() => {});
  const siguienteOrden = () => est.tareas.reduce((m, t) => Math.max(m, t.orden), 0) + 1;

  const filaTarea = (t: Tarea) => `
    <li class="nvm__tarea ${t.hecha ? "nvm__tarea--hecha" : ""}" data-tarea="${esc(t.id)}">
      <button class="nvm__casilla" data-accion="marcar" type="button" aria-label="${t.hecha ? "Marcar como pendiente" : "Marcar como hecha"}" aria-pressed="${t.hecha}">${t.hecha ? MARCA : ""}</button>
      <span class="nvm__tarea-texto">${esc(t.texto)}</span>
      <button class="nvm__borrar-tarea" data-accion="borrar" type="button" title="Borrar" aria-label="Borrar tarea">✕</button>
    </li>`;

  const pintarTareas = () => {
    const pend = est.tareas.filter((t) => !t.hecha).sort((a, b) => a.orden - b.orden);
    const hechas = est.tareas.filter((t) => t.hecha).sort((a, b) => b.modificada - a.modificada);
    contador.textContent = `${dos(pend.length)} PENDIENTES`;
    listaTareas.innerHTML = est.tareas.length
      ? `<ul class="nvm__tareas">${[...pend, ...hechas].map(filaTarea).join("")}</ul>`
      : `<div class="nvm__vacio-lista">SIN TAREAS</div>`;
  };
  listaTareas.addEventListener("click", (e) => {
    const o = e.target as HTMLElement;
    const accion = o.closest<HTMLElement>("[data-accion]")?.dataset.accion;
    const t = est.tareas.find((x) => x.id === o.closest<HTMLElement>("[data-tarea]")?.dataset.tarea);
    if (!accion || !t) return;
    if (accion === "borrar") void borrarTarea(t.id).catch(() => {});
    else guardarT(t.hecha ? { ...t, hecha: false, orden: siguienteOrden() } : { ...t, hecha: true });
  });
  campoTarea.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    const texto = campoTarea.value.trim();
    if (!texto) return;
    campoTarea.value = "";
    guardarT({ id: crypto.randomUUID(), texto, hecha: false, orden: siguienteOrden(), creada: 0, modificada: 0, borrada: false });
  });

  // --- Sincronización ---
  const pintarSync = () => {
    sync.classList.remove("nvm__sync--local", "nvm__sync--girando", "nvm__sync--error", "nvm__sync--ok");
    errorEl.hidden = !est.error;
    errorEl.textContent = est.error ?? "";
    btnSync.hidden = !est.carpeta_sync;
    btnSync.disabled = est.sincronizando;
    if (!est.carpeta_sync) {
      sync.classList.add("nvm__sync--local");
      syncTexto.textContent = "SOLO LOCAL";
    } else if (est.sincronizando) {
      sync.classList.add("nvm__sync--girando");
      syncTexto.textContent = "SINCRONIZANDO";
    } else if (est.error) {
      sync.classList.add("nvm__sync--error");
      syncTexto.textContent = "ERROR";
    } else {
      sync.classList.add("nvm__sync--ok");
      syncTexto.textContent = est.ultima_sync ? hace(est.ultima_sync) : "PENDIENTE";
    }
  };
  btnSync.addEventListener("click", () => void sincronizarNotas().catch(() => {}));

  // --- Plegado de la lista (solo se ve en una columna) ---
  const btnPlegar = q<HTMLButtonElement>("plegar");
  function plegar(plegada: boolean) {
    raiz.classList.toggle("nvm--plegada", plegada);
    btnPlegar.setAttribute("aria-expanded", String(!plegada));
    btnPlegar.textContent = plegada ? "LISTA ▸" : "LISTA ▾";
  }
  btnPlegar.addEventListener("click", () => plegar(!raiz.classList.contains("nvm--plegada")));

  // --- Atajos ---
  const alTeclear = (e: KeyboardEvent) => {
    if (e.ctrlKey && !e.altKey && e.key.toLowerCase() === "n") {
      e.preventDefault();
      nuevaNota();
    } else if (e.ctrlKey && !e.altKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      plegar(false);
      buscar.focus();
      buscar.select();
    } else if (e.key === "Escape") {
      (document.activeElement as HTMLElement | null)?.blur();
    }
  };
  document.addEventListener("keydown", alTeclear);

  // --- Estado entrante ---
  const recibir = (s: EstadoNotas) => {
    if (!vivo) return;
    est = s;
    pintarSync();
    pintarTareas();
    pintarNotas();
    // Cambios de fuera (sincronización): solo se refresca el editor si no se está escribiendo,
    // para no pisar el texto en curso.
    if (!actual || nueva || temporizador) return;
    const n = est.notas.find((x) => x.id === actual!.id);
    if (!n) {
      if (!ficha.contains(document.activeElement)) {
        actual = null;
        mostrarEditor();
      }
    } else if (!ficha.contains(document.activeElement) && (n.titulo !== titulo.value || n.cuerpo !== cuerpo.value)) {
      actual = n;
      mostrarEditor();
    }
  };

  const intervalo = window.setInterval(() => {
    pintarSync();
    pintarNotas(); // refresca los «HACE X»
  }, 30000);

  const limpiar = (): Promise<void> => {
    if (!vivo) return Promise.resolve();
    const guardando = volcar(); // que no se pierda lo escrito al cerrar la ventana
    vivo = false;
    clearInterval(intervalo);
    clearTimeout(temporizadorBorrado);
    document.removeEventListener("keydown", alTeclear);
    suscripciones.forEach((u) => u());
    return guardando;
  };

  alEvento<EstadoNotas>("notas://estado", recibir).then((un) => {
    if (vivo) suscripciones.push(un);
    else un();
  });
  pintarSync();
  pintarTareas();
  pintarNotas();
  estadoNotas().then(recibir).catch(() => {});

  return limpiar;
}
