// Apartado HISTÓRICO de la ventana de CONSUMO: controles (periodo, agrupar por, unidad, modo, paso),
// gráfico de líneas o áreas apiladas en canvas y tabla del ranking. Los números salen del comando
// `historico_consumo` (el núcleo agrupa y cachea); aquí solo se dibuja.
//
// REGLA DE RECURSOS
// - Pide datos al entrar, al cambiar un control y al llegar `consumo://archivo` (solo con el apartado
//   activo); nada de temporizadores ni bucles.
// - El gráfico solo se repinta con datos nuevos, al cambiar el tamaño o el tema, y al cambiar el
//   tramo bajo el ratón (mousemove, fundido en un único requestAnimationFrame por `crearLienzo`).
// - La vista se guarda solo si cambia (y el núcleo solo escribe si cambia lo guardado).
import {
  generarHistoricoConsumo,
  guardarVistaHistorico,
  historicoConsumo,
  vistaHistorico,
  type HistoricoConsumo,
  type PeriodoHistoricoConsumo,
  type UnidadHistoricoConsumo,
  type VistaHistorico,
} from "../../lib/ipc";
import type { Apartado, Cambio, Entorno } from "./apartados";
import { colorProyecto, conAlfa, temaGrafica, type TemaGrafica } from "./colores";
import { crearLienzo, fuente, lineaH, lineaV } from "./graficas";

// --- Opciones de los controles (alineadas con historico.rs) ---
const PERIODOS: [PeriodoHistoricoConsumo, string][] = [
  ["semana", "ESTA SEMANA"],
  ["semana_pasada", "SEMANA PASADA"],
  ["mes", "ESTE MES"],
  ["mes_pasado", "MES PASADO"],
  ["3meses", "3 MESES"],
  ["anio", "AÑO"],
  ["todo", "TODO"],
];
const GRUPOS: [VistaHistorico["grupo"], string][] = [
  ["proyecto", "PROYECTO"],
  ["equipo", "EQUIPO"],
  ["modelo", "MODELO"],
  ["tarea", "TAREA"],
];
const UNIDADES: [UnidadHistoricoConsumo, string][] = [
  ["eur", "€"],
  ["dias", "DÍAS"],
  ["usd", "$"],
  ["tokens", "TOKENS"],
  ["horas", "HORAS"],
  ["eur_h", "€/H"],
  ["pct_h", "%/H"],
  ["tok_h", "TOK/H"],
];
const MODOS: [VistaHistorico["modo"], string][] = [
  ["lineas", "LÍNEAS"],
  ["apilado", "APILADO"],
];
const PASOS: [VistaHistorico["paso"], string][] = [
  ["auto", "AUTO"],
  ["dia", "DÍA"],
  ["semana", "SEMANA"],
  ["mes", "MES"],
];
/** Cifras por hora: un cociente no se apila. */
const COCIENTES: UnidadHistoricoConsumo[] = ["eur_h", "pct_h", "tok_h"];
const SUFIJO: Record<UnidadHistoricoConsumo, string> = {
  eur: " €",
  dias: " d",
  usd: " $",
  tokens: " M",
  horas: " h",
  eur_h: " €/h",
  pct_h: " %/h",
  tok_h: " M/h",
};
const MAX_SERIES = 10;
const SIN_ATRIBUIR = "SIN ATRIBUIR";
const RESTO = "RESTO";

// --- Formato ---
const decimales = (v: number) => (Math.abs(v) >= 100 ? 0 : Math.abs(v) >= 10 ? 1 : 2);
const num = (v: number, dec = decimales(v)) => v.toLocaleString("es-ES", { maximumFractionDigits: dec });
const conUnidad = (v: number, u: UnidadHistoricoConsumo) => `${num(v)}${SUFIJO[u]}`;

// --- Datos del gráfico ---
interface SerieDib {
  nombre: string;
  color: string;
  /** Valores ya escalados; null = sin cifra. */
  v: (number | null)[];
  /** Línea discontinua (lo sin atribuir en líneas). */
  gris?: boolean;
}
interface Preparado {
  series: SerieDib[];
  apilado: boolean;
  /** El tramo de cada punto. */
  etiquetas: string[];
  unidad: UnidadHistoricoConsumo;
}

const colorDe = (clave: string, t: TemaGrafica) => (clave === "" || clave === "resto" ? t.otros : colorProyecto(clave, t));

