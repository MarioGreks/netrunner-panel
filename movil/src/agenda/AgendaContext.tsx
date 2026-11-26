import { File, Paths } from 'expo-file-system';
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { AppState } from 'react-native';
import { useConexion } from '../ConexionContext';
import { NoAlcanzable, TokenInvalido, agenda as pedirAgenda } from '../panel';
import { validarAgenda, type Agenda } from './modelo';

interface AgendaCtx {
  agenda: Agenda;
  /** Hora (ms) de la última respuesta válida, de la red o de la caché. */
  actualizada: number | null;
  cargando: boolean;
  error: string | null;
  /** Aviso tenue (no es un error): p. ej. panel no alcanzable. */
  aviso: string | null;
  actualizar: () => Promise<void>;
}

const Ctx = createContext<AgendaCtx | null>(null);

export function useAgenda(): AgendaCtx {
  const c = useContext(Ctx);
  if (c === null) throw new Error('Falta AgendaProvider');
  return c;
}

const VACIA: Agenda = { eventos: [], cuentas: [] };
const cacheF = () => new File(Paths.document, 'agenda.json');

async function leerCache(): Promise<{ agenda: Agenda; hora: number } | null> {
  try {
    const f = cacheF();
    if (!f.exists) return null;
    const c = JSON.parse(await f.text()) as { hora?: unknown; agenda?: unknown };
    const a = validarAgenda(c.agenda);
    return a !== null && typeof c.hora === 'number' ? { agenda: a, hora: c.hora } : null;
  } catch {
    return null;
  }
}

function guardarCache(agenda: Agenda, hora: number): void {
  const f = cacheF();
  if (!f.exists) f.create();
  f.write(JSON.stringify({ hora, agenda }));
}

export function AgendaProvider({ children }: { children: ReactNode }) {
  const conexion = useConexion();
  const conexionRef = useRef(conexion);
  conexionRef.current = conexion;
  const puede = conexion.estado === 'emparejado';
  const puedeRef = useRef(puede);
  puedeRef.current = puede;

  const [agenda, setAgenda] = useState<Agenda>(VACIA);
  const [actualizada, setActualizada] = useState<number | null>(null);
  const [cargando, setCargando] = useState(false);
  const [cargada, setCargada] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [aviso, setAviso] = useState<string | null>(null);
  const enCurso = useRef(false);

  const actualizar = useCallback(async (): Promise<void> => {
    if (!puedeRef.current || enCurso.current) return;
    enCurso.current = true;
    setCargando(true);
    try {
      const a = validarAgenda(await pedirAgenda());
      if (a === null) {
        setError('RESPUESTA DEL PANEL SIN EL FORMATO ESPERADO');
        return;
      }
      const ahora = Date.now();
      setAgenda(a);
      setActualizada(ahora);
      setError(null);
      setAviso(null);
      conexionRef.current.informar(true);
      try {
        guardarCache(a, ahora);
      } catch {
        setError('NO SE PUDO GUARDAR LA COPIA LOCAL');
      }
    } catch (e) {
      if (e instanceof NoAlcanzable) {
        setAviso('PANEL NO ALCANZABLE');
        conexionRef.current.informar(false);
      } else if (e instanceof TokenInvalido) void conexionRef.current.caducado();
      else setError(e instanceof Error ? e.message : 'ERROR AL LEER LA AGENDA');
    } finally {
      enCurso.current = false;
      setCargando(false);
    }
  }, []);
  const actualizarRef = useRef(actualizar);
  actualizarRef.current = actualizar;

  // Arranque: muestra la caché.
  useEffect(() => {
    void leerCache().then((c) => {
      if (c !== null) {
        setAgenda(c.agenda);
        setActualizada(c.hora);
      }
      setCargada(true);
    });
  }, []);

  // Arranque con conexión y cada vez que se vuelve a estar emparejado.
  useEffect(() => {
    if (cargada && puede) void actualizar();
  }, [cargada, puede, actualizar]);

  // Al volver a primer plano.
  useEffect(() => {
    const sub = AppState.addEventListener('change', (e) => {
      if (e === 'active') void actualizarRef.current();
    });
    return () => sub.remove();
  }, []);

  const valor = useMemo(() => ({ agenda, actualizada, cargando, error, aviso, actualizar }), [agenda, actualizada, cargando, error, aviso, actualizar]);
  return <Ctx.Provider value={valor}>{children}</Ctx.Provider>;
}
