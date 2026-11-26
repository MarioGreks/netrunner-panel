import { File, Paths } from 'expo-file-system';
import { normalizar, type Datos } from './fusion';

const datosF = () => new File(Paths.document, 'notas.json');
const tmpF = () => new File(Paths.document, 'notas.tmp.json');
const syncF = () => new File(Paths.document, 'notas-sync.json');

/** Copia local (con marcas de borrado). Si no hay o está corrupta, datos vacíos . */
export async function cargarCopia(): Promise<Datos> {
  const f = datosF();
  if (!f.exists) return { tareas: [], notas: [] };
  try {
    return normalizar(JSON.parse(await f.text()));
  } catch {
    // se aparta para no pisarla con una copia vacía
    try {
      const c = new File(Paths.document, 'notas.corrupto.json');
      f.move(c, { overwrite: true });
    } catch {
      // sin más
    }
    return { tareas: [], notas: [] };
  }
}

export function guardarCopia(datos: Datos): void {
  // escritura atómica: temporal + reemplazo
  const t = tmpF();
  if (!t.exists) t.create();
  t.write(JSON.stringify(datos, null, 2));
  t.move(datosF(), { overwrite: true });
}

export async function cargarUltimaSync(): Promise<number | null> {
  const f = syncF();
  if (!f.exists) return null;
  try {
    const v = (JSON.parse(await f.text()) as { ultimaSync?: unknown }).ultimaSync;
    return typeof v === 'number' ? v : null;
  } catch {
    return null;
  }
}

export function guardarUltimaSync(ms: number): void {
  const f = syncF();
  if (!f.exists) f.create();
  f.write(JSON.stringify({ ultimaSync: ms }));
}
