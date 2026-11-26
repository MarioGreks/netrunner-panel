import * as Crypto from 'expo-crypto';
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { AppState } from 'react-native';
import { useConexion } from '../ConexionContext';
import { leerJson, escribirJson } from '../onedrive';
import { cargarCopia, cargarUltimaSync, guardarCopia, guardarUltimaSync } from './almacen';
import { esDatosLegible, fusionar, igual, normalizar, purgar, type Datos, type Nota, type Tarea } from './fusion';

const ARCHIVO = 'notas.json';
const RETARDO_CAMBIO = 2000;
const INTERVALO_SYNC = 5 * 60 * 1000;
const MAX_INTENTOS = 3;

interface Notas {
  tareasPendientes: Tarea[];
  tareasHechas: Tarea[];
  notas: Nota[];
  sincronizando: boolean;
  ultimaSync: number | null;
  error: string | null;
  sincronizar: () => Promise<void>;
  crearTarea: (texto: string) => void;
  editarTarea: (id: string, texto: string) => void;
  alternarTarea: (id: string) => void;
  borrarTarea: (id: string) => void;
  /** Devuelve el id de la nota creada. */
  crearNota: (titulo: string, cuerpo: string) => string;
  editarNota: (id: string, titulo: string, cuerpo: string) => void;
  borrarNota: (id: string) => void;
}

const Ctx = createContext<Notas | null>(null);

export function useNotas(): Notas {
  const c = useContext(Ctx);
  if (c === null) throw new Error('Falta NotasProvider');
  return c;
}

const maxOrden = (d: Datos) => d.tareas.reduce((m, t) => (t.borrada ? m : Math.max(m, t.orden)), 0);

