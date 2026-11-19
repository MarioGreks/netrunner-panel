// Guía de CONSUMO (apartado «GUÍA» de la ventana flotante; botón «?» y tecla F1). Port de `guia.py`
// de la app Consumo, con los textos adaptados a netrunner.
//
// Cada sección es { titulo, lineas }. El texto es un markdown mínimo, una cosa por línea:
//   «## Título»  subtítulo · «- texto»  viñeta · cualquier otra línea, párrafo ·
//   «**negrita**», «*cursiva*» y «`código`» dentro de cualquier línea.
// Si cambia algo de la ventana (un apartado, un botón, un ajuste), cambia también aquí.
//
// REGLA DE RECURSOS: la guía no existe en el DOM hasta que se abre y se destruye al cerrarla.
// Se pinta con nodos DOM y `textContent` (nunca con innerHTML), así que no hay nada que escapar
// a mano: el texto no puede inyectar HTML.

export interface SeccionGuia {
  titulo: string;
  lineas: string[];
}

export const SECCIONES: SeccionGuia[] = [
  {
    titulo: "Qué es Consumo",
    lineas: [
      "CONSUMO te dice cuánto llevas gastado de tu **límite de Claude**, y en qué.",
      "## La pestaña del panel",
      "- En el panel de netrunner, la pestaña **CONSUMO** enseña los dos límites de tu cuenta, el **semanal** y el de **5 h**, cada uno con su anillo y la cuenta atrás hasta el reset, y debajo el **contexto** de las sesiones de Code que están vivas.",
      "- Su botón **ABRIR** abre la **ventana flotante** de CONSUMO: una ventana normal, con barra de título propia, que puedes mover, agrandar y minimizar, y que sale en la barra de tareas. Solo hay una: si ya está abierta, se trae al frente.",
      "## La ventana flotante",
      "- Arriba, la barra de título con **INFORMES**, **AJUSTES**, **?** (esta guía), minimizar y cerrar. Debajo, los **apartados**: **RESUMEN** (anillos y contexto), **RITMO**, **DÍAS**, **PROYECTOS**, **EQUIPOS**, **USO** e **HISTÓRICO**. Las flechas del teclado pasan de uno a otro.",
      "- En el pie, a la derecha, ves lo que ocupan los datos de CONSUMO en este equipo (en amarillo cerca del máximo que hayas puesto y en rojo al pasarlo).",
      "- Se recuerda dónde la dejaste. Con la ventana minimizada no se calcula ni se dibuja nada.",
      "## Qué mide",
      "Los límites de tu **cuenta** de Claude: el **semanal** y el de **5 horas**. Los lee del plugin `uso-consumo`, que solo los copia a un fichero de tu perfil; no envía nada a ningún sitio.",
    ],
  },
  {
    titulo: "Primeros pasos",
    lineas: [
      "Para que haya datos hacen falta tres cosas: **Claude**, el **plugin uso-consumo** y que Claude haya mandado algún dato.",
      "- Abre **AJUSTES → CONSUMO DE CLAUDE** (el botón **AJUSTES** de la barra de título te lleva directo) y pulsa **COMPROBAR**: te dice qué tienes y qué falta. Si el plugin falta o está roto, sale **INSTALAR / REPARAR PLUGIN**.",
      "- Después, **cierra Claude del todo, ábrelo y escribe un mensaje** en una sesión de Code: en unos segundos aparecen los datos.",
      "- Con una **API key** (sin plan de suscripción) no hay límites de plan que leer.",
      "- Si con Claude abierto no llega ningún dato, el RESUMEN enseña un aviso con lo que falta.",
    ],
  },
  {
    titulo: "Leer los anillos",
    lineas: [
      "## Los dos anillos",
      "En RESUMEN hay un anillo para el límite **semanal** y otro para el de **5 h**. Cada uno enseña el % ya gastado y se tiñe según cómo vas:",
      "- **Cian**: vas al ritmo esperado o por debajo.",
      "- **Amarillo**: vas por encima del ritmo esperado.",
      "- **Rojo**: hay riesgo de quedarte sin límite antes del reset.",
      "## Los textos",
      "- «**RESET**»: lo que falta para que se renueve la ventana. La cuenta atrás avanza sola mientras la ventana se ve.",
      "- El **ritmo esperado** es dónde estarías si gastaras de forma pareja hasta el reset; las proyecciones («hoy gastarás», «acabas en») salen de la media de tus días (o ventanas de 5 h) anteriores. Son estimaciones, no promesas.",
    ],
  },
  {
    titulo: "Los apartados",
    lineas: [
      "De izquierda a derecha en la barra de apartados:",
      "- **RESUMEN**: los anillos de los dos límites y el **contexto** de las sesiones de Code con actividad reciente (hasta 10). Cada una se llena según los tokens de su contexto sobre el máximo (200 k, 500 k o 1 M: se elige en **AJUSTES → CONSUMO DE CLAUDE → CONTEXTO MÁXIMO**) y pasa de un color a otro cuanto más se acerca al límite. Un clic abre el chat de esa sesión en la app de Claude.",
      "- **RITMO**: el % acumulado de la semana frente a lo esperado (la diagonal), con la proyección hasta el reset, y debajo el **consumo por hora**, acumulado en la semana actual con una capa por proyecto.",
      "- **DÍAS**: barras del % gastado cada día de la semana, apiladas por proyecto, con lo de hoy y lo de ayer, y el mismo consumo por hora.",
      "- **PROYECTOS**: una barra por proyecto, partida en sus sesiones (tonos de su color); la más larga es el proyecto que más gasta. El proyecto es el repositorio git donde se trabajó (aunque sea en una subcarpeta, en un subagente o en un worktree); lo que se hace fuera de cualquier repositorio cuenta en el repositorio donde empezó la sesión. Debajo, cómo se repartió la **semana anterior**.",
      "- **EQUIPOS**: lo que hizo cada uno de tus equipos y hace cuánto trabajó (en amarillo, el que lleva más de dos días parado).",
      "- **USO**: el consumo por **modelo** (una barra apilada) y por **tarea** (comandos, escribir código, explorar, mensajes…).",
      "- **HISTÓRICO**: el archivo de semanas pasadas (ver su sección).",
      "## Cómo se reparte",
      "El plugin solo da el % de tu cuenta, así que CONSUMO **estima** el reparto: lee las transcripciones de Claude Code de tus equipos (`~/.claude/projects`) y reparte cada subida del % entre lo que se hizo en ese intervalo, en proporción a su peso (tokens ponderados por precio y modelo). La fórmula real del límite no es pública: son estimaciones. Todos los repartos van sobre lo consumido (el gráfico entero es lo de ahora, 100 %).",
      "Lo que ningún equipo explica (claude.ai, el móvil, un equipo sin CONSUMO, transcripciones que Claude Code ya borró…) sale como **«sin atribuir»**. El único caso que no cuenta como sin atribuir es el primer salto de toda la historia (el % que ya llevaba la semana cuando se arrancó por primera vez a mitad de semana): se ignora al leer, sin tocar los archivos.",
      "## Colores",
      "El color de un proyecto sale de su nombre: es **siempre el mismo** en todos los gráficos, apartados e informes. Los tonos cambian con el tema del panel (AJUSTES). «Resto» y «sin atribuir» van en grises.",
      "## Leyendas",
      "Solo salen las entradas con peso suficiente; si hay más, las demás van juntas en «+N más» (pasa el ratón por encima para ver cuáles).",
    ],
  },
  {
    titulo: "Histórico",
    lineas: [
      "Cada equipo guarda por semana un archivo con el detalle (en la carpeta `historial` de la carpeta de sincronización, unos 15 KB). Claude Code borra las transcripciones antiguas; así los meses atrás siguen existiendo. Un periodo solo tiene datos desde que CONSUMO empezó a guardarlos. Se guarda solo, cada pocos minutos y solo si algo cambió.",
      "El apartado **HISTÓRICO** de la ventana flotante lo explora. Se calcula al abrirlo (y al llegar un archivo nuevo) y se pinta en cuanto llega. Los controles se recuerdan al cerrar:",
      "- **PERIODO**: *esta semana* y *semana pasada* (de reset a reset), *este mes*, *mes pasado*, *3 meses* (los últimos 90 días), *año* (el natural) y *todo*.",
      "- **AGRUPAR POR**: *proyecto*, *equipo*, *modelo* o *tarea* (una cada vez).",
      "- **UNIDAD**: **€** de coste real (el % del límite semanal sobre lo que cuesta esa semana: un mes son 52/12 semanas, así que con 18 € al mes el 100 % semanal son unos 4,15 €), **días** de límite (1 día = 1/7 del límite semanal), **$** a precio de API (solo una referencia), **tokens**, **horas** de tiempo activo y tres cifras por hora activa: **€/h**, **%/h** y **tok/h**. Los € por mes que pagas son el ajuste `suscripcion_eur` de `~/.claude/statusbar/consumo_ajustes.json` (por defecto 18).",
      "- **MODO**: *líneas* o *apilado*. Las cifras por hora no se apilan nunca: un cociente no se suma (se calculan con las sumas: suma del valor / suma de horas, y lo que no tiene horas no tiene cifra).",
      "- **PASO**: *auto* (por día si el periodo dura hasta 31 días, por semana de Claude hasta unas 26 semanas y por mes si es más), o a mano.",
      "- **Tiempo activo** suma los huecos entre mensajes seguidos de una sesión cuando duran **menos de 10 minutos** (una pausa más larga no cuenta); el trabajo de los subagentes cuenta como actividad de su sesión y dos en paralelo no suman el mismo minuto dos veces. Lo guardado antes de que existiera ese dato cuenta 0 h.",
      "- **Gráfico**: pasa el ratón y aparece una raya en el tramo más cercano con el valor de cada serie. *Sin atribuir* sale en gris, solo en € y días (en $, tokens y horas no existe).",
      "- **Ranking** (debajo): todas las filas de la agrupación en el periodo, ordenadas por la unidad, con su color, valor, % del total, las demás unidades, la primera y la última actividad y las semanas activas.",
      "- **EXPORTAR HTML**: genera el *Histórico total* solo del periodo elegido, con todas las agrupaciones y unidades, lo guarda en Descargas y lo abre en el navegador.",
    ],
  },
  {
    titulo: "Informes",
    lineas: [
      "## Resúmenes (HTML)",
      "El botón **INFORMES** de la barra de título tiene: **ESTA SEMANA**, **SEMANA PASADA**, **ESTE MES**, **MES PASADO**, **TRIMESTRE**, **SEMESTRE**, **AÑO** y **HISTÓRICO TOTAL**. El informe se genera en ese momento (nada se hace en segundo plano), se guarda en **Descargas** y se abre en el navegador. La línea de estado del pie dice cómo se llama el fichero.",
      "El resumen de una semana es exhaustivo: sesiones, detalle por día y mapa de horas. Los demás suman semanas. Llevan el consumo hora a hora por proyecto (en periodos largos, varias horas por columna), tablas de reparto por proyecto, equipo, modelo y tipo de tarea con el % del límite y los tokens, el **coste estimado a precio de API**, el ahorro de la caché de contexto y cuánto sale frente a tu suscripción (ajuste `suscripcion`, en $, de `consumo_ajustes.json`; por defecto 20).",
      "El HTML es un solo fichero: no depende de internet ni de nada instalado. Antes de generarlo se guarda en el archivo histórico lo de ahora, para que lleve lo último.",
      "## Histórico total",
      "Suma **todo el archivo histórico** de todos los equipos y lo reparte por **proyecto**, **equipo**, **modelo** o **tipo de tarea**, a tu elección en la propia página, en las mismas unidades del apartado HISTÓRICO. Trae una tabla por cada agrupación (con una columna fija de horas) y un gráfico de columnas por semana. Es una estimación y solo cubre desde que CONSUMO guarda el archivo.",
    ],
  },
  {
    titulo: "Varios equipos",
    lineas: [
      "Si usas Claude en varios equipos, el límite es **de la cuenta** pero el gasto es de todos.",
      "- Cada equipo publica sus ficheros `<PC>.consumo.json` y `<PC>.detalle.json` en una **carpeta compartida** (por defecto en `OneDrive\\Claude-consumo`) y CONSUMO suma los de todos.",
      "- Para usar otra carpeta: **AJUSTES → CONSUMO DE CLAUDE → CARPETA DE SINCRONIZACIÓN → ELEGIR** (**POR DEFECTO** vuelve a la de siempre). Hay que elegir la misma en cada equipo.",
      "- **Dos cuentas de OneDrive** (p. ej. la tuya y la de otra persona que comparte tu cuenta de Claude): un equipo solo puede tener un OneDrive, así que cada cuenta tiene su carpeta. Si hay más de una carpeta de sincronización, CONSUMO lee y publica en **todas** y pasa de una a otra lo que han publicado los demás equipos. Ojo: los ficheros llevan los nombres de tus proyectos y sesiones, y los verá quien tenga acceso a la carpeta.",
      "- Los informes juntan los archivos históricos de todas las carpetas.",
      "- Si dos equipos dan datos distintos para la misma ventana, gana el más reciente.",
      "- Mientras la app Consumo original siga abierta en este equipo, netrunner no escribe en el historial ni en el archivo (ya lo hace ella): solo lee.",
    ],
  },
  {
    titulo: "Avisos y espacio",
    lineas: [
      "Todo esto está en **AJUSTES → CONSUMO DE CLAUDE**:",
      "- **AVISOS AL 80 % Y AL 100 %**: una ventanita al llegar al 80 % y al 100 % del límite semanal o del de 5 horas (una vez por ventana).",
      "- **CONTEXTO MÁXIMO** (200 k, 500 k o 1 M): cambia el tamaño con el que se calcula el % de cada sesión de contexto.",
      "- **MÁXIMO DE ESPACIO (GB)**: lo que ocupan los datos de CONSUMO en este equipo (transcripciones de Claude, historial de la nube) se mide cada cierto tiempo y sale en el pie de la ventana. Con un máximo puesto (0 lo quita), el indicador va en amarillo desde el 80 % y en rojo al pasarlo. El máximo es de este equipo.",
      "- **PLUGIN**: **COMPROBAR** diagnostica y, si hace falta, **INSTALAR / REPARAR PLUGIN**.",
    ],
  },
  {
    titulo: "Problemas frecuentes",
    lineas: [
      "## No salen datos",
      "Abre **AJUSTES → CONSUMO DE CLAUDE** y pulsa **COMPROBAR**. Lo más habitual: falta el plugin (botón *INSTALAR / REPARAR PLUGIN*) o Claude aún no ha mandado nada (cierra Claude del todo, ábrelo y escribe un mensaje en una sesión de Code).",
      "## El plugin no carga («Path escapes plugin directory»)",
      "La app de Claude de la Tienda de Windows no carga plugins de AppData, así que el plugin se instala desde `~/.claude/consumo-mod`. Pulsa *INSTALAR / REPARAR PLUGIN*.",
      "## El histórico está vacío",
      "Un periodo solo tiene datos desde que CONSUMO empezó a guardar el archivo semanal. Si acabas de instalarlo, se va llenando semana a semana; lo anterior no se puede reconstruir porque Claude Code borra las transcripciones.",
      "## El informe no se abre",
      "El informe se guarda en **Descargas** aunque el navegador no llegue a abrirse: busca el fichero con el nombre que dice la línea de estado.",
      "## Algo no cuadra con los límites",
      "El % real de tu cuenta es el de los anillos. Los repartos por proyecto, modelo o tarea son estimaciones, y lo que no se puede explicar sale como «sin atribuir».",
    ],
  },
  {
    titulo: "Atajos",
    lineas: [
      "- **F1**: esta guía (otra vez F1, **Esc** o el botón **?** vuelven al apartado en que estabas).",
      "- **← / →**: pasan al apartado anterior o siguiente.",
      "- **Clic** en una sesión de contexto: abre el chat de esa sesión en la app de Claude.",
      "- **INFORMES**: abre el menú de informes; **Esc** o un clic fuera lo cierra.",
      "- **AJUSTES**: abre el panel en AJUSTES → CONSUMO DE CLAUDE.",
    ],
  },
];

