import { File, Paths } from 'expo-file-system';
import * as SecureStore from 'expo-secure-store';
import type { Datos } from './notas/fusion';

export const PUERTO_DEFECTO = 47821;
const CLAVE_TOKEN = 'panel_token';
const TIMEOUT_MS = 4000;

export interface ConfigPanel {
  host: string;
  puerto: number;
}

/** Red caída, timeout o panel apagado. */
export class NoAlcanzable extends Error {
  constructor() {
    super('NO SE ENCUENTRA EL PANEL: ¿MISMA WIFI? ¿CONEXIÓN MÓVIL ACTIVADA EN EL PC?');
  }
}
/** 401 en una ruta con token. */
export class TokenInvalido extends Error {
  constructor() {
    super('EMPAREJAMIENTO CADUCADO: VUELVE A EMPAREJAR');
  }
}

const configF = () => new File(Paths.document, 'panel.json');

export async function configuracion(): Promise<ConfigPanel | null> {
  try {
    const f = configF();
    if (!f.exists) return null;
    const c = JSON.parse(await f.text()) as Partial<ConfigPanel>;
    return typeof c.host === 'string' && typeof c.puerto === 'number' ? { host: c.host, puerto: c.puerto } : null;
  } catch {
    return null;
  }
}

export async function guardarConfiguracion(c: ConfigPanel): Promise<void> {
  const f = configF();
  if (!f.exists) f.create();
  f.write(JSON.stringify(c));
}

/** Borra el token (el emparejamiento) pero conserva host/puerto. */
export async function olvidarToken(): Promise<void> {
  await SecureStore.deleteItemAsync(CLAVE_TOKEN);
}

/** Borra token y configuración. */
export async function olvidarPanel(): Promise<void> {
  await olvidarToken();
  try {
    const f = configF();
    if (f.exists) f.delete();
  } catch {
    // sin más
  }
}

export async function hayToken(): Promise<boolean> {
  return (await SecureStore.getItemAsync(CLAVE_TOKEN)) !== null;
}

async function pedir(c: ConfigPanel, ruta: string, init: { metodo?: string; cuerpo?: unknown; token?: string }): Promise<Response> {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), TIMEOUT_MS);
  try {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (init.token !== undefined) headers.Authorization = `Bearer ${init.token}`;
    return await fetch(`http://${c.host}:${c.puerto}${ruta}`, {
      method: init.metodo ?? 'GET',
      headers,
      body: init.cuerpo === undefined ? undefined : JSON.stringify(init.cuerpo),
      signal: ctl.signal,
    });
  } catch {
    throw new NoAlcanzable();
  } finally {
    clearTimeout(t);
  }
}

async function conToken<T>(ruta: string, init: { metodo?: string; cuerpo?: unknown } = {}): Promise<T> {
  const [c, token] = await Promise.all([configuracion(), SecureStore.getItemAsync(CLAVE_TOKEN)]);
  if (c === null || token === null) throw new TokenInvalido();
  const r = await pedir(c, ruta, { ...init, token });
  if (r.status === 401) throw new TokenInvalido();
  if (!r.ok) throw new Error(`ERROR DEL PANEL (${r.status})`);
  try {
    return (await r.json()) as T;
  } catch {
    throw new Error('RESPUESTA DEL PANEL ILEGIBLE');
  }
}

export async function emparejar(host: string, puerto: number, codigo: string): Promise<void> {
  const c = { host, puerto };
  const r = await pedir(c, '/emparejar', { metodo: 'POST', cuerpo: { codigo } });
  if (r.status === 401) throw new Error('CÓDIGO INCORRECTO O CADUCADO');
  if (r.status === 429) throw new Error('DEMASIADOS INTENTOS: PIDE OTRO CÓDIGO EN EL PC');
  if (!r.ok) throw new Error(`ERROR DEL PANEL (${r.status})`);
  let token: unknown;
  try {
    token = ((await r.json()) as { token?: unknown }).token;
  } catch {
    token = undefined;
  }
  if (typeof token !== 'string' || token === '') throw new Error('RESPUESTA DEL PANEL ILEGIBLE');
  await SecureStore.setItemAsync(CLAVE_TOKEN, token);
  await guardarConfiguracion(c);
}

export const estadoPanel = () => conToken<{ app: string; version: string }>('/estado');

export const sincronizarNotas = (datos: Datos) => conToken<unknown>('/notas/sincronizar', { metodo: 'POST', cuerpo: datos });

export interface RespuestaAgenda {
  eventos: unknown[];
  cuentas: unknown[];
  ultima_sync: string | null;
}
export const agenda = () => conToken<RespuestaAgenda>('/agenda');
