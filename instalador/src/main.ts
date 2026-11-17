// Instalador de Netrunner Panel: flujo de pantallas.
import "../../src/styles/tokens.css";
import "../../src/styles/temas.css";
import "./estilos.css";
import { TEMAS, temaValido } from "../../src/lib/tema";
import { backend, pantallaInicial } from "./backend";
import type { Borde, EstadoInstalador, OpcionesInstalacion } from "./backend";
import { arranque } from "./arranque";
import { transicion } from "./glitch";
import { boton, casilla, el, esperar } from "./util";

type Nombre = "arranque" | "bienvenida" | "opciones" | "instalando" | "fin";

const ventana = document.getElementById("ventana") as HTMLElement;
const interior = document.getElementById("interior") as HTMLElement;
const pantalla = document.getElementById("pantalla") as HTMLElement;
const tituloBarra = document.getElementById("titulo-barra") as HTMLElement;
const cerrar = document.getElementById("cerrar") as HTMLButtonElement;

let est: EstadoInstalador;
let op: OpcionesInstalacion;
let abrirAlFinal = true;
let limpiarPantalla: (() => void) | null = null;

const actualizando = () => est.modo === "actualizacion";
const flujo = (): Nombre[] =>
  actualizando() ? ["arranque", "instalando", "fin"] : ["arranque", "bienvenida", "opciones", "instalando", "fin"];

function siguiente(de: Nombre): Nombre {
  const f = flujo();
  return f[Math.min(f.length - 1, f.indexOf(de) + 1)];
}

const aplicarTema = (id: string) => ventana.setAttribute("data-tema", temaValido(id));

/** Estructura común: cuerpo + pie con indicador de pasos y botones. */
function marco(nombre: string, cuerpo: HTMLElement[], botones: HTMLElement[]): HTMLElement {
  const raiz = el("section", `pant ${nombre}`);
  const c = el("div", "cuerpo");
  c.append(...cuerpo);
  const pie = el("footer", "pie");
  const pasos = el("div", "pasos");
  const f: string[] = flujo().filter((n) => n !== "arranque");
  const idx = f.indexOf(nombre);
  f.forEach((_, i) => pasos.append(el("i", i <= idx ? "on" : "")));
  const b = el("div", "botones");
  b.append(...botones);
  pie.append(pasos, b);
  raiz.append(c, pie);
  return raiz;
}

const etiqueta = (t: string) => el("div", "etiqueta", t);
const titulo = (t: string) => el("h1", "", t);
const parrafo = (t: string) => el("p", "texto", t);

// ---------- Pantallas ----------

function bienvenida(): HTMLElement {
  const detectada = est.modo === "detectada";
  const cuerpo: HTMLElement[] = [etiqueta("SYS // 0x00 · BIENVENIDA")];
  if (detectada) {
    cuerpo.push(el("div", "version", `VERSIÓN ${est.version_instalada} DETECTADA → ${est.version_nueva}`));
  }
  cuerpo.push(
    titulo("NETRUNNER PANEL"),
    parrafo(
      detectada
        ? "Se actualizará encima de tu versión. Tus notas, cuentas y ajustes se conservan."
        : "Panel lateral HUD para tu escritorio. Se instala en tu usuario, sin permisos de administrador.",
    ),
  );
  return marco("bienvenida", cuerpo, [
    boton(detectada ? "ACTUALIZAR >>" : "INICIAR >>", true, () => void ir("opciones")),
  ]);
}

const NOMBRES_BORDE: Record<Borde, string> = { left: "IZQUIERDA", top: "ARRIBA", right: "DERECHA" };

function opciones(): HTMLElement {
  const detectada = est.modo === "detectada";
  const cab = etiqueta(`CFG // 0x01 · OPCIONES${detectada ? " · VALORES ACTUALES" : ""}`);

  // Columna izquierda: tema + autostart
  const izq = el("div", "col");
  izq.append(el("div", "sub", "TEMA DE COLOR"));
  const rejilla = el("div", "temas");
  const tarjetas = new Map<string, HTMLButtonElement>();
  for (const t of TEMAS) {
    const b = el("button", "tema");
    b.type = "button";
    b.setAttribute("data-tema", t.id);
    const mu = el("span", "muestras");
    mu.append(el("i"), el("i"), el("i"));
    b.append(mu, el("span", "nombre", t.nombre));
    b.addEventListener("click", () => {
      op.tema = t.id;
      aplicarTema(t.id);
      marcar();
    });
    tarjetas.set(t.id, b);
    rejilla.append(b);
  }
  const marcar = () => tarjetas.forEach((b, id) => b.classList.toggle("sel", id === op.tema));
  marcar();
  izq.append(rejilla, casilla("Arrancar con Windows", op.autostart, (v) => (op.autostart = v)));

  // Columna derecha: borde de pantalla
  const der = el("div", "col");
  der.append(el("div", "sub", "BORDE DE PANTALLA"));
  const monitor = el("div", "monitor");
  const zonas = new Map<Borde, HTMLButtonElement>();
  const nombreBorde = el("div", "nombre-borde");
  for (const e of ["left", "top", "right"] as Borde[]) {
    const z = el("button", `zona ${e}`);
    z.type = "button";
    z.setAttribute("aria-label", NOMBRES_BORDE[e]);
    z.addEventListener("click", () => {
      op.edge = e;
      pintarBorde();
    });
    zonas.set(e, z);
    monitor.append(z);
  }
  const pintarBorde = () => {
    zonas.forEach((z, e) => z.classList.toggle("sel", e === op.edge));
    nombreBorde.textContent = NOMBRES_BORDE[op.edge];
  };
  pintarBorde();
  der.append(monitor, nombreBorde);

  const dos = el("div", "dos-col");
  dos.append(izq, der);
  return marco("opciones", [cab, dos], [
    boton("ATRÁS", false, () => void ir("bienvenida")),
    boton("INSTALAR >>", true, () => void ir("instalando")),
  ]);
}