// --- Markdown mínimo ---

export type EstiloTrozo = "" | "b" | "i" | "c";
export interface Trozo {
  texto: string;
  estilo: EstiloTrozo;
}
export type TipoLinea = "h" | "li" | "p";
export interface LineaGuia {
  tipo: TipoLinea;
  trozos: Trozo[];
}

/** Trozos de una línea: «**negrita**» (b), «*cursiva*» (i, los nombres de las opciones) o «`código`» (c). */
export function trozos(linea: string): Trozo[] {
  const out: Trozo[] = [];
  const re = /\*\*(.+?)\*\*|\*(.+?)\*|`(.+?)`/g;
  let pos = 0;
  for (let m = re.exec(linea); m; m = re.exec(linea)) {
    if (m.index > pos) out.push({ texto: linea.slice(pos, m.index), estilo: "" });
    out.push({ texto: m[1] ?? m[2] ?? m[3], estilo: m[1] !== undefined ? "b" : m[2] !== undefined ? "i" : "c" });
    pos = m.index + m[0].length;
  }
  if (pos < linea.length) out.push({ texto: linea.slice(pos), estilo: "" });
  return out;
}

/** Líneas de un texto: «## » subtítulo (h), «- » viñeta (li) o párrafo (p); las vacías no cuentan. */
export function lineas(texto: string[] | string): LineaGuia[] {
  const out: LineaGuia[] = [];
  for (const cruda of Array.isArray(texto) ? texto : texto.split(/\r?\n/)) {
    const l = cruda.trim();
    if (!l) continue;
    if (l.startsWith("## ")) out.push({ tipo: "h", trozos: trozos(l.slice(3)) });
    else if (l.startsWith("- ")) out.push({ tipo: "li", trozos: trozos(l.slice(2)) });
    else out.push({ tipo: "p", trozos: trozos(l) });
  }
  return out;
}

