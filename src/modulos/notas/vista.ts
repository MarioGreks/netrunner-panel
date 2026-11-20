// Módulo NOTAS: tareas y notas rápidas, guardadas en local con sincronización
// opcional con una carpeta de OneDrive.
import {
  alEvento,
  borrarNota,
  borrarTarea,
  estadoNotas,
  guardarNota,
  guardarTarea,
  type EstadoNotas,
  type Nota,
  type Tarea,
} from "../../lib/ipc";
import { abrirAjustes } from "../../lib/navegacion";
import { hacerDesacoplable } from "../../ui/desacoplar";

const esc = (t: string) =>
  t.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
const dos = (n: number) => String(n).padStart(2, "0");

const ENGRANAJE = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M10.5 2.5h3l.5 2.4 1.9.8 2.1-1.4 2.1 2.1-1.4 2.1.8 1.9 2.4.5v3l-2.4.5-.8 1.9 1.4 2.1-2.1 2.1-2.1-1.4-1.9.8-.5 2.4h-3l-.5-2.4-1.9-.8-2.1 1.4-2.1-2.1 1.4-2.1-.8-1.9-2.4-.5v-3l2.4-.5.8-1.9-1.4-2.1 2.1-2.1 2.1 1.4 1.9-.8z"/></svg>`;
const NUBE = `<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true"><path d="M7 18h10a4 4 0 0 0 .5-7.97A5.5 5.5 0 0 0 7 9.5 4.25 4.25 0 0 0 7 18z"/><path data-nube="marca" d="M12 15v-4M10 13l2-2 2 2"/></svg>`;
const MARCA = `<svg viewBox="0 0 12 12" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M2 6.5l2.6 2.6L10 3.2"/></svg>`;

/** «HACE 5 MIN», «HACE 2 H», «HACE 3 D» a partir de milisegundos. */
function hace(ms: number): string {
  const min = Math.floor((Date.now() - ms) / 60000);
  if (min < 1) return "AHORA";
  if (min < 60) return `HACE ${min} MIN`;
  if (min < 1440) return `HACE ${Math.floor(min / 60)} H`;
  return `HACE ${Math.floor(min / 1440)} D`;
}

const primeraLinea = (cuerpo: string) => cuerpo.split("\n").find((l) => l.trim() !== "")?.trim() ?? "";