/** Series a dibujar: las 10 mayores (en cifras sumables, el resto junto) y lo sin atribuir. */
function preparar(d: HistoricoConsumo, t: TemaGrafica): Preparado {
  const apilado = d.vista.modo === "apilado" && !COCIENTES.includes(d.unidad);
  const todas: SerieDib[] = d.series.map((s) => ({
    nombre: s.nombre.toUpperCase(),
    color: colorDe(s.clave, t),
    v: s.valores.map((x) => (x === null ? null : x * d.escala)),
  }));
  let series = todas;
  if (todas.length > MAX_SERIES) {
    series = todas.slice(0, MAX_SERIES);
    if (!COCIENTES.includes(d.unidad)) {
      const resto = todas.slice(MAX_SERIES);
      series.push({
        nombre: RESTO,
        color: t.resto,
        v: d.etiquetas.map((_, i) => resto.reduce((s, x) => s + (x.v[i] ?? 0), 0)),
      });
    }
  }
  if (d.vista.sin && d.sinAtribuir && d.sinAtribuir.some((x) => x > 0)) {
    series.push({ nombre: SIN_ATRIBUIR, color: t.otros, v: d.sinAtribuir.map((x) => x * d.escala), gris: true });
  }
  return { series, apilado, etiquetas: d.etiquetas, unidad: d.unidad };
}

// --- Geometría compartida entre el dibujo y el ratón ---
const PX0 = 46;
const PX1 = 12;
const PY0 = 10;
const PY1 = 24;
const xDe = (i: number, n: number, w: number) => (n <= 1 ? PX0 + (w - PX0 - PX1) / 2 : PX0 + (i / (n - 1)) * (w - PX0 - PX1));
/** El tramo bajo el ratón (−1 fuera del área). */
export function tramoBajo(x: number, n: number, w: number): number {
  const pw = w - PX0 - PX1;
  if (n < 1 || x < PX0 - 6 || x > PX0 + pw + 6) return -1;
  if (n === 1) return 0;
  return Math.max(0, Math.min(n - 1, Math.round(((x - PX0) / pw) * (n - 1))));
}

/** Paso «bonito» (1, 2, 5 × 10^k) para dividir `max` en unas 4 partes. */
function pasoBonito(max: number): number {
  const crudo = max / 4;
  const e = Math.pow(10, Math.floor(Math.log10(crudo)));
  const m = crudo / e;
  return (m <= 1 ? 1 : m <= 2 ? 2 : m <= 5 ? 5 : 10) * e;
}

