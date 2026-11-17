// Estado de interfaz compartido entre equipos: tira METEO desplegada y última pestaña.
// Se carga al arrancar; cambiarlo avisa al backend con 1 s de retardo para agrupar cambios
// (el backend lo guarda y lo sincroniza). Un cambio remoto solo llega a la tira: nunca se
// cambia de pestaña mientras el usuario usa el panel.
import { alEvento, estadoInterfaz, guardarInterfaz, type EstadoInterfaz } from "./ipc";

const RETARDO_GUARDADO = 1000;

let estado: { tira: boolean; pestana: string } = { tira: false, pestana: "" };
let temporizador: number | undefined;
const oyentesTira = new Set<(abierta: boolean) => void>();

/** Lee el estado guardado. Devuelve la pestaña inicial ("" si no hay). */
export async function cargarInterfaz(): Promise<string> {
  try {
    const e = await estadoInterfaz();
    estado = { tira: e.tira_meteo_abierta, pestana: e.ultima_pestana };
  } catch {
    /* sin estado guardado: valores por defecto */
  }
  void alEvento<EstadoInterfaz>("interfaz://estado", (e) => {
    // Solo la tira; la pestaña se actualiza en memoria para no deshacerla al guardar
    const cambiaTira = e.tira_meteo_abierta !== estado.tira;
    estado = { tira: e.tira_meteo_abierta, pestana: estado.pestana };
    if (cambiaTira) oyentesTira.forEach((f) => f(estado.tira));
  });
  return estado.pestana;
}

export const tiraAbiertaGuardada = (): boolean => estado.tira;

/** Avisa de que la tira cambió en otro equipo. Devuelve la función que quita el aviso. */
export function alCambiarTiraRemota(f: (abierta: boolean) => void): () => void {
  oyentesTira.add(f);
  return () => oyentesTira.delete(f);
}

const programar = () => {
  clearTimeout(temporizador);
  temporizador = window.setTimeout(() => {
    void guardarInterfaz(estado.tira, estado.pestana).catch(() => {});
  }, RETARDO_GUARDADO);
};

export function recordarTira(abierta: boolean): void {
  if (abierta === estado.tira) return;
  estado.tira = abierta;
  programar();
}

export function recordarPestana(id: string): void {
  if (id === estado.pestana) return;
  estado.pestana = id;
  programar();
}