export function montar(contenedor: HTMLElement): void {
  contenedor.innerHTML = `
    <div class="nts">
      <div class="nts__cab">
        <div class="nts__segmentos" role="tablist">
          <button class="nts__seg nts__seg--activo" data-id="seg-tareas" type="button" role="tab">TAREAS</button>
          <button class="nts__seg" data-id="seg-notas" type="button" role="tab">NOTAS</button>
        </div>
        <span class="nts__sync" data-id="sync">${NUBE}<span data-id="sync-texto"></span></span>
        <button class="nts__engranaje" data-id="engranaje" type="button" title="Ajustes de sincronización" aria-label="Abrir los ajustes">${ENGRANAJE}</button>
      </div>

      <div class="nts__error" data-id="error" hidden></div>
      <div class="nts__vista" data-id="vista"></div>
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const raiz = q("vista");
  const segTareas = q("seg-tareas");
  const segNotas = q("seg-notas");
  const sync = q("sync");
  const syncTexto = q("sync-texto");
  const errorEl = q("error");

  let vivo = true;
  let est: EstadoNotas = {
    tareas: [],
    notas: [],
    ultima_sync: null,
    sincronizando: false,
    error: null,
    carpeta_sync: null,
  };
  let pestana: "tareas" | "notas" = "tareas";
  let completadasAbiertas = false;
  /** Tarea en edición en línea: mientras dure, no se repinta la lista. */
  let editandoLinea = false;
  /** Nota abierta en el editor (null = lista). */
  let editor: { nota: Nota; nueva: boolean; temporizador: number; titulo: HTMLInputElement; cuerpo: HTMLTextAreaElement } | null = null;
  let confirmandoBorrado: string | null = null;

  const suscripciones: (() => void)[] = [];
  let intervalo = 0;
  const limpiar = () => {
    if (!vivo) return;
    vivo = false;
    clearInterval(intervalo);
    if (editor) {
      clearTimeout(editor.temporizador);
      volcarEditor();
    }
    suscripciones.forEach((u) => u());
  };
  const comprobar = () => {
    if (!contenedor.isConnected || !contenedor.contains(raiz)) {
      limpiar();
      return false;
    }
    return true;
  };

  // --- Indicador de sincronización ---
  const pintarSync = () => {
    sync.classList.remove("nts__sync--local", "nts__sync--girando", "nts__sync--error", "nts__sync--ok");
    errorEl.hidden = !est.error;
    errorEl.textContent = est.error ?? "";
    if (!est.carpeta_sync) {
      sync.classList.add("nts__sync--local");
      syncTexto.textContent = "SOLO LOCAL";
      sync.title = "Las notas solo se guardan en este equipo";
    } else if (est.sincronizando) {
      sync.classList.add("nts__sync--girando");
      syncTexto.textContent = "SINCRONIZANDO";
      sync.title = "Sincronizando con OneDrive";
    } else if (est.error) {
      sync.classList.add("nts__sync--error");
      syncTexto.textContent = "ERROR";
      sync.title = est.error;
    } else {
      sync.classList.add("nts__sync--ok");
      syncTexto.textContent = est.ultima_sync ? hace(est.ultima_sync) : "PENDIENTE";
      sync.title = "Sincronizado con OneDrive";
    }
  };

  // --- TAREAS ---
  const guardarT = (t: Tarea) => void guardarTarea(t).catch(() => {});

  const pendientes = () => est.tareas.filter((t) => !t.hecha).sort((a, b) => a.orden - b.orden);
  const siguienteOrden = () => est.tareas.reduce((m, t) => Math.max(m, t.orden), 0) + 1;

  const filaTarea = (t: Tarea) => `
    <li class="nts__tarea ${t.hecha ? "nts__tarea--hecha" : ""}" data-tarea="${esc(t.id)}" ${t.hecha ? "" : 'draggable="true"'}>
      <button class="nts__casilla" data-accion="marcar" type="button" aria-label="${t.hecha ? "Marcar como pendiente" : "Marcar como hecha"}" aria-pressed="${t.hecha}">${t.hecha ? MARCA : ""}</button>
      <span class="nts__texto" data-accion="texto" title="Doble clic para editar">${esc(t.texto)}</span>
      <button class="nts__borrar" data-accion="borrar" type="button" title="Borrar" aria-label="Borrar tarea">✕</button>
    </li>`;

  const pintarListaTareas = () => {
    const lista = raiz.querySelector<HTMLElement>('[data-id="lista-tareas"]');
    if (!lista) return;
    if (editandoLinea) return; // se repintará al terminar la edición
    const pend = pendientes();
    const hechas = est.tareas.filter((t) => t.hecha).sort((a, b) => b.modificada - a.modificada);
    raiz.querySelector<HTMLElement>('[data-id="contador"]')!.textContent = `${dos(pend.length)} PENDIENTES`;
    lista.innerHTML =
      (pend.length
        ? `<ul class="nts__lista" data-id="pendientes">${pend.map(filaTarea).join("")}</ul>`
        : `<div class="nts__vacio">SIN TAREAS PENDIENTES</div>`) +
      (hechas.length
        ? `<button class="nts__plegable" data-accion="plegar" type="button" aria-expanded="${completadasAbiertas}">
             <span class="nts__flecha">${completadasAbiertas ? "▾" : "▸"}</span> COMPLETADAS (${hechas.length})
           </button>
           ${completadasAbiertas ? `<ul class="nts__lista nts__lista--hechas">${hechas.map(filaTarea).join("")}</ul>` : ""}`
        : "");
  };

  const montarTareas = () => {
    raiz.innerHTML = `
      <section class="marco nts__marco">
        <div class="marco__cabecera"><span>TAREAS</span><span class="marco__etiqueta" data-id="contador">00 PENDIENTES</span></div>
        <div class="marco__cuerpo">
          <div class="nts__nueva">
            <input class="nts__input" data-id="nueva" type="text" autocomplete="off" spellcheck="false" maxlength="500" placeholder="+ NUEVA TAREA_" />
          </div>
          <div data-id="lista-tareas"></div>
        </div>
      </section>`;
    hacerDesacoplable(raiz.querySelector<HTMLElement>(".marco")!, { tipo: "notas", titulo: "TAREAS" });
    const campo = raiz.querySelector<HTMLInputElement>('[data-id="nueva"]')!;
    campo.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      const texto = campo.value.trim();
      if (!texto) return;
      campo.value = "";
      guardarT({
        id: crypto.randomUUID(),
        texto,
        hecha: false,
        orden: siguienteOrden(),
        creada: 0,
        modificada: 0,
        borrada: false,
      });
    });
    pintarListaTareas();
  };

  const tareaDe = (el: Element | null): Tarea | undefined => {
    const id = el?.closest<HTMLElement>("[data-tarea]")?.dataset.tarea;
    return est.tareas.find((t) => t.id === id);
  };

  const editarEnLinea = (li: HTMLElement, t: Tarea) => {
    const span = li.querySelector<HTMLElement>(".nts__texto")!;
    editandoLinea = true;
    li.draggable = false;
    const input = document.createElement("input");
    input.className = "nts__input nts__input--linea";
    input.type = "text";
    input.maxLength = 500;
    input.value = t.texto;
    span.replaceWith(input);
    input.focus();
    input.select();
    let cerrado = false;
    const cerrar = (guardar: boolean) => {
      if (cerrado) return;
      cerrado = true;
      editandoLinea = false;
      const texto = input.value.trim();
      if (guardar && texto && texto !== t.texto) guardarT({ ...t, texto });
      pintarListaTareas();
    };
    input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") cerrar(true);
      else if (e.key === "Escape") cerrar(false);
    });
    input.addEventListener("blur", () => cerrar(true));
  };

  // Un solo gestor de clics y arrastre para toda la lista
  raiz.addEventListener("click", (e) => {
    const objetivo = e.target as HTMLElement;
    const accion = objetivo.closest<HTMLElement>("[data-accion]")?.dataset.accion;
    if (!accion) return;
    if (accion === "plegar") {
      completadasAbiertas = !completadasAbiertas;
      pintarListaTareas();
      return;
    }
    const t = tareaDe(objetivo);
    if (!t) return;
    if (accion === "borrar") {
      void borrarTarea(t.id).catch(() => {});
    } else if (accion === "marcar") {
      const li = objetivo.closest<HTMLElement>("[data-tarea]")!;
      if (!t.hecha) {
        // Animación de tachado antes de pasar a COMPLETADAS
        li.classList.add("nts__tarea--tachando");
        window.setTimeout(() => guardarT({ ...t, hecha: true }), 380);
      } else {
        guardarT({ ...t, hecha: false, orden: siguienteOrden() });
      }
    }
  });
  raiz.addEventListener("dblclick", (e) => {
    const objetivo = e.target as HTMLElement;
    if (!objetivo.closest('[data-accion="texto"]')) return;
    const t = tareaDe(objetivo);
    const li = objetivo.closest<HTMLElement>("[data-tarea]");
    if (t && li) editarEnLinea(li, t);
  });

  let arrastrando: string | null = null;
  raiz.addEventListener("dragstart", (e) => {
    const li = (e.target as HTMLElement).closest<HTMLElement>("li[data-tarea]");
    if (!li || li.draggable === false) return;
    arrastrando = li.dataset.tarea ?? null;
    e.dataTransfer?.setData("text/plain", arrastrando ?? "");
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
    li.classList.add("nts__tarea--arrastrada");
  });
  const limpiarMarcas = () =>
    raiz.querySelectorAll(".nts__tarea--antes, .nts__tarea--despues, .nts__tarea--arrastrada").forEach((el) =>
      el.classList.remove("nts__tarea--antes", "nts__tarea--despues", "nts__tarea--arrastrada"),
    );
  /** Fila sobre la que está el cursor y si cae en su mitad superior. */
  const destino = (e: DragEvent) => {
    const li = (e.target as HTMLElement).closest<HTMLElement>("li[data-tarea]");
    if (!li || li.dataset.tarea === arrastrando || !li.draggable) return null;
    const r = li.getBoundingClientRect();
    return { li, antes: e.clientY < r.top + r.height / 2 };
  };
  raiz.addEventListener("dragover", (e) => {
    if (!arrastrando) return;
    const d = destino(e);
    if (!d) return;
    e.preventDefault();
    raiz.querySelectorAll(".nts__tarea--antes, .nts__tarea--despues").forEach((el) =>
      el.classList.remove("nts__tarea--antes", "nts__tarea--despues"),
    );
    d.li.classList.add(d.antes ? "nts__tarea--antes" : "nts__tarea--despues");
  });
  raiz.addEventListener("drop", (e) => {
    if (!arrastrando) return;
    const d = destino(e);
    const id = arrastrando;
    arrastrando = null;
    limpiarMarcas();
    if (!d) return;
    e.preventDefault();
    const orden = pendientes().filter((t) => t.id !== id);
    const movida = est.tareas.find((t) => t.id === id);
    if (!movida) return;
    let i = orden.findIndex((t) => t.id === d.li.dataset.tarea);
    if (!d.antes) i += 1;
    orden.splice(i, 0, movida);
    // Se renumera y solo se guardan las tareas cuyo orden cambia
    orden.forEach((t, k) => {
      if (t.orden !== k) guardarT({ ...t, orden: k });
    });
  });
  raiz.addEventListener("dragend", () => {
    arrastrando = null;
    limpiarMarcas();
  });

  // --- NOTAS ---
  const pintarListaNotas = () => {
    const lista = raiz.querySelector<HTMLElement>('[data-id="lista-notas"]');
    if (!lista) return;
    if (!est.notas.length) {
      lista.innerHTML = `<div class="nts__vacio">SIN NOTAS</div>`;
      return;
    }
    lista.innerHTML = est.notas
      .map((n) => {
        const borrando = confirmandoBorrado === n.id;
        return `
        <div class="nts__nota ${borrando ? "nts__nota--borrando" : ""}" data-nota="${esc(n.id)}">
          <button class="nts__nota-abrir" data-accion="abrir" type="button">
            <span class="nts__nota-titulo">${esc(n.titulo.trim() || "SIN TÍTULO")}</span>
            <span class="nts__nota-linea">${esc(primeraLinea(n.cuerpo)) || "&nbsp;"}</span>
            <span class="nts__nota-hace">${hace(n.modificada)}</span>
          </button>
          ${
            borrando
              ? `<span class="nts__confirmar">¿BORRAR?
                   <button class="nts__btn nts__btn--peligro" data-accion="borrar-si" type="button">SÍ</button>
                   <button class="nts__btn" data-accion="borrar-no" type="button">NO</button>
                 </span>`
              : `<button class="nts__borrar" data-accion="borrar-nota" type="button" title="Borrar" aria-label="Borrar nota">✕</button>`
          }
        </div>`;
      })
      .join("");
  };

  const montarNotas = () => {
    raiz.innerHTML = `
      <section class="marco nts__marco">
        <div class="marco__cabecera">
          <span>NOTAS</span>
          <button class="nts__btn nts__btn--ok" data-accion="nueva-nota" type="button">+ NOTA</button>
        </div>
        <div class="marco__cuerpo"><div data-id="lista-notas"></div></div>
      </section>`;
    hacerDesacoplable(raiz.querySelector<HTMLElement>(".marco")!, { tipo: "notas", titulo: "NOTAS" });
    pintarListaNotas();
  };

  const notaDe = (el: Element) =>
    est.notas.find((n) => n.id === el.closest<HTMLElement>("[data-nota]")?.dataset.nota);

  /** Guarda lo escrito en el editor, si hay algo que guardar. */
  function volcarEditor() {
    if (!editor) return;
    const { nota, titulo, cuerpo } = editor;
    const t = titulo.value;
    const c = cuerpo.value;
    if (editor.nueva && !t.trim() && !c.trim()) return;
    if (t === nota.titulo && c === nota.cuerpo) return;
    editor.nota = { ...nota, titulo: t, cuerpo: c };
    editor.nueva = false;
    void guardarNota(editor.nota).catch(() => {});
  }

  const abrirEditor = (nota: Nota, nueva: boolean) => {
    raiz.innerHTML = `
      <section class="marco nts__marco nts__editor">
        <div class="marco__cabecera">
          <button class="nts__btn" data-id="volver" type="button">← VOLVER</button>
          <span class="marco__etiqueta" data-id="guardado"></span>
        </div>
        <div class="marco__cuerpo nts__editor-cuerpo">
          <input class="nts__input nts__editor-titulo" data-id="titulo" type="text" maxlength="200" autocomplete="off" spellcheck="false" placeholder="TÍTULO" />
          <textarea class="nts__editor-texto" data-id="cuerpo" spellcheck="false" placeholder="Escribe aquí…"></textarea>
        </div>
      </section>`;
    const titulo = raiz.querySelector<HTMLInputElement>('[data-id="titulo"]')!;
    const cuerpo = raiz.querySelector<HTMLTextAreaElement>('[data-id="cuerpo"]')!;
    const guardado = raiz.querySelector<HTMLElement>('[data-id="guardado"]')!;
    titulo.value = nota.titulo;
    cuerpo.value = nota.cuerpo;
    editor = { nota, nueva, temporizador: 0, titulo, cuerpo };
    const programar = () => {
      if (!editor) return;
      guardado.textContent = "ESCRIBIENDO…";
      clearTimeout(editor.temporizador);
      editor.temporizador = window.setTimeout(() => {
        volcarEditor();
        guardado.textContent = "GUARDADO";
      }, 600);
    };
    titulo.addEventListener("input", programar);
    cuerpo.addEventListener("input", programar);
    raiz.querySelector('[data-id="volver"]')!.addEventListener("click", cerrarEditor);
    (nueva ? titulo : cuerpo).focus();
  };

  const cerrarEditor = () => {
    if (!editor) return;
    clearTimeout(editor.temporizador);
    volcarEditor();
    editor = null;
    pintar();
  };

  raiz.addEventListener("click", (e) => {
    const objetivo = e.target as HTMLElement;
    const accion = objetivo.closest<HTMLElement>("[data-accion]")?.dataset.accion;
    if (!accion) return;
    if (accion === "nueva-nota") {
      abrirEditor({ id: crypto.randomUUID(), titulo: "", cuerpo: "", creada: 0, modificada: 0, borrada: false }, true);
      return;
    }
    const n = notaDe(objetivo);
    if (!n) return;
    if (accion === "abrir") abrirEditor(n, false);
    else if (accion === "borrar-nota") {
      confirmandoBorrado = n.id;
      pintarListaNotas();
    } else if (accion === "borrar-no") {
      confirmandoBorrado = null;
      pintarListaNotas();
    } else if (accion === "borrar-si") {
      confirmandoBorrado = null;
      void borrarNota(n.id).catch(() => {});
    }
  });

  // --- Vista actual ---
  const pintar = () => {
    segTareas.classList.toggle("nts__seg--activo", pestana === "tareas");
    segNotas.classList.toggle("nts__seg--activo", pestana === "notas");
    if (pestana === "tareas") montarTareas();
    else montarNotas();
  };
  const cambiarPestana = (p: "tareas" | "notas") => {
    if (editor) {
      clearTimeout(editor.temporizador);
      volcarEditor();
      editor = null;
    }
    pestana = p;
    confirmandoBorrado = null;
    pintar();
  };
  segTareas.addEventListener("click", () => cambiarPestana("tareas"));
  segNotas.addEventListener("click", () => cambiarPestana("notas"));

  q("engranaje").addEventListener("click", () => abrirAjustes("notas"));

  // --- Estado entrante ---
  const recibir = (s: EstadoNotas) => {
    if (!comprobar()) return;
    est = s;
    pintarSync();
    if (editor) return; // el editor tiene su propio texto: no se toca
    if (pestana === "tareas") pintarListaTareas();
    else pintarListaNotas();
  };

  pintarSync();
  pintar();
  intervalo = window.setInterval(() => {
    if (!comprobar()) return;
    pintarSync();
  }, 30000);

  const suscribir = (p: Promise<() => void>) =>
    p.then((un) => {
      if (vivo) suscripciones.push(un);
      else un();
    });
  suscribir(alEvento<EstadoNotas>("notas://estado", recibir));
  suscribir(
    alEvento("panel://mostrar", () => {
      if (!vivo) return;
      pintarSync();
      if (!editor && pestana === "notas") pintarListaNotas();
    }),
  );
  estadoNotas().then(recibir).catch(() => {});
}
