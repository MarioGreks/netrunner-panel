// Módulo ACCESOS: control de la música que suena (SMTC) y cuadrícula de accesos directos.
import { open } from "@tauri-apps/plugin-dialog";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  abrirAcceso,
  alEvento,
  anadirAcceso,
  borrarAcceso,
  estadoAccesos,
  estadoMusica,
  musicaAlternar,
  musicaAnterior,
  musicaSiguiente,
  renombrarAcceso,
  reordenarAccesos,
  type Acceso,
  type EstadoMusica,
  type TipoAcceso,
} from "../../lib/ipc";
import { bloqueo } from "../../lib/bloqueo";
import { esc } from "../../lib/texto";


const SEGMENTOS = 28;

const ICO_ANT = `<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true"><path d="M5 5h2.4v14H5zM20 5v14L9 12z"/></svg>`;
const ICO_SIG = `<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true"><path d="M16.6 5H19v14h-2.4zM4 5l11 7L4 19z"/></svg>`;
const ICO_PLAY = `<svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true"><path d="M7 4.5v15L19.5 12z"/></svg>`;
const ICO_PAUSA = `<svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true"><path d="M6 5h4.2v14H6zM13.8 5H18v14h-4.2z"/></svg>`;
const DISCO = `<svg viewBox="0 0 64 64" width="100%" height="100%" fill="none" stroke="currentColor" stroke-width="1.2" aria-hidden="true"><circle cx="32" cy="32" r="26"/><circle cx="32" cy="32" r="19" opacity=".6"/><circle cx="32" cy="32" r="12" opacity=".6"/><circle cx="32" cy="32" r="5"/><path d="M32 6v8M32 50v8M6 32h8M50 32h8" opacity=".5"/></svg>`;
const GLOBO = `<svg viewBox="0 0 48 48" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><circle cx="24" cy="24" r="17"/><ellipse cx="24" cy="24" rx="7.5" ry="17"/><path d="M7 24h34M10 15h28M10 33h28"/></svg>`;
const GENERICO = `<svg viewBox="0 0 48 48" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M10 6h20l8 8v28H10z"/><path d="M30 6v8h8"/></svg>`;
const MAS = `<svg viewBox="0 0 48 48" width="26" height="26" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M24 10v28M10 24h28"/></svg>`;

const tiempo = (s: number) => {
  const t = Math.max(0, Math.floor(s));
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
};