// --- Vista ---

const ETIQUETA_ESTILO: Record<EstiloTrozo, string> = { "": "", b: "strong", i: "em", c: "code" };

function el<K extends keyof HTMLElementTagNameMap>(tag: K, clase?: string, texto?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (clase) e.className = clase;
  if (texto !== undefined) e.textContent = texto;
  return e;
}

/** Pinta los trozos de una línea en `padre`: solo `textContent`, nunca HTML. */
function pintarTrozos(padre: HTMLElement, ts: Trozo[]) {
  for (const t of ts) {
    const tag = ETIQUETA_ESTILO[t.estilo];
    if (tag) padre.appendChild(el(tag as "strong" | "em" | "code", undefined, t.texto));
    else padre.appendChild(document.createTextNode(t.texto));
  }
}

export interface Guia {
  destruir(): void;
}

/**
 * Monta la guía en `raiz` (índice de secciones a la izquierda, texto a la derecha). `cerrar` se
 * llama con el botón VOLVER; Esc y F1 los gestiona la página.
 */
export function montarGuia(raiz: HTMLElement, cerrar: () => void): Guia {
  const caja = el("div", "gd");
  const indice = el("nav", "gd__indice");
  indice.setAttribute("aria-label", "Secciones de la guía");
  const cuerpo = el("article", "gd__texto");
  cuerpo.tabIndex = -1;
  const volver = el("button", "gd__volver", "← VOLVER");
  volver.type = "button";
  volver.addEventListener("click", cerrar);
  indice.appendChild(volver);
  const botones: HTMLButtonElement[] = [];
  SECCIONES.forEach((s, i) => {
    const b = el("button", "gd__enlace", s.titulo.toUpperCase());
    b.type = "button";
    b.addEventListener("click", () => mostrar(i));
    botones.push(b);
    indice.appendChild(b);
  });
  caja.append(indice, cuerpo);
  raiz.replaceChildren(caja);

  const mostrar = (i: number) => {
    botones.forEach((b, k) => {
      b.classList.toggle("gd__enlace--activo", k === i);
      if (k === i) b.setAttribute("aria-current", "true");
      else b.removeAttribute("aria-current");
    });
    const s = SECCIONES[i];
    cuerpo.replaceChildren(el("h2", "gd__titulo", s.titulo.toUpperCase()));
    let lista: HTMLUListElement | null = null;
    for (const l of lineas(s.lineas)) {
      if (l.tipo === "li") {
        if (!lista) {
          lista = el("ul", "gd__lista");
          cuerpo.appendChild(lista);
        }
        const li = el("li");
        pintarTrozos(li, l.trozos);
        lista.appendChild(li);
        continue;
      }
      lista = null;
      const e = el(l.tipo === "h" ? "h3" : "p", l.tipo === "h" ? "gd__sub" : "gd__p");
      pintarTrozos(e, l.trozos);
      cuerpo.appendChild(e);
    }
    cuerpo.scrollTop = 0;
  };
  mostrar(0);

  return {
    destruir() {
      raiz.replaceChildren();
    },
  };
}
