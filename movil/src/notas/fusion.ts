// Port puro (sin React ni E/S) de la fusión de src-tauri/src/sync.rs y notas.rs.

export interface Tarea {
  id: string;
  texto: string;
  hecha: boolean;
  orden: number;
  creada: number;
  modificada: number;
  borrada: boolean;
}

export interface Nota {
  id: string;
  titulo: string;
  cuerpo: string;
  creada: number;
  modificada: number;
  borrada: boolean;
}

export interface Datos {
  tareas: Tarea[];
  notas: Nota[];
}

export const VIDA_MARCAS_MS = 30 * 24 * 3600 * 1000;

/** El remoto gana si su `modificada` es mayor; en empate gana el borrado; si no, se queda el local. */
export function ganaRemota(lMod: number, lBorr: boolean, rMod: number, rBorr: boolean): boolean {
  return rMod !== lMod ? rMod > lMod : rBorr && !lBorr;
}

/** Parte de la lista local en su orden, sustituye por la remota si gana y añade al final las remotas nuevas. */
export function fusionarLista<T extends { id: string; modificada: number; borrada: boolean }>(local: T[], remoto: T[]): T[] {
  const resultado = [...local];
  const indice = new Map<string, number>();
  resultado.forEach((e, i) => indice.set(e.id, i));
  for (const r of remoto) {
    const i = indice.get(r.id);
    if (i !== undefined) {
      const l = resultado[i]!;
      if (ganaRemota(l.modificada, l.borrada, r.modificada, r.borrada)) resultado[i] = r;
    } else {
      indice.set(r.id, resultado.length);
      resultado.push(r);
    }
  }
  return resultado;
}

export function fusionar(local: Datos, remoto: Datos): Datos {
  return { tareas: fusionarLista(local.tareas, remoto.tareas), notas: fusionarLista(local.notas, remoto.notas) };
}

/** Devuelve una copia sin las marcas de borrado con más de 30 días. */
export function purgar(datos: Datos, ahora: number): Datos {
  const limite = ahora - VIDA_MARCAS_MS;
  return {
    tareas: datos.tareas.filter((t) => !(t.borrada && t.modificada < limite)),
    notas: datos.notas.filter((n) => !(n.borrada && n.modificada < limite)),
  };
}

/** Igualdad profunda: el orden de las claves de los objetos no cuenta; el de los arrays sí. */
export function igual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((x, i) => igual(x, b[i]));
  if (typeof a === 'object' && a !== null && typeof b === 'object' && b !== null && !Array.isArray(b)) {
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    return ka.length === kb.length && ka.every((k) => k in b && igual((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
  }
  return false;
}

const num = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : 0);
const txt = (v: unknown) => (typeof v === 'string' ? v : '');
const obj = (v: unknown): Record<string, unknown> => (typeof v === 'object' && v !== null ? (v as Record<string, unknown>) : {});

/** ¿Tiene la forma que el panel sabe leer? Si no (como serde en el panel), no se escribe encima. */
export function esDatosLegible(crudo: unknown): boolean {
  if (typeof crudo !== 'object' || crudo === null || Array.isArray(crudo)) return false;
  const lista = (v: unknown) => v === undefined || (Array.isArray(v) && v.every((x) => typeof x === 'object' && x !== null && !Array.isArray(x)));
  const c = crudo as Record<string, unknown>;
  return lista(c.tareas) && lista(c.notas);
}

/** Rellena los campos ausentes con sus valores por defecto (como serde(default)). */
export function normalizar(crudo: unknown): Datos {
  const c = obj(crudo);
  const lista = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);
  return {
    tareas: lista(c.tareas).map((x) => {
      const t = obj(x);
      return { id: txt(t.id), texto: txt(t.texto), hecha: t.hecha === true, orden: num(t.orden), creada: num(t.creada), modificada: num(t.modificada), borrada: t.borrada === true };
    }),
    notas: lista(c.notas).map((x) => {
      const n = obj(x);
      return { id: txt(n.id), titulo: txt(n.titulo), cuerpo: txt(n.cuerpo), creada: num(n.creada), modificada: num(n.modificada), borrada: n.borrada === true };
    }),
  };
}
