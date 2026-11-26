// Modelo de la agenda recibida del panel (GET /agenda) y lógica pura de pintado.

export interface Evento {
  id: string;
  titulo: string;
  inicio: string;
  fin: string;
  todo_el_dia: boolean;
  ubicacion: string | null;
  categorias: string[];
  descripcion: string | null;
}

export interface Cuenta {
  id_evento: string;
  titulo: string;
  inicio: string;
  categorias: string[];
}

export interface Agenda {
  eventos: Evento[];
  cuentas: Cuenta[];
}

const esObj = (x: unknown): x is Record<string, unknown> => typeof x === 'object' && x !== null;
const fechaValida = (x: unknown): x is string => typeof x === 'string' && !Number.isNaN(new Date(x).getTime());
const textoONulo = (x: unknown): string | null => (typeof x === 'string' && x !== '' ? x : null);
const categoriasDe = (x: unknown): string[] => (Array.isArray(x) ? x.filter((c): c is string => typeof c === 'string') : []);

function evento(x: unknown): Evento | null {
  if (!esObj(x) || typeof x.titulo !== 'string' || !fechaValida(x.inicio)) return null;
  return {
    id: typeof x.id === 'string' ? x.id : `${x.titulo}|${x.inicio}`,
    titulo: x.titulo,
    inicio: x.inicio,
    fin: fechaValida(x.fin) ? x.fin : x.inicio,
    todo_el_dia: x.todo_el_dia === true,
    ubicacion: textoONulo(x.ubicacion),
    categorias: categoriasDe(x.categorias),
    descripcion: textoONulo(x.descripcion),
  };
}

function cuenta(x: unknown): Cuenta | null {
  if (!esObj(x) || typeof x.titulo !== 'string' || !fechaValida(x.inicio) || x.borrada === true) return null;
  return {
    id_evento: typeof x.id_evento === 'string' ? x.id_evento : `${x.titulo}|${x.inicio}`,
    titulo: x.titulo,
    inicio: x.inicio,
    categorias: categoriasDe(x.categorias),
  };
}

/** Acepta solo lo bien formado: descarta los elementos rotos en vez de fallar entero. null si no es una agenda. */
export function validarAgenda(x: unknown): Agenda | null {
  if (!esObj(x) || !Array.isArray(x.eventos) || !Array.isArray(x.cuentas)) return null;
  return {
    eventos: x.eventos.map(evento).filter((e): e is Evento => e !== null),
    cuentas: x.cuentas.map(cuenta).filter((c): c is Cuenta => c !== null),
  };
}

// Paleta de ../src/lib/categorias.ts con los valores del tema netrunner (cat-4/cat-5 son color-mix del panel).
const PALETA = ['#f5e50a', '#2de2e6', '#ff2a4d', '#9be46d', '#8c8fa1'];

/** Color determinista por nombre de categoría (FNV-1a, sin tildes ni mayúsculas), igual que el panel. */
export function colorCategoria(nombre: string): string {
  const n = nombre.normalize('NFD').replace(/[̀-ͯ]/g, '').trim().toLowerCase();
  let h = 2166136261;
  for (let i = 0; i < n.length; i++) {
    h ^= n.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return PALETA[h % PALETA.length];
}

const dos = (n: number) => String(n).padStart(2, '0');
const DIAS = ['DOM', 'LUN', 'MAR', 'MIÉ', 'JUE', 'VIE', 'SÁB'];
const MESES = ['ENE', 'FEB', 'MAR', 'ABR', 'MAY', 'JUN', 'JUL', 'AGO', 'SEP', 'OCT', 'NOV', 'DIC'];

/** Tiempo que falta, como en el panel: «2D 03H 05M», «03H 05M» o «05M». */
export function restante(ms: number): string {
  const min = Math.max(0, Math.floor(ms / 60000));
  const d = Math.floor(min / 1440);
  const h = Math.floor((min % 1440) / 60);
  const m = min % 60;
  if (d > 0) return `${d}D ${dos(h)}H ${dos(m)}M`;
  if (h > 0) return `${dos(h)}H ${dos(m)}M`;
  return `${dos(m)}M`;
}

export const horaDe = (d: Date) => `${dos(d.getHours())}:${dos(d.getMinutes())}`;
export const diaCorto = (d: Date) => `${DIAS[d.getDay()]} ${d.getDate()} ${MESES[d.getMonth()]}`;
const inicioDia = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());

/** HOY, MAÑANA o «JUE 27 NOV». */
export function etiquetaDia(d: Date, hoy: Date): string {
  const dif = Math.round((inicioDia(d).getTime() - inicioDia(hoy).getTime()) / 86400000);
  return dif === 0 ? 'HOY' : dif === 1 ? 'MAÑANA' : diaCorto(d);
}

export interface Dia {
  clave: string;
  fecha: Date;
  eventos: Evento[];
}

/** Eventos de hoy en adelante agrupados por día local; los de día completo (fin exclusivo) aparecen en cada día que cubren. */
export function agruparPorDias(eventos: Evento[], ahora: Date): Dia[] {
  const hoy = inicioDia(ahora).getTime();
  const dias = new Map<string, Dia>();
  const meter = (f: Date, e: Evento) => {
    const clave = `${f.getFullYear()}-${dos(f.getMonth() + 1)}-${dos(f.getDate())}`;
    const d = dias.get(clave) ?? { clave, fecha: inicioDia(f), eventos: [] };
    d.eventos.push(e);
    dias.set(clave, d);
  };
  for (const e of eventos) {
    const ini = new Date(e.inicio);
    if (e.todo_el_dia) {
      const fin = new Date(e.fin).getTime();
      let f = inicioDia(ini);
      let n = 0;
      do {
        if (f.getTime() >= hoy) meter(f, e);
        f = new Date(f.getFullYear(), f.getMonth(), f.getDate() + 1);
      } while (f.getTime() < fin && ++n < 366);
    } else if (inicioDia(ini).getTime() >= hoy) meter(ini, e);
  }
  const hora = (e: Evento) => (e.todo_el_dia ? -1 : new Date(e.inicio).getTime());
  return [...dias.values()]
    .sort((a, b) => a.fecha.getTime() - b.fecha.getTime())
    .map((d) => ({ ...d, eventos: d.eventos.sort((a, b) => hora(a) - hora(b)) }));
}
