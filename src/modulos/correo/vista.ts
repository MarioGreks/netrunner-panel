// Módulo CORREO: correos importantes (remitentes VIP y palabras clave) leídos del Outlook clásico.
import {
  abrirCorreo,
  abrirOutlook,
  alEvento,
  estadoCorreo,
  obtenerConfig,
  sincronizarCorreo,
  type Config,
  type CorreoImportante,
  type EstadoCorreo,
} from "../../lib/ipc";
import { abrirAjustes } from "../../lib/navegacion";
import { colorCategoria } from "../../lib/categorias";
import { dos, esc } from "../../lib/texto";

const MESES = ["ENE", "FEB", "MAR", "ABR", "MAY", "JUN", "JUL", "AGO", "SEP", "OCT", "NOV", "DIC"];

const ENGRANAJE = `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M10.5 2.5h3l.5 2.4 1.9.8 2.1-1.4 2.1 2.1-1.4 2.1.8 1.9 2.4.5v3l-2.4.5-.8 1.9 1.4 2.1-2.1 2.1-2.1-1.4-1.9.8-.5 2.4h-3l-.5-2.4-1.9-.8-2.1 1.4-2.1-2.1 1.4-2.1-.8-1.9-2.4-.5v-3l2.4-.5.8-1.9-1.4-2.1 2.1-2.1 2.1 1.4 1.9-.8z"/></svg>`;

/** Hora si es de hoy, «AYER» o «12 NOV». */
function cuando(iso: string, ahora = new Date()): string {
  const d = new Date(iso);
  const inicio = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const dif = Math.round((inicio(ahora) - inicio(d)) / 86400000);
  if (dif === 0) return `${dos(d.getHours())}:${dos(d.getMinutes())}`;
  if (dif === 1) return "AYER";
  return `${d.getDate()} ${MESES[d.getMonth()]}`;
}

function hace(iso: string | null): string {
  if (!iso) return "SIN LEER";
  const min = Math.floor((Date.now() - new Date(iso).getTime()) / 60000);
  if (min < 1) return "AHORA";
  if (min < 60) return `HACE ${min} MIN`;
  return `HACE ${Math.floor(min / 60)} H`;
}

