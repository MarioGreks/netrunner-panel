import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/tab.css";
import { iniciarTema } from "./lib/tema";
import {
  alEvento,
  alternarPanel,
  estadoCalendario,
  estadoCuentas,
  estadoMeteo,
  obtenerConfig,
  type Borde,
  type CuentaAtras,
  type EstadoCalendario,
  type EstadoMeteo,
} from "./lib/ipc";
import { iconoMeteo } from "./lib/iconos-meteo";
import { colorCategoria } from "./lib/categorias";
import { proximo } from "./ui/resumen";

const DIAS = ["DOM", "LUN", "MAR", "MIÉ", "JUE", "VIE", "SÁB"];
const MESES = ["ENE", "FEB", "MAR", "ABR", "MAY", "JUN", "JUL", "AGO", "SEP", "OCT", "NOV", "DIC"];
const MIN_ALERTA = 15;
/** Tiempo con el ratón encima antes de abrir el panel. */
const ESPERA_HOVER = 300;
/** Tras cerrarse el panel la ventana reaparece bajo el cursor: no se reabre por hover enseguida. */
const GUARDA_CIERRE = 1200;

const dos = (n: number) => String(n).padStart(2, "0");
const recortar = (t: string, max: number) => (t.length > max ? `${t.slice(0, max - 1).trimEnd()}…` : t);
const inicioDeDia = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** «23D 04H 12M»; sin días «04H 12M»; sin horas «12M». */
function restante(ms: number): string {
  const min = Math.max(0, Math.floor(ms / 60000));
  const d = Math.floor(min / 1440);
  const h = Math.floor((min % 1440) / 60);
  const m = min % 60;
  if (d > 0) return `${d}D ${dos(h)}H ${dos(m)}M`;
  if (h > 0) return `${dos(h)}H ${dos(m)}M`;
  return `${dos(m)}M`;
}

const q = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const elHm = q("hm");
const elSeg = q("seg");
const elFecha = q("fecha");
const elMeteo = q("meteo");
const elProx = q("prox");
const elProxCat = q("prox-cat");
const elProxTexto = q("prox-texto");
const elCuenta = q("cuenta");
const elCuentaTitulo = q("cuenta-titulo");
const elCuentaValor = q("cuenta-valor");

// El borde decide qué lado del marco queda sin línea
const aplicarBorde = (borde: Borde) => {
  document.documentElement.dataset.borde = borde;
};
iniciarTema();
aplicarBorde("right");

let meteo: EstadoMeteo | null = null;
let agenda: EstadoCalendario | null = null;
let cuentas: CuentaAtras[] = [];
let tieneUrl = false;

// --- Próximo evento y cuenta atrás ---
const pintarAgenda = () => {
  const ahora = new Date();
  const ev = tieneUrl && agenda ? proximo(agenda.eventos, ahora) : null;

  let texto = "SIN EVENTOS";
  let alerta = false;
  let color = "";
  if (ev) {
    const ini = new Date(ev.inicio);
    let cuando: string;
    if (ev.todo_el_dia) {
      cuando = "HOY";
    } else if (ini.getTime() <= ahora.getTime()) {
      cuando = "AHORA";
    } else {
      const hoy = inicioDeDia(ahora);
      const dia = inicioDeDia(ini);
      const hora = `${dos(ini.getHours())}:${dos(ini.getMinutes())}`;
      cuando = dia === hoy ? hora : dia === hoy + 86400000 ? `MAÑ ${hora}` : `${ini.getDate()} ${MESES[ini.getMonth()]}`;
      alerta = ini.getTime() - ahora.getTime() < MIN_ALERTA * 60000;
    }
    texto = `${cuando} · ${ev.titulo}`;
    color = ev.categorias.length ? colorCategoria(ev.categorias[0]) : "";
  }
  elProxTexto.textContent = texto;
  elProx.classList.toggle("mini__prox--vacio", !ev);
  elProx.classList.toggle("mini__prox--alerta", alerta);
  if (color) elProxCat.style.setProperty("--cat", color);
  else elProxCat.style.removeProperty("--cat");

  const cuenta = cuentas
    .filter((c) => new Date(c.inicio).getTime() > ahora.getTime())
    .sort((a, b) => new Date(a.inicio).getTime() - new Date(b.inicio).getTime())[0];
  elCuenta.hidden = !cuenta;
  elProx.classList.toggle("mini__prox--grande", !cuenta);
  if (cuenta) {
    elCuentaTitulo.textContent = recortar(cuenta.titulo.toUpperCase(), 26);
    elCuentaValor.textContent = restante(new Date(cuenta.inicio).getTime() - ahora.getTime());
  }
};

