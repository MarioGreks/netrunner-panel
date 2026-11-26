import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { AppState } from 'react-native';
import {
  PUERTO_DEFECTO,
  TokenInvalido,
  configuracion,
  emparejar as emparejarPanel,
  estadoPanel,
  guardarConfiguracion,
  hayToken,
  olvidarPanel,
  olvidarToken,
  type ConfigPanel,
} from './panel';

type Estado = 'sin-emparejar' | 'emparejado';

interface Conexion {
  estado: Estado;
  /** null = sin comprobar aún. */
  alcanzable: boolean | null;
  panel: ConfigPanel | null;
  version?: string;
  error: string | null;
  emparejar: (host: string, puerto: number, codigo: string) => Promise<void>;
  comprobar: () => Promise<void>;
  /** Cambia IP/puerto sin re-emparejar. */
  cambiarDireccion: (host: string, puerto: number) => Promise<void>;
  olvidar: () => Promise<void>;
  /** Para quien sincroniza: resultado de una llamada al panel. */
  informar: (alcanzable: boolean) => void;
  /** Para quien sincroniza: una llamada dio TokenInvalido. */
  caducado: () => Promise<void>;
}

const Ctx = createContext<Conexion | null>(null);

export function useConexion(): Conexion {
  const c = useContext(Ctx);
  if (c === null) throw new Error('Falta ConexionProvider');
  return c;
}

export { PUERTO_DEFECTO };

export function ConexionProvider({ children }: { children: ReactNode }) {
  const [estado, setEstado] = useState<Estado>('sin-emparejar');
  const [alcanzable, setAlcanzable] = useState<boolean | null>(null);
  const [panel, setPanel] = useState<ConfigPanel | null>(null);
  const [version, setVersion] = useState<string | undefined>();
  const [error, setError] = useState<string | null>(null);
  const emparejado = useRef(false);

  const caducado = useCallback(async () => {
    await olvidarToken();
    emparejado.current = false;
    setEstado('sin-emparejar');
    setAlcanzable(null);
    setVersion(undefined);
    setError(new TokenInvalido().message);
  }, []);

  const comprobar = useCallback(async () => {
    if (!emparejado.current) return;
    try {
      const e = await estadoPanel();
      setAlcanzable(true);
      setVersion(e.version);
      setError(null);
    } catch (e) {
      if (e instanceof TokenInvalido) await caducado();
      else setAlcanzable(false);
    }
  }, [caducado]);

  // Arranque: carga la configuración y comprueba.
  useEffect(() => {
    void (async () => {
      const [c, t] = await Promise.all([configuracion(), hayToken()]);
      setPanel(c);
      if (c !== null && t) {
        emparejado.current = true;
        setEstado('emparejado');
        await comprobar();
      }
    })();
  }, [comprobar]);

  // Al volver a primer plano.
  useEffect(() => {
    const sub = AppState.addEventListener('change', (e) => {
      if (e === 'active') void comprobar();
    });
    return () => sub.remove();
  }, [comprobar]);

  const valor: Conexion = useMemo(
    () => ({
      estado,
      alcanzable,
      panel,
      version,
      error,
      emparejar: async (host, puerto, codigo) => {
        setError(null);
        await emparejarPanel(host, puerto, codigo); // lanza con el texto ya traducido
        emparejado.current = true;
        setPanel({ host, puerto });
        setEstado('emparejado');
        await comprobar();
      },
      comprobar,
      cambiarDireccion: async (host, puerto) => {
        await guardarConfiguracion({ host, puerto });
        setPanel({ host, puerto });
        setAlcanzable(null);
        await comprobar();
      },
      olvidar: async () => {
        await olvidarPanel();
        emparejado.current = false;
        setPanel(null);
        setEstado('sin-emparejar');
        setAlcanzable(null);
        setVersion(undefined);
        setError(null);
      },
      informar: setAlcanzable,
      caducado,
    }),
    [estado, alcanzable, panel, version, error, comprobar, caducado],
  );
  return <Ctx.Provider value={valor}>{children}</Ctx.Provider>;
}

/** Texto de las pantallas aún vacías según el estado de la conexión. */
export function textoVacio(c: Conexion): string {
  return c.estado === 'emparejado' ? 'PRÓXIMAMENTE' : 'EMPAREJA EL MÓVIL CON EL PANEL EN AJUSTES';
}