function dibujarHistorico(p: Preparado, hover: number, c: CanvasRenderingContext2D, w: number, h: number, t: TemaGrafica) {
  const n = p.etiquetas.length;
  const pw = w - PX0 - PX1;
  const ph = h - PY0 - PY1;
  if (n === 0 || p.series.length === 0 || pw < 20 || ph < 20) return;

  // Alturas: apiladas (acumulado por tramo) o sueltas
  const bajo: number[][] = p.series.map(() => new Array<number>(n).fill(0));
  const alto: number[][] = p.series.map(() => new Array<number>(n).fill(0));
  const suma = new Array<number>(n).fill(0);
  p.series.forEach((s, k) => {
    for (let i = 0; i < n; i++) {
      const v = s.v[i] ?? 0;
      if (p.apilado) {
        bajo[k][i] = suma[i];
        suma[i] += v;
        alto[k][i] = suma[i];
      } else {
        alto[k][i] = v;
      }
    }
  });
  const tope = Math.max(0, ...(p.apilado ? suma : alto.flat()));
  const paso = pasoBonito(tope > 0 ? tope : 1);
  const ymax = Math.max(paso, Math.ceil(tope / paso - 1e-9) * paso);
  const Y = (v: number) => PY0 + ph - (v / ymax) * ph;
  const X = (i: number) => xDe(i, n, w);

  // Rejilla y eje Y
  c.font = fuente(9);
  c.textAlign = "right";
  c.textBaseline = "middle";
  for (let v = 0; v <= ymax + paso / 2; v += paso) {
    lineaH(c, PX0, PX0 + pw, Y(v), v === 0 ? t.rejillaFuerte : t.rejilla);
    c.fillStyle = t.tenue;
    c.fillText(num(v, v < 10 && paso < 1 ? 2 : paso < 10 ? 1 : 0), PX0 - 5, Y(v));
  }
  // Eje X: no más etiquetas de las que caben
  const caben = Math.max(1, Math.floor(pw / 62));
  const cada = Math.max(1, Math.ceil(n / caben));
  c.textAlign = "center";
  c.fillStyle = t.tenue;
  for (let i = 0; i < n; i += cada) {
    lineaV(c, X(i), PY0, PY0 + ph, t.rejilla);
    c.fillText(p.etiquetas[i], Math.min(Math.max(X(i), PX0 + 22), w - 22), PY0 + ph + 13);
  }

  // Series
  const barra = Math.min(34, pw / 3);
  p.series.forEach((s, k) => {
    if (n === 1) {
      const a = alto[k][0];
      const b = bajo[k][0];
      c.fillStyle = conAlfa(s.color, 0.7);
      c.fillRect(X(0) - barra / 2, Y(a), barra, Math.max(1, Y(b) - Y(a)));
      return;
    }
    if (p.apilado) {
      c.beginPath();
      for (let i = 0; i < n; i++) (i ? c.lineTo(X(i), Y(alto[k][i])) : c.moveTo(X(i), Y(alto[k][i])));
      for (let i = n - 1; i >= 0; i--) c.lineTo(X(i), Y(bajo[k][i]));
      c.closePath();
      c.fillStyle = conAlfa(s.color, s.gris ? 0.35 : 0.5);
      c.fill();
    }
    c.save();
    c.strokeStyle = s.color;
    c.lineWidth = 1.5;
    if (s.gris) c.setLineDash([4, 3]);
    c.beginPath();
    let abierto = false;
    for (let i = 0; i < n; i++) {
      if (!p.apilado && s.v[i] === null) {
        abierto = false; // sin cifra: la línea se corta
        continue;
      }
      if (abierto) c.lineTo(X(i), Y(alto[k][i]));
      else c.moveTo(X(i), Y(alto[k][i]));
      abierto = true;
    }
    c.stroke();
    c.restore();
  });

  // Tooltip: el valor de cada serie en el tramo bajo el ratón
  if (hover < 0 || hover >= n) return;
  lineaV(c, X(hover), PY0, PY0 + ph, t.rejillaFuerte);
  const filas = p.series
    .map((s) => ({ s, v: s.v[hover] }))
    .filter((f): f is { s: SerieDib; v: number } => f.v !== null && f.v !== 0)
    .slice(0, 9);
  for (const f of filas) {
    const k = p.series.indexOf(f.s);
    c.fillStyle = f.s.color;
    c.beginPath();
    c.arc(X(hover), Y(alto[k][hover]), 3, 0, Math.PI * 2);
    c.fill();
  }
  const lineas: [string | null, string, string][] = filas.map((f) => [f.s.color, f.s.nombre.slice(0, 18), conUnidad(f.v, p.unidad)]);
  if (p.apilado && filas.length > 1) lineas.push([null, "TOTAL", conUnidad(suma[hover], p.unidad)]);
  if (!lineas.length) lineas.push([null, "SIN CONSUMO", ""]);
  c.font = fuente(10);
  const anchoNombre = Math.max(...lineas.map((l) => c.measureText(l[1]).width));
  const anchoValor = Math.max(...lineas.map((l) => c.measureText(l[2]).width));
  const cab = p.etiquetas[hover];
  const caja = Math.max(c.measureText(cab).width, 14 + anchoNombre + 12 + anchoValor) + 16;
  const alt = 20 + lineas.length * 15;
  const xc = X(hover);
  const bx = xc + 12 + caja > w - 4 ? xc - 12 - caja : xc + 12;
  const by = Math.max(2, Math.min(PY0 + 4, h - alt - 2));
  c.fillStyle = t.fondo;
  c.fillRect(bx, by, caja, alt);
  c.strokeStyle = t.rejillaFuerte;
  c.strokeRect(bx + 0.5, by + 0.5, caja - 1, alt - 1);
  c.textAlign = "left";
  c.textBaseline = "middle";
  c.fillStyle = t.amarillo;
  c.fillText(cab, bx + 8, by + 11);
  lineas.forEach(([color, nombre, valor], j) => {
    const y = by + 26 + j * 15;
    if (color) {
      c.fillStyle = color;
      c.fillRect(bx + 8, y - 4, 8, 8);
    }
    c.fillStyle = color ? t.texto : t.tenue;
    c.fillText(nombre, bx + 22, y);
    c.textAlign = "right";
    c.fillText(valor, bx + caja - 8, y);
    c.textAlign = "left";
  });
}

