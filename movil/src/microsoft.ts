import * as AuthSession from 'expo-auth-session';
import * as SecureStore from 'expo-secure-store';
import * as WebBrowser from 'expo-web-browser';
import { useEffect, useRef } from 'react';

WebBrowser.maybeCompleteAuthSession();

const descubrimiento = {
  authorizationEndpoint: 'https://login.microsoftonline.com/common/oauth2/v2.0/authorize',
  tokenEndpoint: 'https://login.microsoftonline.com/common/oauth2/v2.0/token',
};
const clientId = process.env.EXPO_PUBLIC_MS_CLIENT_ID ?? '';
const scopes = ['openid', 'profile', 'offline_access', 'User.Read', 'Files.ReadWrite'];
const CLAVE = 'ms_refresh_token';

/** Error: no hay sesión (o caducó y hay que volver a conectar). */
export class SinSesion extends Error {
  constructor(mensaje = 'SIN SESIÓN: CONECTA TU CUENTA EN AJUSTES') {
    super(mensaje);
    this.name = 'SinSesion';
  }
}

/** ¿Hay EXPO_PUBLIC_MS_CLIENT_ID? */
export const clientIdConfigurado = (): boolean => clientId !== '';

/** URI de redirección que hay que registrar en Azure. */
export const redirectUri = (): string => AuthSession.makeRedirectUri({ scheme: 'netrunner', path: 'auth' });

let acceso: { token: string; caduca: number } | null = null;
let refrescando: Promise<string> | null = null;

/** Mensaje corto en español para un error de autorización de Microsoft. */
export function traducirError(e: unknown): string {
  const texto = e instanceof Error ? e.message : String(e);
  const codigo = /AADSTS\d+/.exec(texto)?.[0];
  if (codigo === 'AADSTS50011') return `REDIRECT URI NO REGISTRADA: añade ${redirectUri()} en Azure`;
  if (codigo === 'AADSTS700016') return 'CLIENT ID NO ENCONTRADO: revisa EXPO_PUBLIC_MS_CLIENT_ID';
  if (codigo !== undefined && ['AADSTS65001', 'AADSTS90094', 'AADSTS90095'].includes(codigo)) return 'REQUIERE APROBACIÓN DEL ADMINISTRADOR de tu organización';
  if (/admin/i.test(texto)) return 'REQUIERE APROBACIÓN DEL ADMINISTRADOR de tu organización';
  return `ERROR DE MICROSOFT${codigo !== undefined ? ` (${codigo})` : ''}`;
}

function guardarAcceso(r: AuthSession.TokenResponse) {
  acceso = { token: r.accessToken, caduca: Date.now() + (r.expiresIn ?? 3600) * 1000 };
}

/** Access token válido (>60 s); si no, lo renueva con el refresh token. `forzar` lo renueva siempre. */
export async function obtenerToken(forzar = false): Promise<string> {
  if (!forzar && acceso !== null && acceso.caduca - Date.now() > 60_000) return acceso.token;
  refrescando ??= renovar().finally(() => {
    refrescando = null;
  });
  return refrescando;
}

async function renovar(): Promise<string> {
  const refresh = await SecureStore.getItemAsync(CLAVE);
  if (refresh === null) throw new SinSesion();
  try {
    const r = await AuthSession.refreshAsync({ clientId, refreshToken: refresh, scopes }, descubrimiento);
    guardarAcceso(r);
    if (r.refreshToken !== undefined) await SecureStore.setItemAsync(CLAVE, r.refreshToken);
    return r.accessToken;
  } catch (e) {
    if ((e as { code?: string }).code === 'invalid_grant') {
      await cerrarSesion();
      throw new SinSesion('SESIÓN CADUCADA: VUELVE A CONECTAR');
    }
    throw new Error(`${traducirError(e)} (¿sin red?)`);
  }
}

/** Borra el refresh token y el estado en memoria. */
export async function cerrarSesion(): Promise<void> {
  acceso = null;
  await SecureStore.deleteItemAsync(CLAVE);
}

/** Nombre y correo de la cuenta conectada. */
export async function perfil(): Promise<{ nombre: string; correo: string }> {
  const res = await fetch('https://graph.microsoft.com/v1.0/me?$select=displayName,userPrincipalName,mail', {
    headers: { Authorization: `Bearer ${await obtenerToken()}` },
  });
  if (!res.ok) throw new Error(`NO SE PUDO LEER EL PERFIL (${res.status})`);
  const p = (await res.json()) as { displayName?: string; userPrincipalName?: string; mail?: string };
  return { nombre: p.displayName ?? '', correo: p.mail ?? p.userPrincipalName ?? '' };
}

/** Hook de inicio de sesión: `iniciar()` abre el navegador; `alTerminar` recibe null si fue bien, el error, o undefined si se canceló. */
export function useLoginMicrosoft(alTerminar: (error: string | null | undefined) => void) {
  const [request, response, promptAsync] = AuthSession.useAuthRequest(
    { clientId, scopes, redirectUri: redirectUri(), responseType: AuthSession.ResponseType.Code, usePKCE: true },
    descubrimiento,
  );
  const cb = useRef(alTerminar);
  cb.current = alTerminar;

  useEffect(() => {
    if (response === null || request === null) return;
    if (response.type === 'error') cb.current(traducirError(response.params.error_description ?? response.error ?? ''));
    else if (response.type !== 'success') cb.current(undefined);
    else {
      AuthSession.exchangeCodeAsync(
        { clientId, code: response.params.code, redirectUri: redirectUri(), extraParams: { code_verifier: request.codeVerifier ?? '' } },
        descubrimiento,
      )
        .then(async (r) => {
          guardarAcceso(r);
          if (r.refreshToken !== undefined) await SecureStore.setItemAsync(CLAVE, r.refreshToken);
          cb.current(null);
        })
        .catch((e) => cb.current(traducirError(e)));
    }
  }, [response, request]);

  return { listo: request !== null, iniciar: () => promptAsync().then(() => undefined) };
}