export function montar(contenedor: HTMLElement): void {
  contenedor.innerHTML = `
    <div class="acc">
      <section class="marco marco--cian mus">
        <div class="marco__cabecera"><span>MÚSICA</span><span class="marco__etiqueta" data-id="mus-src">SRC: --</span></div>
        <div class="marco__cuerpo">
          <div class="mus__sinsenal" data-id="mus-nada">
            <svg class="mus__onda" viewBox="0 0 200 20" preserveAspectRatio="none" aria-hidden="true"><path d="M0 10H200"/></svg>
            <span>SIN SEÑAL</span>
          </div>
          <div class="mus__pista" data-id="mus-pista" hidden>
            <div class="mus__caratula" data-id="mus-caratula"></div>
            <div class="mus__datos">
              <div class="mus__titulo"><span data-id="mus-titulo"></span></div>
              <div class="mus__artista" data-id="mus-artista"></div>
              <div class="mus__album" data-id="mus-album"></div>
            </div>
          </div>
          <div class="mus__ctrl" data-id="mus-ctrl" hidden>
            <div class="mus__progreso">
              <div class="mus__barra" data-id="mus-barra">${"<i></i>".repeat(SEGMENTOS)}</div>
              <span class="mus__tiempo" data-id="mus-tiempo">0:00 / 0:00</span>
            </div>
            <div class="mus__botones">
              <button class="mus__btn" data-id="mus-ant" type="button" aria-label="Anterior">${ICO_ANT}</button>
              <button class="mus__btn mus__btn--grande" data-id="mus-play" type="button" aria-label="Reproducir o pausar">${ICO_PLAY}</button>
              <button class="mus__btn" data-id="mus-sig" type="button" aria-label="Siguiente">${ICO_SIG}</button>
            </div>
          </div>
        </div>
      </section>

      <section class="marco acc__marco" data-id="acc-marco">
        <div class="marco__cabecera"><span>ACCESOS</span><span class="marco__etiqueta" data-id="acc-cuenta">0</span></div>
        <div class="marco__cuerpo">
          <div class="acc__error" data-id="acc-error" hidden></div>
          <div class="acc__rejilla" data-id="acc-rejilla"></div>
          <div class="acc__ayuda" data-id="acc-ayuda" hidden>Sin accesos. Pulsa «+» para añadir una app, carpeta, archivo o URL.</div>
          <div class="acc__anadir" data-id="acc-anadir" hidden>
            <button class="acc__op" data-id="op-app" type="button">APP / ARCHIVO</button>
            <button class="acc__op" data-id="op-carpeta" type="button">CARPETA</button>
            <button class="acc__op" data-id="op-url" type="button">URL</button>
          </div>
          <div class="acc__urlfila" data-id="acc-urlfila" hidden>
            <input class="acc__input" data-id="acc-url" type="text" placeholder="https://…" spellcheck="false" autocomplete="off" />
            <button class="acc__op acc__op--ok" data-id="acc-url-ok" type="button">GUARDAR</button>
          </div>
        </div>
        <div class="acc__menu" data-id="acc-menu" hidden></div>
      </section>
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const raiz = q("acc-marco");
  const suscripciones: UnlistenFn[] = [];
  let vivo = true;
  let intervalo: number | undefined;

  const limpiar = () => {
    if (!vivo) return;
    vivo = false;
    clearInterval(intervalo);
    document.removeEventListener("pointerdown", cerrarMenuFuera, true);
    document.removeEventListener("keydown", teclaGlobal, true);
    suscripciones.forEach((u) => u());
  };
  const comprobar = () => {
    if (!contenedor.isConnected || !contenedor.contains(raiz)) {
      limpiar();
      return false;
    }
    return true;
  };

  // =========================================================================
  // MÚSICA
  // =========================================================================
  const nada = q("mus-nada");
  const pista = q("mus-pista");
  const ctrl = q("mus-ctrl");
  const src = q("mus-src");
  const caratula = q("mus-caratula");
  const tituloEl = q("mus-titulo");
  const barra = q("mus-barra");
  const segs = Array.from(barra.children) as HTMLElement[];
  const tiempoEl = q("mus-tiempo");
  const btnAnt = q<HTMLButtonElement>("mus-ant");
  const btnPlay = q<HTMLButtonElement>("mus-play");
  const btnSig = q<HTMLButtonElement>("mus-sig");

  let musica: EstadoMusica | null = null;
  let recibido = performance.now();
  let caratulaPintada: string | null | undefined;
  let claveTitulo = "";

  const posicionActual = () => {
    if (!musica) return 0;
    let p = musica.posicion;
    if (musica.reproduciendo) p += (performance.now() - recibido) / 1000;
    return musica.duracion > 0 ? Math.min(p, musica.duracion) : p;
  };

  const pintarProgreso = () => {
    if (!musica) return;
    const p = posicionActual();
    const f = musica.duracion > 0 ? p / musica.duracion : 0;
    const llenos = Math.round(f * SEGMENTOS);
    segs.forEach((s, i) => s.classList.toggle("on", i < llenos));
    tiempoEl.textContent = musica.duracion > 0 ? `${tiempo(p)} / ${tiempo(musica.duracion)}` : tiempo(p);
  };

  const pintarMarquesina = () => {
    const caja = tituloEl.parentElement!;
    tituloEl.style.removeProperty("--desp");
    tituloEl.classList.remove("mus__titulo--corre");
    const sobra = tituloEl.scrollWidth - caja.clientWidth;
    if (sobra > 2) {
      tituloEl.style.setProperty("--desp", `-${sobra + 12}px`);
      tituloEl.style.setProperty("--dur", `${Math.max(6, (sobra + 12) / 25)}s`);
      tituloEl.classList.add("mus__titulo--corre");
    }
  };

  const pintarMusica = () => {
    const m = musica;
    nada.hidden = !!m;
    pista.hidden = !m;
    ctrl.hidden = !m;
    src.textContent = m ? `SRC: ${m.origen}` : "SRC: --";
    if (!m) {
      caratulaPintada = undefined;
      claveTitulo = "";
      return;
    }
    if (caratulaPintada !== m.caratula) {
      caratulaPintada = m.caratula;
      caratula.innerHTML = m.caratula
        ? `<img src="${esc(m.caratula)}" alt="" draggable="false" /><span class="mus__velo"></span>`
        : `<span class="mus__disco">${DISCO}</span><span class="mus__velo"></span>`;
    }
    const clave = `${m.titulo}\u0001${m.artista}\u0001${m.album}`;
    if (clave !== claveTitulo) {
      claveTitulo = clave;
      tituloEl.textContent = m.titulo || "SIN TÍTULO";
      q("mus-artista").textContent = m.artista;
      q("mus-album").textContent = m.album;
      requestAnimationFrame(pintarMarquesina);
    }
    btnAnt.disabled = !m.puede_anterior;
    btnSig.disabled = !m.puede_siguiente;
    btnPlay.innerHTML = m.reproduciendo ? ICO_PAUSA : ICO_PLAY;
    pintarProgreso();
  };

  const recibirMusica = (m: EstadoMusica | null) => {
    if (!comprobar()) return;
    musica = m;
    recibido = performance.now();
    pintarMusica();
  };

  btnPlay.addEventListener("click", () => void musicaAlternar().catch(() => {}));
  btnAnt.addEventListener("click", () => void musicaAnterior().catch(() => {}));
  btnSig.addEventListener("click", () => void musicaSiguiente().catch(() => {}));

  // =========================================================================
  // ACCESOS
  // =========================================================================
  const rejilla = q("acc-rejilla");
  const ayuda = q("acc-ayuda");
  const panelAnadir = q("acc-anadir");
  const urlFila = q("acc-urlfila");
  const urlInput = q<HTMLInputElement>("acc-url");
  const menu = q("acc-menu");
  const errorEl = q("acc-error");
  const cuenta = q("acc-cuenta");

  let accesos: Acceso[] = [];
  let arrastrando = false;
  let editando = false;
  let menuId: string | null = null;
  let temporizadorError: number | undefined;

  const mostrarError = (texto: string) => {
    errorEl.textContent = texto;
    errorEl.hidden = false;
    clearTimeout(temporizadorError);
    temporizadorError = window.setTimeout(() => (errorEl.hidden = true), 4000);
  };
  const mensaje = (e: unknown) => (typeof e === "string" ? e : "No se pudo completar la acción");

  const cerrarMenu = () => {
    menuId = null;
    menu.hidden = true;
    menu.replaceChildren();
  };

  const casillaHtml = (a: Acceso) => {
    const icono = a.icono
      ? `<img src="${esc(a.icono)}" alt="" draggable="false" />`
      : a.tipo === "url"
        ? GLOBO
        : GENERICO;
    return `<button class="acc__casilla" type="button" data-id-acceso="${esc(a.id)}" title="${esc(a.nombre)}">
      <span class="acc__icono${a.icono ? "" : " acc__icono--vector"}">${icono}</span>
      <span class="acc__nombre">${esc(a.nombre)}</span>
    </button>`;
  };

  const pintarAccesos = () => {
    if (editando || arrastrando) return;
    cuenta.textContent = String(accesos.length).padStart(2, "0");
    rejilla.innerHTML =
      accesos.map(casillaHtml).join("") +
      `<button class="acc__casilla acc__casilla--mas" data-id="acc-mas" type="button" aria-label="Añadir acceso">${MAS}</button>`;
    ayuda.hidden = accesos.length > 0;
  };

  const alternarAnadir = (forzar?: boolean) => {
    const abrir = forzar ?? panelAnadir.hidden;
    panelAnadir.hidden = !abrir;
    if (!abrir) urlFila.hidden = true;
  };

  // --- Abrir, menú contextual y reordenado ---
  const casillaDe = (el: EventTarget | null) =>
    (el as HTMLElement | null)?.closest<HTMLElement>("[data-id-acceso]") ?? null;

  rejilla.addEventListener("click", (e) => {
    if (suprimirClic) {
      suprimirClic = false;
      return;
    }
    if ((e.target as HTMLElement).closest('[data-id="acc-mas"]')) {
      cerrarMenu();
      alternarAnadir();
      return;
    }
    const c = casillaDe(e.target);
    if (!c || c.querySelector("input")) return;
    cerrarMenu();
    abrirAcceso(c.dataset.idAcceso!).catch((err) => mostrarError(mensaje(err)));
  });

  const abrirMenu = (c: HTMLElement, x: number, y: number) => {
    menuId = c.dataset.idAcceso!;
    menu.innerHTML = `
      <button type="button" data-acc="renombrar">RENOMBRAR</button>
      <button type="button" data-acc="borrar" class="acc__menu-peligro">BORRAR</button>`;
    menu.hidden = false;
    const caja = raiz.getBoundingClientRect();
    const ancho = menu.offsetWidth;
    const izq = Math.max(4, Math.min(x - caja.left, caja.width - ancho - 4));
    menu.style.left = `${izq}px`;
    menu.style.top = `${Math.max(4, y - caja.top)}px`;
  };

  rejilla.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    const c = casillaDe(e.target);
    if (!c) return cerrarMenu();
    abrirMenu(c, e.clientX, e.clientY);
  });

  menu.addEventListener("click", (e) => {
    const b = (e.target as HTMLElement).closest<HTMLElement>("button");
    if (!b || !menuId) return;
    const id = menuId;
    switch (b.dataset.acc) {
      case "renombrar":
        cerrarMenu();
        empezarRenombrar(id);
        break;
      case "borrar":
        menu.innerHTML = `
          <span class="acc__menu-pregunta">¿BORRAR?</span>
          <button type="button" data-acc="si" class="acc__menu-peligro">SÍ</button>
          <button type="button" data-acc="no">NO</button>`;
        break;
      case "si":
        cerrarMenu();
        borrarAcceso(id).catch((err) => mostrarError(mensaje(err)));
        break;
      case "no":
        cerrarMenu();
        break;
    }
  });

  function cerrarMenuFuera(e: PointerEvent) {
    if (!menu.hidden && !menu.contains(e.target as Node)) cerrarMenu();
  }
  function teclaGlobal(e: KeyboardEvent) {
    if (e.key === "Escape" && !menu.hidden) {
      e.stopPropagation();
      cerrarMenu();
    }
  }
  document.addEventListener("pointerdown", cerrarMenuFuera, true);
  document.addEventListener("keydown", teclaGlobal, true);

  const empezarRenombrar = (id: string) => {
    const c = rejilla.querySelector<HTMLElement>(`[data-id-acceso="${CSS.escape(id)}"]`);
    const a = accesos.find((x) => x.id === id);
    if (!c || !a) return;
    editando = true;
    const nombre = c.querySelector<HTMLElement>(".acc__nombre")!;
    const input = document.createElement("input");
    input.className = "acc__input acc__input--nombre";
    input.value = a.nombre;
    input.maxLength = 60;
    input.spellcheck = false;
    nombre.replaceChildren(input);
    input.focus();
    input.select();
    let hecho = false;
    const terminar = (guardar: boolean) => {
      if (hecho) return;
      hecho = true;
      editando = false;
      const nuevo = input.value.trim();
      if (guardar && nuevo && nuevo !== a.nombre) {
        renombrarAcceso(id, nuevo).catch((err) => {
          mostrarError(mensaje(err));
          pintarAccesos();
        });
      } else {
        pintarAccesos();
      }
    };
    input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") terminar(true);
      else if (e.key === "Escape") terminar(false);
    });
    input.addEventListener("blur", () => terminar(true));
    input.addEventListener("click", (e) => e.stopPropagation());
  };

  // Reordenar arrastrando (con eventos de puntero: el arrastre HTML5 choca con la
  // captura de archivos de la ventana en Windows)
  let suprimirClic = false;
  rejilla.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const origen = casillaDe(e.target);
    if (!origen || editando) return;
    const x0 = e.clientX;
    const y0 = e.clientY;
    let activo = false;

    const mover = (ev: PointerEvent) => {
      if (!activo) {
        if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < 6) return;
        activo = true;
        arrastrando = true;
        cerrarMenu();
        origen.classList.add("acc__casilla--arrastrada");
      }
      const bajo = document.elementsFromPoint(ev.clientX, ev.clientY);
      const objetivo = bajo.map((el) => casillaDe(el)).find((c) => c && c !== origen) ?? null;
      if (objetivo) {
        const hijos = Array.from(rejilla.children);
        const iO = hijos.indexOf(origen);
        const iT = hijos.indexOf(objetivo);
        if (iO < iT) objetivo.after(origen);
        else objetivo.before(origen);
      }
    };

    const soltar = () => {
      document.removeEventListener("pointermove", mover);
      document.removeEventListener("pointerup", soltar);
      document.removeEventListener("pointercancel", soltar);
      if (!activo) return;
      suprimirClic = true;
      setTimeout(() => (suprimirClic = false), 0);
      origen.classList.remove("acc__casilla--arrastrada");
      const ids = Array.from(rejilla.querySelectorAll<HTMLElement>("[data-id-acceso]")).map((c) => c.dataset.idAcceso!);
      arrastrando = false;
      const igual = ids.length === accesos.length && ids.every((id, i) => id === accesos[i].id);
      if (igual) return;
      // Orden local inmediato; el evento del backend lo confirmará
      const por = new Map(accesos.map((a) => [a.id, a]));
      accesos = ids.map((id) => por.get(id)!).filter(Boolean);
      reordenarAccesos(ids).catch((err) => {
        mostrarError(mensaje(err));
        void estadoAccesos().then(recibirAccesos);
      });
    };
    document.addEventListener("pointermove", mover);
    document.addEventListener("pointerup", soltar);
    document.addEventListener("pointercancel", soltar);
  });

  // --- Añadir ---
  const anadir = async (destino: string, tipo: TipoAcceso) => {
    try {
      await anadirAcceso(destino, tipo);
      alternarAnadir(false);
      urlInput.value = "";
    } catch (e) {
      mostrarError(mensaje(e));
    }
  };

  const elegir = async (directorio: boolean) => {
    try {
      // El selector nativo roba el foco: se avisa para que el panel no se oculte
      bloqueo.dialogoAbierto = true;
      const r = await open(
        directorio
          ? { directory: true, multiple: false, title: "Elige una carpeta" }
          : {
              multiple: false,
              title: "Elige una app o un archivo",
              filters: [
                { name: "Aplicaciones y accesos directos", extensions: ["exe", "lnk"] },
                { name: "Todos los archivos", extensions: ["*"] },
              ],
            },
      );
      bloqueo.dialogoAbierto = false;
      if (typeof r !== "string" || !r) return;
      const tipo: TipoAcceso = directorio ? "carpeta" : /\.(exe|lnk)$/i.test(r) ? "app" : "archivo";
      await anadir(r, tipo);
    } catch {
      mostrarError("No se pudo abrir el selector");
    } finally {
      bloqueo.dialogoAbierto = false;
    }
  };

  q("op-app").addEventListener("click", () => void elegir(false));
  q("op-carpeta").addEventListener("click", () => void elegir(true));
  q("op-url").addEventListener("click", () => {
    urlFila.hidden = !urlFila.hidden;
    if (!urlFila.hidden) urlInput.focus();
  });
  const guardarUrl = () => {
    const v = urlInput.value.trim();
    if (v) void anadir(v, "url");
  };
  q("acc-url-ok").addEventListener("click", guardarUrl);
  urlInput.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") guardarUrl();
    else if (e.key === "Escape") {
      urlFila.hidden = true;
      urlInput.value = "";
    }
  });

  const recibirAccesos = (l: Acceso[]) => {
    if (!comprobar()) return;
    accesos = [...l].sort((a, b) => a.orden - b.orden);
    pintarAccesos();
  };

  // --- Arranque ---
  intervalo = window.setInterval(() => {
    if (!comprobar()) return;
    pintarProgreso();
  }, 1000);

  void alEvento<EstadoMusica | null>("musica://estado", recibirMusica).then((u) => suscripciones.push(u));
  void alEvento<Acceso[]>("accesos://estado", recibirAccesos).then((u) => suscripciones.push(u));
  void estadoMusica().then(recibirMusica).catch(() => {});
  void estadoAccesos().then(recibirAccesos).catch(() => mostrarError("No se pudieron leer los accesos"));
  pintarAccesos();
  pintarMusica();
}