// --- Apartado ---
function el<K extends keyof HTMLElementTagNameMap>(tag: K, clase?: string, texto?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (clase) e.className = clase;
  if (texto !== undefined) e.textContent = texto;
  return e;
}

type Control = "periodo" | "grupo" | "unidad" | "modo" | "paso";

export function montarHistorico(raiz: HTMLElement, e: Entorno): Apartado {
  raiz.innerHTML = `
    <div class="cv__col">
      <section class="marco marco--cian" data-id="vacio">
        <div class="marco__cabecera"><span>HISTÓRICO</span><span class="marco__etiqueta">ARCHIVO SEMANAL</span></div>
        <div class="marco__cuerpo"><p class="marco__vacio cv__proximamente" data-id="vacio-texto"></p></div>
      </section>
      <section class="marco marco--cian" data-id="controles" hidden></section>
      <section class="marco" data-id="grafico" hidden>
        <div class="marco__cabecera"><span data-id="titulo">HISTÓRICO</span><span class="marco__etiqueta" data-id="etiqueta"></span></div>
        <div class="marco__cuerpo">
          <div class="cv__lienzo cv__lienzo--hist"><canvas data-id="canvas"></canvas></div>
          <div class="cv__leyenda" data-id="leyenda"></div>
          <div class="cv__textos"><b class="cv__cifra" data-id="total"></b><span data-id="info"></span></div>
          <p class="rp__nota" data-id="nota" hidden></p>
        </div>
      </section>
      <section class="marco" data-id="tabla-marco" hidden>
        <div class="marco__cabecera"><span>RANKING</span><span class="marco__etiqueta" data-id="tabla-etiqueta"></span></div>
        <div class="marco__cuerpo"><div class="hs__tabla" data-id="tabla"></div></div>
      </section>
    </div>`;
  const q = <T extends HTMLElement>(id: string) => raiz.querySelector<T>(`[data-id="${id}"]`)!;
  const vacio = q("vacio");
  q("vacio-texto").textContent = "CALCULANDO…";
  const controles = q("controles");
  const grafico = q("grafico");
  const tablaMarco = q("tabla-marco");
  const canvas = q<HTMLCanvasElement>("canvas");
  const ley = q("leyenda");

  let vista: VistaHistorico | null = null;
  let datos: HistoricoConsumo | null = null;
  let preparado: Preparado | null = null;
  let hover = -1;
  let pedido = 0; // número de la última petición: las respuestas viejas se descartan
  let vivo = true;

  const lienzo = crearLienzo(
    canvas,
    (c, w, h, t) => {
      if (preparado) dibujarHistorico(preparado, hover, c, w, h, t);
    },
    e.activo,
  );

  // --- Tooltip: solo al cambiar de tramo ---
  canvas.addEventListener("mousemove", (ev) => {
    if (!preparado) return;
    const r = canvas.getBoundingClientRect();
    const i = tramoBajo(ev.clientX - r.left, preparado.etiquetas.length, r.width);
    if (i !== hover) {
      hover = i;
      lienzo.redibujar();
    }
  });
  canvas.addEventListener("mouseleave", () => {
    if (hover !== -1) {
      hover = -1;
      lienzo.redibujar();
    }
  });

  // --- Controles ---
  const grupoControl = (titulo: string, control: Control, opciones: [string, string][]) => {
    const g = el("div", "hs__grupo");
    g.appendChild(el("span", "hs__titulo", titulo));
    const seg = el("div", "hs__segmentos");
    seg.setAttribute("role", "radiogroup");
    seg.setAttribute("aria-label", titulo);
    for (const [valor, texto] of opciones) {
      const b = el("button", "hs__seg", texto);
      b.type = "button";
      b.dataset.control = control;
      b.dataset.valor = valor;
      b.setAttribute("role", "radio");
      seg.appendChild(b);
    }
    g.appendChild(seg);
    return g;
  };
  const fila = el("div", "hs__controles");
  fila.append(
    grupoControl("PERIODO", "periodo", PERIODOS),
    grupoControl("AGRUPAR POR", "grupo", GRUPOS),
    grupoControl("UNIDAD", "unidad", UNIDADES),
    grupoControl("MODO", "modo", MODOS),
    grupoControl("PASO", "paso", PASOS),
  );
  const exportar = el("div", "hs__grupo hs__grupo--exportar");
  exportar.appendChild(el("span", "hs__titulo", "INFORME"));
  const botonExportar = el("button", "hs__accion", "EXPORTAR HTML");
  botonExportar.type = "button";
  botonExportar.title = "Genera el histórico del periodo elegido, lo guarda en Descargas y lo abre en el navegador";
  exportar.appendChild(botonExportar);
  fila.appendChild(exportar);
  botonExportar.addEventListener("click", () => {
    if (!vista || botonExportar.disabled) return;
    botonExportar.disabled = true;
    e.mensaje("GENERANDO HISTÓRICO…");
    generarHistoricoConsumo({ periodo: vista.periodo, desde: vista.desde || undefined, hasta: vista.hasta || undefined })
      .then((t) => e.mensaje(t))
      .catch((err) => e.mensaje(typeof err === "string" ? err : "NO SE PUDO GENERAR EL HISTÓRICO"))
      .finally(() => {
        if (vivo) botonExportar.disabled = false;
      });
  });
  const cuerpoControles = el("div", "marco__cuerpo");
  cuerpoControles.appendChild(fila);
  const cabControles = el("div", "marco__cabecera");
  cabControles.append(el("span", undefined, "VISTA"), el("span", "marco__etiqueta", "SE RECUERDA"));
  controles.append(cabControles, cuerpoControles);

  const marcarControles = () => {
    fila.querySelectorAll<HTMLButtonElement>(".hs__seg").forEach((b) => {
      const activo = !!vista && (vista as unknown as Record<string, unknown>)[b.dataset.control!] === b.dataset.valor;
      b.classList.toggle("hs__seg--activo", activo);
      b.setAttribute("aria-checked", String(activo));
    });
  };

  fila.addEventListener("click", (ev) => {
    const b = (ev.target as HTMLElement).closest<HTMLButtonElement>(".hs__seg");
    if (!b || !vista) return;
    const control = b.dataset.control as Control;
    if ((vista as unknown as Record<string, unknown>)[control] === b.dataset.valor) return;
    vista = { ...vista, [control]: b.dataset.valor } as VistaHistorico;
    marcarControles();
    hover = -1;
    // Solo si cambia: el núcleo también comprueba antes de escribir
    void guardarVistaHistorico(vista).catch(() => {});
    pedir();
  });

  // --- Datos ---
  const pedir = () => {
    if (!vista || !e.activo()) return;
    const mio = ++pedido;
    historicoConsumo(vista)
      .then((d) => {
        if (!vivo || mio !== pedido) return;
        datos = d;
        vista = d.vista;
        pintar();
      })
      .catch(() => {});
  };

  const cargar = () => {
    if (vista) return pedir();
    void vistaHistorico()
      .then((v) => {
        if (!vivo) return;
        vista ??= v;
        marcarControles();
        pedir();
      })
      .catch(() => {});
  };

  const pintarLeyenda = (p: Preparado) => {
    const t = temaGrafica();
    const clave = `${t.cian}\u0001${p.series.map((s) => s.nombre + s.color).join("\u0001")}`;
    if (ley.dataset.clave === clave) return; // sin cambios: no se toca el DOM
    ley.dataset.clave = clave;
    ley.replaceChildren(
      ...p.series.map((s) => {
        const item = el("span", "cv__ley");
        const caja = el("i");
        caja.style.background = s.color;
        item.append(caja, el("span", undefined, s.nombre));
        return item;
      }),
    );
  };

  const pintarTabla = (d: HistoricoConsumo, t: TemaGrafica) => {
    const cont = q("tabla");
    const otras = d.filas[0]?.valores.map((v) => v.unidad).filter((u) => u !== d.unidad) ?? UNIDADES.map((u) => u[0]).filter((u) => u !== d.unidad);
    const etiqueta = (u: UnidadHistoricoConsumo) => UNIDADES.find((x) => x[0] === u)?.[1] ?? u;
    const tabla = el("table", "hs__tab");
    const cab = el("tr");
    cab.append(el("th", undefined, d.grupoNombre.toUpperCase()), el("th", "hs__num", `TOTAL ${etiqueta(d.unidad)}`), el("th", "hs__num", "%"));
    for (const u of otras) cab.appendChild(el("th", "hs__num hs__tenue", etiqueta(u)));
    tabla.appendChild(el("thead")).appendChild(cab);
    const cuerpo = el("tbody");
    const celda = (txt: string, clase = "hs__num") => el("td", clase, txt);
    for (const f of d.filas) {
      const tr = el("tr");
      const nombre = el("td", "hs__nombre");
      const punto = el("i", "rp__punto");
      punto.style.background = colorDe(f.clave, t);
      const txt = el("span", undefined, f.nombre.toUpperCase());
      txt.title = f.nombre;
      nombre.append(punto, txt);
      tr.append(nombre, celda(f.texto), celda(f.textoParticipacion));
      for (const u of otras) tr.appendChild(celda(f.valores.find((v) => v.unidad === u)?.texto ?? "—", "hs__num hs__tenue"));
      cuerpo.appendChild(tr);
    }
    // Lo sin atribuir (solo en € y días), siempre al final y en gris
    const sin = d.vista.sin ? d.sin.find((s) => s.unidad === d.unidad) : undefined;
    const total = d.total.find((s) => s.unidad === d.unidad)?.valor;
    if (sin && sin.valor && sin.valor > 0) {
      const tr = el("tr", "hs__fila--sin");
      const nombre = el("td", "hs__nombre");
      const punto = el("i", "rp__punto");
      punto.style.background = t.otros;
      nombre.append(punto, el("span", undefined, SIN_ATRIBUIR));
      const pct = total ? `${num((sin.valor * 100) / total, 1)} %` : "—";
      tr.append(nombre, celda(sin.texto), celda(pct));
      for (const u of otras) tr.appendChild(celda(d.sin.find((s) => s.unidad === u)?.texto ?? "—", "hs__num hs__tenue"));
      cuerpo.appendChild(tr);
    }
    tabla.appendChild(cuerpo);
    cont.replaceChildren(tabla);
  };

  const pintar = () => {
    marcarControles();
    const d = datos;
    const sinArchivo = !!d && d.archivos === 0;
    vacio.hidden = !!d && !sinArchivo;
    controles.hidden = grafico.hidden = tablaMarco.hidden = !d || sinArchivo;
    if (!d) return;
    if (sinArchivo) {
      q("vacio-texto").textContent = "AÚN NO HAY SEMANAS ARCHIVADAS · SE GUARDAN SOLAS CADA SEMANA";
      return;
    }
    const t = temaGrafica();
    q("titulo").textContent = `${d.grupoNombre.toUpperCase()} · ${d.unidadNombre.toUpperCase()}`;
    q("etiqueta").textContent = `${d.titulo.toUpperCase()} · POR ${d.paso === "dia" ? "DÍA" : d.paso === "semana" ? "SEMANA" : "MES"}`;
    q("tabla-etiqueta").textContent = `${d.filas.length} · ${d.semanas} SEM.`;
    if (!d.hayDatos) {
      preparado = null;
      ley.replaceChildren();
      delete ley.dataset.clave;
      q("total").textContent = "";
      q("info").textContent = "SIN DATOS EN ESTE PERIODO";
      q("nota").hidden = true;
      q("tabla").replaceChildren();
      lienzo.redibujar();
      return;
    }
    preparado = preparar(d, t);
    if (hover >= preparado.etiquetas.length) hover = -1;
    pintarLeyenda(preparado);
    const total = d.total.find((x) => x.unidad === d.unidad);
    q("total").textContent = total ? `TOTAL ${total.texto}` : "";
    const trozos = [`${d.semanas} SEMANAS`, d.desde && d.hasta ? `${d.desde} → ${d.hasta}` : "", d.maquinas.length ? d.maquinas.join(", ").toUpperCase() : ""];
    q("info").textContent = trozos.filter(Boolean).join(" · ");
    const nota = q("nota");
    nota.hidden = !(COCIENTES.includes(d.unidad) && d.vista.modo === "apilado");
    nota.textContent = "LAS CIFRAS POR HORA NO SE APILAN: SE DIBUJAN COMO LÍNEAS";
    pintarTabla(d, t);
    lienzo.redibujar();
  };

  return {
    pintar(c: Cambio) {
      if (c === "todo" || c === "archivo") cargar();
      // El tema o el tamaño los repinta el propio lienzo
    },
    destruir() {
      vivo = false;
      pedido++;
      lienzo.destruir();
      raiz.replaceChildren();
    },
  };
}
