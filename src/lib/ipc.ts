// Envoltorios tipados de comandos y eventos de Tauri.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Borde = "left" | "right" | "top";

export interface Ciudad {
  nombre: string;
  pais: string;
  latitud: number;
  longitud: number;
}

export interface Config {
  edge: Borde;
  autostart: boolean;
  /** URL ICS privada: no se muestra ni se registra en ningún sitio. */
  ics_url: string | null;
  ciudad: Ciudad | null;
}

export const obtenerConfig = () => invoke<Config>("get_config");
export const guardarConfig = (config: Config) => invoke<void>("set_config", { config });
export const alternarPanel = () => invoke<void>("alternar_panel");
export const ocultarPanel = () => invoke<void>("ocultar_panel");

export interface Disco {
  nombre: string;
  montaje: string;
  libre: number;
  total: number;
}

export interface Bateria {
  porcentaje: number;
  cargando: boolean;
  minutos_restantes: number | null;
}

export interface MuestraSistema {
  cpu_total: number;
  cpu_nucleos: number[];
  ram_usada: number;
  ram_total: number;
  gpu_uso: number | null;
  gpu_temp: number | null;
  discos: Disco[];
  bateria: Bateria | null;
}

export const muestraSistema = () => invoke<MuestraSistema>("muestra_sistema");

export interface EventoCalendario {
  titulo: string;
  /** RFC 3339 con el desfase de la zona local */
  inicio: string;
  fin: string;
  todo_el_dia: boolean;
  ubicacion: string | null;
}

export interface EstadoCalendario {
  eventos: EventoCalendario[];
  ultima_sync: string | null;
  sincronizando: boolean;
  error: string | null;
}

export const estadoCalendario = () => invoke<EstadoCalendario>("estado_calendario");
export const sincronizarCalendario = () => invoke<void>("sincronizar_calendario");

export interface ActualMeteo {
  temperatura: number;
  sensacion: number;
  humedad: number;
  viento: number;
  codigo: number;
  es_de_dia: boolean;
  texto: string;
  icono: string;
}

export interface DiaMeteo {
  fecha: string;
  codigo: number;
  maxima: number | null;
  minima: number | null;
  prob_lluvia: number | null;
  texto: string;
  icono: string;
}

export interface EstadoMeteo {
  ciudad: Ciudad | null;
  actual: ActualMeteo | null;
  dias: DiaMeteo[];
  actualizado: string | null;
  cargando: boolean;
  error: string | null;
}

export const estadoMeteo = () => invoke<EstadoMeteo>("estado_meteo");
export const buscarCiudad = (texto: string) => invoke<Ciudad[]>("buscar_ciudad", { texto });

export type EventoPanel =
  | "panel://mostrar"
  | "panel://ocultar"
  | "config://cambio"
  | "sistema://muestra"
  | "calendario://estado"
  | "meteo://estado";

export const alEvento = <T = unknown>(
  evento: EventoPanel,
  manejador: (carga: T) => void,
): Promise<UnlistenFn> => listen<T>(evento, (e) => manejador(e.payload));