export function NotasProvider({ children }: { children: ReactNode }) {
  const conexion = useConexion();
  const puede = conexion.estado === 'conectada' && conexion.carpeta !== null;
  const puedeRef = useRef(puede);
  puedeRef.current = puede;

  // Fuente de verdad en memoria (incluye marcas de borrado); `datos` es su espejo para pintar.
  const ref = useRef<Datos>({ tareas: [], notas: [] });
  const [datos, setDatos] = useState<Datos>(ref.current);
  const [cargada, setCargada] = useState(false);
  const [sincronizando, setSincronizando] = useState(false);
  const [ultimaSync, setUltimaSync] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const enCurso = useRef(false);
  const pendiente = useRef(false);
  const temporizador = useRef<ReturnType<typeof setTimeout> | null>(null);

  const poner = useCallback((d: Datos) => {
    ref.current = d;
    setDatos(d);
    try {
      guardarCopia(d);
    } catch {
      setError('NO SE PUDO GUARDAR LA COPIA LOCAL');
    }
  }, []);

  const sincronizarRef = useRef<() => Promise<void>>(async () => {});

  const sincronizar = useCallback(async (): Promise<void> => {
    if (!puedeRef.current) return;
    if (enCurso.current) {
      pendiente.current = true;
      return;
    }
    enCurso.current = true;
    setSincronizando(true);
    try {
      do {
        pendiente.current = false;
        await una();
      } while (pendiente.current && puedeRef.current);
    } finally {
      enCurso.current = false;
      setSincronizando(false);
    }

    async function una(): Promise<void> {
      for (let intento = 1; intento <= MAX_INTENTOS; intento++) {
        let remoto: { datos: unknown; etag: string | null };
        try {
          remoto = await leerJson<unknown>(ARCHIVO);
        } catch (e) {
          // remoto ilegible o corrupto: no se escribe nada
          setError(e instanceof Error ? e.message : 'ERROR DE SINCRONIZACIÓN');
          return;
        }
        if (remoto.datos !== null && !esDatosLegible(remoto.datos)) {
          setError(`${ARCHIVO} NO TIENE EL FORMATO DEL PANEL: NO SE TOCA`);
          return;
        }
        try {
          const local = ref.current;
          const rem = remoto.datos === null ? null : normalizar(remoto.datos);
          const fusionado = purgar(rem !== null ? fusionar(local, rem) : local, Date.now());
          if (rem === null && fusionado.tareas.length + fusionado.notas.length === 0) {
            // posible carpeta equivocada: no se crea nada
          } else if (rem === null || !igual(rem, fusionado)) {
            const r = await escribirJson(ARCHIVO, fusionado, remoto.etag);
            if (!r.ok) {
              if (intento === MAX_INTENTOS) {
                setError('CONFLICTO: REINTENTA');
                return;
              }
              continue;
            }
          }
          // Los cambios hechos durante el await ya están en ref.current: se fusionan encima (ganan por `modificada`)
          // y los sube la sincronización que ellos mismos programaron.
          poner(purgar(fusionar(ref.current, fusionado), Date.now()));
          const ahora = Date.now();
          guardarUltimaSync(ahora);
          setUltimaSync(ahora);
          setError(null);
          return;
        } catch (e) {
          setError(e instanceof Error ? e.message : 'ERROR DE SINCRONIZACIÓN');
          return;
        }
      }
    }
  }, [poner]);
  sincronizarRef.current = sincronizar;

  const programar = useCallback(() => {
    if (temporizador.current !== null) clearTimeout(temporizador.current);
    temporizador.current = setTimeout(() => void sincronizarRef.current(), RETARDO_CAMBIO);
  }, []);

  // Carga de la copia local al arrancar.
  useEffect(() => {
    void (async () => {
      const [d, u] = await Promise.all([cargarCopia(), cargarUltimaSync()]);
      // por si el usuario ya cambió algo antes de terminar de leer
      ref.current = fusionar(ref.current, d);
      setDatos(ref.current);
      setUltimaSync(u);
      setCargada(true);
    })();
    return () => {
      if (temporizador.current !== null) clearTimeout(temporizador.current);
    };
  }, []);

  // Arranque con conexión y cada vez que se vuelve a tener carpeta/sesión.
  useEffect(() => {
    if (cargada && puede) void sincronizar();
  }, [cargada, puede, sincronizar]);

  // Primer plano y cada 5 min mientras la app está activa.
  useEffect(() => {
    if (!cargada) return;
    let intervalo: ReturnType<typeof setInterval> | null = null;
    const parar = () => {
      if (intervalo !== null) clearInterval(intervalo);
      intervalo = null;
    };
    const arrancar = () => {
      parar();
      intervalo = setInterval(() => void sincronizarRef.current(), INTERVALO_SYNC);
    };
    if (AppState.currentState === 'active') arrancar();
    const sub = AppState.addEventListener('change', (e) => {
      if (e === 'active') {
        arrancar();
        void sincronizarRef.current();
      } else parar();
    });
    return () => {
      parar();
      sub.remove();
    };
  }, [cargada]);

  const mutar = useCallback(
    (f: (d: Datos, ahora: number) => Datos) => {
      poner(f(ref.current, Date.now()));
      programar();
    },
    [poner, programar],
  );

  const valor: Notas = useMemo(() => {
    const t = datos.tareas.filter((x) => !x.borrada);
    return {
      tareasPendientes: t.filter((x) => !x.hecha).sort((a, b) => a.orden - b.orden || a.creada - b.creada),
      tareasHechas: t.filter((x) => x.hecha).sort((a, b) => b.modificada - a.modificada),
      notas: datos.notas.filter((x) => !x.borrada).sort((a, b) => b.modificada - a.modificada),
      sincronizando,
      ultimaSync,
      error,
      sincronizar,
      crearTarea: (texto) => {
        const limpio = texto.trim();
        if (limpio === '') return;
        mutar((d, ahora) => ({
          ...d,
          tareas: [...d.tareas, { id: Crypto.randomUUID(), texto: limpio, hecha: false, orden: maxOrden(d) + 1, creada: ahora, modificada: ahora, borrada: false }],
        }));
      },
      editarTarea: (id, texto) => mutar((d, ahora) => ({ ...d, tareas: d.tareas.map((x) => (x.id === id ? { ...x, texto, modificada: ahora, borrada: false } : x)) })),
      alternarTarea: (id) =>
        mutar((d, ahora) => ({
          ...d,
          tareas: d.tareas.map((x) =>
            x.id !== id ? x : { ...x, hecha: !x.hecha, orden: x.hecha ? maxOrden(d) + 1 : x.orden, modificada: ahora, borrada: false },
          ),
        })),
      borrarTarea: (id) => mutar((d, ahora) => ({ ...d, tareas: d.tareas.map((x) => (x.id === id ? { ...x, borrada: true, texto: '', modificada: ahora } : x)) })),
      crearNota: (titulo, cuerpo) => {
        const id = Crypto.randomUUID();
        mutar((d, ahora) => ({ ...d, notas: [...d.notas, { id, titulo, cuerpo, creada: ahora, modificada: ahora, borrada: false }] }));
        return id;
      },
      editarNota: (id, titulo, cuerpo) => mutar((d, ahora) => ({ ...d, notas: d.notas.map((x) => (x.id === id ? { ...x, titulo, cuerpo, modificada: ahora, borrada: false } : x)) })),
      borrarNota: (id) => mutar((d, ahora) => ({ ...d, notas: d.notas.map((x) => (x.id === id ? { ...x, borrada: true, titulo: '', cuerpo: '', modificada: ahora } : x)) })),
    };
  }, [datos, sincronizando, ultimaSync, error, sincronizar, mutar]);

  return <Ctx.Provider value={valor}>{children}</Ctx.Provider>;
}