function instalando(): HTMLElement {
  const act = actualizando();
  const cab = etiqueta("INSTALL // 0x02");
  const h = titulo(act ? "ACTUALIZANDO" : "INSTALANDO");
  const pct = el("span", "pct", "0%");
  h.append(" ", pct);
  const barra = el("div", "barra-seg");
  const segs: HTMLElement[] = [];
  for (let i = 0; i < 30; i++) segs.push(barra.appendChild(el("i")));
  const registro = el("div", "registro");
  const msg = el("p", "error-msg");
  const botones = el("div", "botones-error");
  const raiz = marco("instalando", [cab, h, barra, registro, msg], []);
  raiz.querySelector(".pie .botones")!.append(botones);

  const lineas = new Map<string, HTMLElement>();
  const alPaso = (p: { texto: string; hecho: boolean }) => {
    let l = lineas.get(p.texto);
    if (!l) {
      l = el("div", "linea");
      lineas.set(p.texto, l);
      registro.append(l);
      registro.scrollTop = registro.scrollHeight;
    }
    l.replaceChildren(`> ${p.texto} `);
    if (p.hecho) l.append(el("span", "ok", "OK"));
    else l.append("…");
  };
  const alProgreso = (v: number) => {
    const n = Math.max(0, Math.min(100, v));
    pct.textContent = `${Math.round(n)}%`;
    const enc = Math.floor((n / 100) * 30);
    segs.forEach((s, i) => s.classList.toggle("on", i < enc));
  };

  const lanzar = async () => {
    raiz.classList.remove("fallo");
    ventana.classList.add("ocupado");
    lineas.clear();
    registro.replaceChildren();
    alProgreso(0);
    h.firstChild!.textContent = act ? "ACTUALIZANDO" : "INSTALANDO";
    try {
      await backend.instalar({ ...op }, alPaso, alProgreso);
      alProgreso(100);
      await esperar(500);
      ventana.classList.remove("ocupado");
      await ir("fin");
    } catch (e) {
      ventana.classList.remove("ocupado");
      raiz.classList.add("fallo");
      h.firstChild!.textContent = "ERROR";
      msg.textContent = e instanceof Error ? e.message : String(e);
    }
  };
  botones.append(
    boton("REINTENTAR >>", true, () => void lanzar()),
    boton("SALIR", false, () => void backend.salir()),
  );
  // Se lanza al montarse la pantalla
  queueMicrotask(() => void lanzar());
  return raiz;
}

function fin(): HTMLElement {
  const cab = etiqueta("DONE // 0x03");
  if (actualizando()) {
    const l = el("div", "linea cuenta");
    let n = 3;
    const pintar = () => (l.textContent = `> Relanzando en ${n}`);
    pintar();
    const lanzar = async () => {
      limpiarPantalla?.();
      await backend.abrirApp();
      await backend.salir();
    };
    const id = setInterval(() => {
      n -= 1;
      pintar();
      if (n <= 0) {
        clearInterval(id);
        void lanzar();
      }
    }, 1000);
    limpiarPantalla = () => clearInterval(id);
    return marco(
      "fin",
      [cab, titulo(`ACTUALIZADO A ${est.version_nueva}`), parrafo("Netrunner Panel se abrirá en unos segundos."), l],
      [boton("ABRIR YA", true, () => void lanzar())],
    );
  }
  const lado = { left: "izquierdo", top: "superior", right: "derecho" }[op.edge];
  abrirAlFinal = true;
  return marco(
    "fin",
    [
      cab,
      titulo("SISTEMA LISTO"),
      parrafo(`Acerca el ratón al borde ${lado} para abrir el panel.`),
      casilla("Abrir Netrunner Panel ahora", true, (v) => (abrirAlFinal = v)),
    ],
    [
      boton("FINALIZAR", true, async () => {
        if (abrirAlFinal) await backend.abrirApp();
        await backend.salir();
      }),
    ],
  );
}

// ---------- Navegación ----------

function montar(nombre: Nombre): HTMLElement | null {
  limpiarPantalla?.();
  limpiarPantalla = null;
  switch (nombre) {
    case "bienvenida":
      return bienvenida();
    case "opciones":
      return opciones();
    case "instalando":
      return instalando();
    case "fin":
      return fin();
    default:
      return null;
  }
}

let enTransicion = false;

function mostrar(nombre: Nombre) {
  if (nombre === "arranque") {
    const a = arranque(actualizando(), est.version_nueva);
    pantalla.replaceChildren(a.nodo);
    void a.iniciar().then(() => ir(siguiente("arranque")));
    return;
  }
  const n = montar(nombre);
  if (n) pantalla.replaceChildren(n);
  tituloBarra.textContent = `NETRUNNER // ${actualizando() ? "UPDATE" : "SETUP"} v${est.version_nueva}`;
}

async function ir(nombre: Nombre) {
  if (enTransicion) return;
  enTransicion = true;
  await transicion(interior, () => mostrar(nombre));
  enTransicion = false;
}

async function iniciar() {
  est = await backend.estado();
  op = { ...est.opciones, tema: temaValido(est.opciones.tema) };
  aplicarTema(op.tema);
  tituloBarra.textContent = `NETRUNNER // ${actualizando() ? "UPDATE" : "SETUP"} v${est.version_nueva}`;
  cerrar.addEventListener("click", () => {
    if (!ventana.classList.contains("ocupado")) void backend.salir();
  });
  const salto = pantallaInicial() as Nombre | null;
  mostrar(salto && flujo().includes(salto) ? salto : "arranque");
}

void iniciar();