export function montar(contenedor: HTMLElement): void {
  contenedor.innerHTML = `
    <div class="crr">
      <section class="marco marco--cian" data-id="marco">
        <div class="marco__cabecera">
          <span>CORREO</span>
          <span class="crr__der">
            <span class="crr__estado" data-id="estado"></span>
            <span class="marco__etiqueta" data-id="hace"></span>
            <button class="crr__icono" data-id="sync" type="button" title="Leer el correo ahora" aria-label="Leer el correo ahora"><span class="crr__giro" data-id="giro">⟳</span></button>
            <button class="crr__icono" data-id="engranaje" type="button" title="Ajustes del correo" aria-label="Abrir los ajustes del correo">${ENGRANAJE}</button>
          </span>
        </div>
        <div class="marco__cuerpo"><div class="crr__lista" data-id="lista"></div></div>
      </section>
    </div>`;

  const q = <T extends HTMLElement>(id: string) => contenedor.querySelector<T>(`[data-id="${id}"]`)!;
  const raiz = q("marco");
  const estadoEl = q("estado");
  const haceEl = q("hace");
  const lista = q("lista");
  const giro = q("giro");

  let vivo = true;
  let estado: EstadoCorreo | null = null;
  let config: Config | null = null;
  let abriendo = false;
  let lanzando = false;
  let girando = 0;

  q("engranaje").addEventListener("click", () => abrirAjustes("correo"));
  q("sync").addEventListener("click", () => {
    giro.classList.add("crr__giro--activo");
    clearTimeout(girando);
    girando = window.setTimeout(() => giro.classList.remove("crr__giro--activo"), 1500);
    void sincronizarCorreo();
  });

  const pintarCabecera = () => {
    const e = estado?.estado ?? "conectando";
    estadoEl.className = `crr__estado crr__estado--${e}`;
    estadoEl.textContent =
      e === "conectado" ? "● CONECTADO" : e === "cerrado" ? "○ OUTLOOK CERRADO" : e === "conectando" ? "◌ CONECTANDO…" : "▲ ERROR";
    estadoEl.title = e === "error" ? (estado?.detalle ?? "") : "";
    haceEl.textContent = e === "conectado" ? hace(estado?.ultima_sync ?? null) : "";
  };

  const vacio = (titulo: string, texto: string, boton?: { texto: string; accion: string; desactivado?: boolean }) => `
    <div class="crr__vacio">
      <span class="crr__vacio-t">${titulo}</span>
      ${texto ? `<span class="crr__vacio-p">${texto}</span>` : ""}
      ${boton ? `<button class="crr__btn" data-accion="${boton.accion}" type="button"${boton.desactivado ? " disabled" : ""}>${boton.texto}</button>` : ""}
    </div>`;

  const htmlCorreo = (c: CorreoImportante): string => `
    <button class="crr__item${c.no_leido ? " crr__item--nuevo" : ""}" type="button" data-id-correo="${esc(c.id)}" data-store="${esc(c.store_id)}" title="${esc(c.direccion)}">
      <span class="crr__fila">
        <span class="crr__cuenta" style="--cat:${colorCategoria(c.cuenta)}">${esc(c.cuenta)}</span>
        <span class="crr__rem">${c.no_leido ? `<i class="crr__punto" aria-label="Sin leer"></i>` : ""}<span class="crr__rem-t">${esc(c.remitente || c.direccion)}</span></span>
        <span class="crr__motivo">${esc(c.motivo)}</span>
        <span class="crr__hora">${cuando(c.recibido)}</span>
      </span>
      <span class="crr__asunto">${esc(c.asunto || "(sin asunto)")}</span>
      ${c.fragmento ? `<span class="crr__frag">${esc(c.fragmento)}</span>` : ""}
    </button>`;

  const pintarLista = () => {
    const e = estado;
    if (!e || e.estado === "conectando") {
      lista.innerHTML = vacio("CONECTANDO CON OUTLOOK…", "");
      return;
    }
    if (e.estado === "cerrado") {
      lista.innerHTML = config?.outlook_auto
        ? vacio("OUTLOOK ESTÁ CERRADO", "Ábrelo para ver tus correos importantes.", { texto: lanzando ? "ABRIENDO…" : "ABRIR OUTLOOK", accion: "abrir-outlook", desactivado: lanzando })
        : vacio("ABRE EL OUTLOOK CLÁSICO", "El arranque automático está desactivado.", { texto: lanzando ? "ABRIENDO…" : "ABRIR OUTLOOK", accion: "abrir-outlook", desactivado: lanzando });
      return;
    }
    const error = e.estado === "error" ? `<div class="crr__error">${esc(e.detalle || "No se pudo leer el correo")}</div>` : "";
    if (config && config.correo_vip.length === 0 && config.correo_palabras.length === 0) {
      lista.innerHTML =
        error +
        vacio("SIN FILTROS", "Añade remitentes VIP o palabras clave para ver aquí tus correos importantes.", {
          texto: "CONFIGURAR",
          accion: "configurar",
        });
      return;
    }
    if (e.correos.length === 0) {
      lista.innerHTML = error + vacio("NADA IMPORTANTE EN LOS ÚLTIMOS 14 DÍAS", "");
      return;
    }
    lista.innerHTML = error + e.correos.map(htmlCorreo).join("");
  };

  const pintar = () => {
    pintarCabecera();
    pintarLista();
  };

  lista.addEventListener("click", async (ev) => {
    const t = ev.target as HTMLElement;
    const accion = t.closest<HTMLElement>("[data-accion]")?.dataset.accion;
    if (accion === "configurar") return abrirAjustes("correo");
    if (accion === "abrir-outlook") {
      if (lanzando) return;
      lanzando = true;
      pintarLista();
      try {
        await abrirOutlook();
      } catch {
        /* el estado llega por el evento */
      } finally {
        lanzando = false;
        if (vivo) pintarLista();
      }
      return;
    }
    const item = t.closest<HTMLElement>(".crr__item");
    if (!item || abriendo) return;
    abriendo = true;
    item.classList.add("crr__item--abriendo");
    try {
      await abrirCorreo(item.dataset.idCorreo ?? "", item.dataset.store ?? "");
    } catch (e) {
      item.classList.remove("crr__item--abriendo");
      const aviso = document.createElement("div");
      aviso.className = "crr__error";
      aviso.textContent = typeof e === "string" ? e : "No se pudo abrir el correo";
      lista.prepend(aviso);
      window.setTimeout(() => aviso.remove(), 4000);
    } finally {
      abriendo = false;
    }
  });

  // --- Ciclo de vida ---
  const suscripciones: (() => void)[] = [];
  let intervalo = 0;
  const limpiar = () => {
    if (!vivo) return;
    vivo = false;
    clearInterval(intervalo);
    clearTimeout(girando);
    suscripciones.forEach((u) => u());
  };
  const comprobar = () => {
    if (!contenedor.isConnected || !contenedor.contains(raiz)) {
      limpiar();
      return false;
    }
    return true;
  };
  const recibir = (e: EstadoCorreo) => {
    if (!comprobar()) return;
    estado = e;
    giro.classList.remove("crr__giro--activo");
    // La configuración (filtros y arranque automático) decide qué mensaje vacío toca
    obtenerConfig()
      .then((c) => {
        if (!vivo) return;
        config = c;
        pintar();
      })
      .catch(() => pintar());
  };

  intervalo = window.setInterval(() => {
    if (comprobar()) pintarCabecera();
  }, 30000);
  void alEvento<EstadoCorreo>("correo://estado", recibir).then((un) => {
    if (vivo) suscripciones.push(un);
    else un();
  });
  void alEvento("panel://mostrar", () => vivo && pintar()).then((un) => {
    if (vivo) suscripciones.push(un);
    else un();
  });

  pintar();
  estadoCorreo().then(recibir).catch(() => {});
}
