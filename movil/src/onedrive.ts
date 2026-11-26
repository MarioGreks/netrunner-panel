import { File, Paths } from 'expo-file-system';
import { obtenerToken } from './microsoft';

export interface Carpeta {
  driveId: string;
  folderId: string;
  ruta: string;
}

const GRAPH = 'https://graph.microsoft.com/v1.0';
const fichero = () => new File(Paths.document, 'conexion.json');
const enc = (ruta: string) => ruta.split('/').filter(Boolean).map(encodeURIComponent).join('/');

/** Carpeta elegida y guardada en el móvil, o null. */
export async function carpetaGuardada(): Promise<Carpeta | null> {
  const f = fichero();
  if (!f.exists) return null;
  try {
    return JSON.parse(await f.text()) as Carpeta;
  } catch {
    return null;
  }
}

/** Guarda la carpeta elegida en el móvil. */
export async function guardarCarpeta(c: Carpeta): Promise<void> {
  const f = fichero();
  if (!f.exists) f.create();
  f.write(JSON.stringify(c));
}

/** Olvida la carpeta guardada. */
export async function olvidarCarpeta(): Promise<void> {
  const f = fichero();
  if (f.exists) f.delete();
}

/** Llamada a Graph con token; un 401 reintenta una vez refrescando. Errores de red en español. */
async function graph(url: string, init: RequestInit = {}): Promise<Response> {
  const llamar = async (forzar: boolean) => {
    const token = await obtenerToken(forzar);
    try {
      return await fetch(url.startsWith('http') ? url : GRAPH + url, {
        ...init,
        headers: { ...(init.headers as Record<string, string>), Authorization: `Bearer ${token}` },
      });
    } catch (e) {
      if (e instanceof TypeError) throw new Error('SIN CONEXIÓN A INTERNET');
      throw e;
    }
  };
  const res = await llamar(false);
  return res.status === 401 ? llamar(true) : res;
}

async function fallo(res: Response, que: string): Promise<Error> {
  // solo el código de error de Graph, nunca el cuerpo completo
  const codigo = ((await res.json().catch(() => null)) as { error?: { code?: string } } | null)?.error?.code;
  return new Error(`${que} (${res.status}${codigo ? ' ' + codigo : ''})`);
}

interface Item {
  id: string;
  name: string;
  folder?: unknown;
  parentReference?: { driveId?: string; path?: string };
}

function aCarpeta(i: Item, ruta?: string): Carpeta | null {
  const driveId = i.parentReference?.driveId;
  if (i.folder === undefined || driveId === undefined) return null;
  const padre = (i.parentReference?.path ?? '').replace(/^\/drives?\/[^:]*:?/, '');
  return { driveId, folderId: i.id, ruta: ruta ?? `${padre}/${i.name}`.replace(/^\/+/, '') };
}

/** Busca en el OneDrive las carpetas llamadas exactamente `netrunner-panel`. */
export async function buscarCarpetas(): Promise<Carpeta[]> {
  const res = await graph("/me/drive/root/search(q='netrunner-panel')?$select=id,name,folder,parentReference");
  if (!res.ok) throw await fallo(res, 'NO SE PUDO BUSCAR');
  const { value } = (await res.json()) as { value: Item[] };
  return value.filter((i) => i.name === 'netrunner-panel').map((i) => aCarpeta(i)).filter((c): c is Carpeta => c !== null);
}

/** Carpeta a partir de su ruta escrita a mano (null si no existe o no es carpeta). */
export async function carpetaPorRuta(ruta: string): Promise<Carpeta | null> {
  const res = await graph(`/me/drive/root:/${enc(ruta)}?$select=id,name,folder,parentReference`);
  if (res.status === 404) return null;
  if (!res.ok) throw await fallo(res, 'NO SE PUDO COMPROBAR LA RUTA');
  return aCarpeta((await res.json()) as Item, ruta.split('/').filter(Boolean).join('/'));
}

async function base(): Promise<string> {
  const c = await carpetaGuardada();
  if (c === null) throw new Error('NO HAY CARPETA ELEGIDA: ELÍGELA EN AJUSTES');
  return `/drives/${c.driveId}/items/${c.folderId}`;
}

/** Lee un JSON de la carpeta con su eTag; 404 → datos null. Lanza si el JSON está corrupto (nunca devuelve vacío). */
export async function leerJson<T>(nombre: string): Promise<{ datos: T | null; etag: string | null }> {
  const res = await graph(`${await base()}:/${encodeURIComponent(nombre)}:?$select=id,eTag,size,lastModifiedDateTime,@microsoft.graph.downloadUrl`);
  if (res.status === 404) return { datos: null, etag: null };
  if (!res.ok) throw await fallo(res, `NO SE PUDO LEER ${nombre}`);
  const meta = (await res.json()) as { eTag?: string; '@microsoft.graph.downloadUrl'?: string };
  const url = meta['@microsoft.graph.downloadUrl'];
  let texto: string;
  try {
    // La URL firmada no lleva Authorization; si Graph no la da, se pide /content con token.
    const d = url !== undefined ? await fetch(url) : await graph(`${await base()}:/${encodeURIComponent(nombre)}:/content`);
    if (!d.ok) throw new Error(`NO SE PUDO DESCARGAR ${nombre} (${d.status})`);
    texto = await d.text();
  } catch (e) {
    if (e instanceof TypeError) throw new Error('SIN CONEXIÓN A INTERNET');
    throw e;
  }
  try {
    return { datos: JSON.parse(texto) as T, etag: meta.eTag ?? null };
  } catch {
    throw new Error(`${nombre} ESTÁ CORRUPTO O NO ES JSON: NO SE TOCA`);
  }
}

/** Escribe un JSON con control de versión: etag null = fichero nuevo. Conflicto (412/409) → { ok:false, conflicto:true }. */
export async function escribirJson(
  nombre: string,
  datos: unknown,
  etag: string | null,
): Promise<{ ok: true; etag: string } | { ok: false; conflicto: true }> {
  const res = await graph(`${await base()}:/${encodeURIComponent(nombre)}:/content`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json', ...(etag !== null ? { 'If-Match': etag } : { 'If-None-Match': '*' }) },
    body: JSON.stringify(datos, null, 2),
  });
  if (res.status === 412 || res.status === 409) return { ok: false, conflicto: true };
  if (!res.ok) throw await fallo(res, `NO SE PUDO ESCRIBIR ${nombre}`);
  const r = (await res.json()) as { eTag?: string };
  return { ok: true, etag: r.eTag ?? '' };
}

/** Lista los ficheros de la carpeta (prueba de conexión). */
export async function listarFicheros(): Promise<{ name: string; size: number; lastModifiedDateTime: string }[]> {
  const res = await graph(`${await base()}/children?$select=name,size,lastModifiedDateTime`);
  if (!res.ok) throw await fallo(res, 'NO SE PUDO LISTAR LA CARPETA');
  return ((await res.json()) as { value: { name: string; size: number; lastModifiedDateTime: string }[] }).value;
}
