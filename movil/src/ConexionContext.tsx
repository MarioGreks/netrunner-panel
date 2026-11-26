import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';
import { cerrarSesion, clientIdConfigurado, obtenerToken, perfil, SinSesion, useLoginMicrosoft } from './microsoft';
import { carpetaGuardada, guardarCarpeta, olvidarCarpeta as olvidarGuardada, type Carpeta } from './onedrive';

type Estado = 'sin-cliente' | 'sin-sesion' | 'conectando' | 'conectada';

interface Conexion {
  estado: Estado;
  cuenta?: { nombre: string; correo: string };
  carpeta: Carpeta | null;
  error: string | null;
  conectar: () => void;
  desconectar: () => Promise<void>;
  elegirCarpeta: (c: Carpeta) => Promise<void>;
  olvidarCarpeta: () => Promise<void>;
}

const Ctx = createContext<Conexion | null>(null);

export function useConexion(): Conexion {
  const c = useContext(Ctx);
  if (c === null) throw new Error('Falta ConexionProvider');
  return c;
}

const textoError = (e: unknown) => (e instanceof SinSesion ? null : e instanceof Error ? e.message : 'ERROR DESCONOCIDO');

export function ConexionProvider({ children }: { children: ReactNode }) {
  const [estado, setEstado] = useState<Estado>(clientIdConfigurado() ? 'conectando' : 'sin-cliente');
  const [cuenta, setCuenta] = useState<Conexion['cuenta']>();
  const [carpeta, setCarpeta] = useState<Carpeta | null>(null);
  const [error, setError] = useState<string | null>(null);

  const cargarPerfil = useCallback(async () => {
    try {
      setCuenta(await perfil());
      setEstado('conectada');
      setError(null);
    } catch (e) {
      setEstado('sin-sesion');
      setError(textoError(e));
    }
  }, []);

  const { iniciar, listo } = useLoginMicrosoft((fallo) => {
    if (fallo === null) void cargarPerfil();
    else {
      setEstado('sin-sesion');
      setError(fallo ?? null);
    }
  });

  useEffect(() => {
    void carpetaGuardada().then(setCarpeta);
    if (!clientIdConfigurado()) return;
    obtenerToken().then(cargarPerfil, (e) => {
      setEstado('sin-sesion');
      setError(textoError(e));
    });
  }, [cargarPerfil]);

  const valor: Conexion = {
    estado,
    cuenta,
    carpeta,
    error,
    conectar: () => {
      if (!listo) return;
      setError(null);
      setEstado('conectando');
      void iniciar();
    },
    desconectar: async () => {
      await cerrarSesion();
      setCuenta(undefined);
      setEstado('sin-sesion');
    },
    elegirCarpeta: async (c) => {
      await guardarCarpeta(c);
      setCarpeta(c);
    },
    olvidarCarpeta: async () => {
      await olvidarGuardada();
      setCarpeta(null);
    },
  };
  return <Ctx.Provider value={valor}>{children}</Ctx.Provider>;
}

/** Texto de las pantallas aún vacías según el estado de la conexión. */
export function textoVacio(c: Conexion): string {
  if (c.estado !== 'conectada') return 'CONECTA TU CUENTA EN AJUSTES';
  return c.carpeta === null ? 'ELIGE LA CARPETA EN AJUSTES' : 'PRÓXIMAMENTE';
}
