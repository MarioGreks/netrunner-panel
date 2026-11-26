// Comprobaciones de la fusión (equivalentes a los tests de notas.rs). No se importa desde app/.
// Se ejecuta compilando con tsc a un directorio temporal y lanzando node.
import assert from 'node:assert';
import { esDatosLegible, fusionar, igual, normalizar, purgar, type Datos, type Nota, type Tarea } from './fusion';

const tarea = (id: string, texto: string, modificada: number, borrada: boolean): Tarea => ({ id, texto, hecha: false, orden: 0, creada: 0, modificada, borrada });
const nota = (id: string, titulo: string, modificada: number, borrada: boolean): Nota => ({ id, titulo, cuerpo: '', creada: 0, modificada, borrada });
const datosT = (tareas: Tarea[]): Datos => ({ tareas, notas: [] });
let n = 0;
const prueba = (nombre: string, f: () => void) => {
  f();
  n++;
  console.log('  ok ' + nombre);
};

prueba('cambios distintos en cada lado se conservan', () => {
  const l: Datos = { tareas: [tarea('a', 'local', 10, false)], notas: [nota('n1', 'solo local', 10, false)] };
  const r: Datos = { tareas: [tarea('b', 'remota', 20, false)], notas: [nota('n2', 'solo remota', 20, false)] };
  const f = fusionar(l, r);
  assert.strictEqual(f.tareas.length, 2);
  assert.strictEqual(f.notas.length, 2);
  assert.deepStrictEqual(f.tareas.map((t) => t.id), ['a', 'b']);
});
prueba('misma tarea editada: gana la más reciente (ambos sentidos)', () => {
  const l = datosT([tarea('a', 'vieja', 10, false)]);
  const r = datosT([tarea('a', 'nueva', 20, false)]);
  assert.strictEqual(fusionar(l, r).tareas[0]!.texto, 'nueva');
  assert.strictEqual(fusionar(r, l).tareas[0]!.texto, 'nueva');
});
prueba('borrado gana a edición más antigua', () => {
  const l = datosT([tarea('a', 'editada', 10, false)]);
  const r = datosT([tarea('a', '', 20, true)]);
  assert.ok(fusionar(l, r).tareas[0]!.borrada);
  assert.ok(fusionar(r, l).tareas[0]!.borrada);
});
prueba('edición más nueva gana a borrado antiguo', () => {
  const l = datosT([tarea('a', 'revivida', 30, false)]);
  const r = datosT([tarea('a', '', 20, true)]);
  assert.ok(!fusionar(l, r).tareas[0]!.borrada);
  assert.ok(!fusionar(r, l).tareas[0]!.borrada);
});
prueba('empate gana el borrado (tareas y notas)', () => {
  const l = datosT([tarea('a', 'texto', 10, false)]);
  const r = datosT([tarea('a', '', 10, true)]);
  assert.ok(fusionar(l, r).tareas[0]!.borrada);
  assert.ok(fusionar(r, l).tareas[0]!.borrada);
  const ln: Datos = { tareas: [], notas: [nota('n', 't', 5, false)] };
  const rn: Datos = { tareas: [], notas: [nota('n', '', 5, true)] };
  assert.ok(fusionar(ln, rn).notas[0]!.borrada);
  assert.ok(fusionar(rn, ln).notas[0]!.borrada);
});
prueba('empate sin borrados: se queda la local', () => {
  const f = fusionar(datosT([tarea('a', 'L', 10, false)]), datosT([tarea('a', 'R', 10, false)]));
  assert.strictEqual(f.tareas[0]!.texto, 'L');
});
prueba('purga marcas de más de 30 días', () => {
  const dia = 24 * 3600 * 1000;
  const ahora = 100 * dia;
  const d: Datos = {
    tareas: [tarea('vieja', '', ahora - 31 * dia, true), tarea('reciente', '', ahora - 29 * dia, true), tarea('viva_antigua', 'x', ahora - 90 * dia, false)],
    notas: [nota('n_vieja', '', ahora - 40 * dia, true), nota('n_viva', 'x', ahora - 40 * dia, false)],
  };
  const p = purgar(d, ahora);
  assert.deepStrictEqual(p.tareas.map((t) => t.id), ['reciente', 'viva_antigua']);
  assert.deepStrictEqual(p.notas.map((x) => x.id), ['n_viva']);
});
prueba('igualdad: claves en otro orden = igual', () => {
  assert.ok(igual({ a: 1, b: [{ x: 1, y: 2 }] }, { b: [{ y: 2, x: 1 }], a: 1 }));
});
prueba('igualdad: array en otro orden = distinto', () => {
  assert.ok(!igual({ l: [1, 2] }, { l: [2, 1] }));
  assert.ok(!igual(datosT([tarea('a', '', 1, false), tarea('b', '', 1, false)]), datosT([tarea('b', '', 1, false), tarea('a', '', 1, false)])));
});
prueba('normalizar rellena los campos ausentes', () => {
  const d = normalizar({ tareas: [{ id: 'a', texto: 'x' }], notas: [{ id: 'n' }] });
  assert.deepStrictEqual(d.tareas[0], { id: 'a', texto: 'x', hecha: false, orden: 0, creada: 0, modificada: 0, borrada: false });
  assert.deepStrictEqual(d.notas[0], { id: 'n', titulo: '', cuerpo: '', creada: 0, modificada: 0, borrada: false });
  assert.deepStrictEqual(normalizar({}), { tareas: [], notas: [] });
});

prueba('formato ajeno: no es legible (no se escribe encima)', () => {
  assert.ok(esDatosLegible({ tareas: [], notas: [] }) && esDatosLegible({}));
  assert.ok(!esDatosLegible([]) && !esDatosLegible('x') && !esDatosLegible(null));
  assert.ok(!esDatosLegible({ tareas: 'no' }) && !esDatosLegible({ notas: [1] }));
});

console.log(`fusión: ${n} comprobaciones OK`);