// --- Reloj: cada segundo ---
let fechaPintada = "";
const pintarReloj = () => {
  const ahora = new Date();
  elHm.textContent = `${dos(ahora.getHours())}:${dos(ahora.getMinutes())}`;
  elSeg.textContent = dos(ahora.getSeconds());
  const fecha = `${DIAS[ahora.getDay()]} ${ahora.getDate()} ${MESES[ahora.getMonth()]}`;
  if (fecha !== fechaPintada) {
    fechaPintada = fecha;
    elFecha.textContent = fecha;
  }
  // El resto (próximo evento y cuenta atrás) se recalcula al cambiar el minuto
  if (ahora.getSeconds() === 0) pintarAgenda();
};

// --- Tiempo ---
const pintarMeteo = () => {
  const a = meteo?.ciudad ? meteo.actual : null;
  elMeteo.innerHTML = a ? `${iconoMeteo(a.icono, 16, "mini__icono")}<span>${Math.round(a.temperatura)}°</span>` : "";
};

const leerConfig = () =>
  obtenerConfig()
    .then((c) => {
      aplicarBorde(c.edge);
      tieneUrl = !!c.ics_url && c.ics_url.trim() !== "";
      pintarAgenda();
    })
    .catch(() => {});

void alEvento<Borde>("config://cambio", () => void leerConfig());
void alEvento<EstadoMeteo>("meteo://estado", (m) => {
  meteo = m;
  pintarMeteo();
});
void alEvento<EstadoCalendario>("calendario://estado", (s) => {
  agenda = s;
  void leerConfig(); // un cambio de URL llega con un estado nuevo
});
void alEvento<CuentaAtras[]>("cuentas://estado", (c) => {
  cuentas = c;
  pintarAgenda();
});

pintarReloj();
setInterval(pintarReloj, 1000);
void leerConfig().then(() => {
  estadoCalendario().then((s) => { agenda = s; pintarAgenda(); }).catch(() => {});
});
estadoMeteo().then((m) => { meteo = m; pintarMeteo(); }).catch(() => {});
estadoCuentas().then((c) => { cuentas = c; pintarAgenda(); }).catch(() => {});

// --- Interacción: clic o 300 ms con el ratón encima abren el panel ---
const mini = q("mini");
let temporizadorHover: number | undefined;
let bloqueadoHasta = 0;

mini.addEventListener("click", () => {
  void alternarPanel();
});
mini.addEventListener("mouseenter", () => {
  clearTimeout(temporizadorHover);
  temporizadorHover = window.setTimeout(() => {
    if (Date.now() < bloqueadoHasta) return;
    // Con la apertura por borde en pausa (bandeja) tampoco abre el hover
    obtenerConfig()
      .then((c) => {
        if (!c.borde_pausado) void alternarPanel();
      })
      .catch(() => {});
  }, ESPERA_HOVER);
});
mini.addEventListener("mouseleave", () => clearTimeout(temporizadorHover));
void alEvento("panel://mostrar", () => clearTimeout(temporizadorHover));
void alEvento("panel://ocultar", () => {
  clearTimeout(temporizadorHover);
  bloqueadoHasta = Date.now() + GUARDA_CIERRE;
});
