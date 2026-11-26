import assert from 'node:assert';
import { agruparPorDias, colorCategoria, etiquetaDia, restante, validarAgenda } from './modelo';

const a = validarAgenda({
  eventos: [
    { id: 'a', titulo: 'Clase', inicio: '2026-11-27T09:00:00+01:00', fin: '2026-11-27T10:00:00+01:00', todo_el_dia: false, categorias: ['Uni'] },
    { id: 'b', titulo: 'Roto', inicio: 'no-es-fecha' },
    { id: 'c', inicio: '2026-11-27T09:00:00+01:00' },
    42,
  ],
  cuentas: [
    { id_evento: 'a', titulo: 'Examen', inicio: '2026-12-01T09:00:00+01:00', categorias: [] },
    { id_evento: 'z', titulo: 'Borrada', inicio: '2026-12-01T09:00:00+01:00', borrada: true },
  ],
});
assert(a !== null && a.eventos.length === 1 && a.eventos[0].titulo === 'Clase');
assert(a.cuentas.length === 1);
assert.strictEqual(validarAgenda({ eventos: 1 }), null);

// día completo con fin exclusivo (medianoche local) cae en un solo día
const ev = {
  id: 'd',
  titulo: 'Todo',
  inicio: new Date(2026, 10, 27).toISOString(),
  fin: new Date(2026, 10, 28).toISOString(),
  todo_el_dia: true,
  ubicacion: null,
  categorias: [],
  descripcion: null,
};
const g = agruparPorDias([ev], new Date(2026, 10, 26, 12));
assert.strictEqual(g.length, 1);
assert.strictEqual(etiquetaDia(g[0].fecha, new Date(2026, 10, 26, 12)), 'MAÑANA');
assert.strictEqual(etiquetaDia(g[0].fecha, new Date(2026, 10, 20)), 'VIE 27 NOV');
assert.strictEqual(agruparPorDias([ev], new Date(2026, 10, 29)).length, 0);

assert.strictEqual(colorCategoria('Uni'), colorCategoria('  úNí '));
assert.strictEqual(restante(2 * 86400000 + 3 * 3600000 + 5 * 60000), '2D 03H 05M');
assert.strictEqual(restante(3 * 3600000 + 5 * 60000), '03H 05M');
assert.strictEqual(restante(5 * 60000), '05M');
assert.strictEqual(restante(-5), '00M');
console.log('agenda OK');
