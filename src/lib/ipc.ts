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
  /** Carpeta de OneDrive para sincronizar las notas; null = solo local. */
  carpeta_sync: string | null;
  /** Id del tema de color; un valor desconocido se trata como «netrunner». */
  tema: string;
  /** Opacidad del fondo del panel, entre 0.55 y 1. */
  opacidad: number;
}

export const obtenerConfig = () => invoke<Config>("get_config");
export const guardarConfig = (config: Config) => invoke<void>("set_config", { config });
export const alternarPanel = () => invoke<void>("alternar_panel");
export const ocultarPanel = () => invoke<void>("ocultar_panel");
/** Devuelve la ruta de los datos locales; con `abrir` también abre la carpeta en el Explorador. */
export const abrirCarpetaDatos = (abrir: boolean) => invoke<string>("abrir_carpeta_datos", { abrir });

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

export interface Tarea {
  id: string;
  texto: string;
  hecha: boolean;
  orden: number;
  creada: number;
  modificada: number;
  borrada: boolean;
}

export interface Nota {
  id: string;
  titulo: string;
  cuerpo: string;
  creada: number;
  modificada: number;
  borrada: boolean;
}

export interface EstadoNotas {
  tareas: Tarea[];
  notas: Nota[];
  /** Milisegundos UTC */
  ultima_sync: number | null;
  sincronizando: boolean;
  error: string | null;
  carpeta_sync: string | null;
}

export const estadoNotas = () => invoke<EstadoNotas>("estado_notas");
export const guardarTarea = (tarea: Tarea) => invoke<void>("guardar_tarea", { tarea });
export const guardarNota = (nota: Nota) => invoke<void>("guardar_nota", { nota });
export const borrarTarea = (id: string) => invoke<void>("borrar_tarea", { id });
export const borrarNota = (id: string) => invoke<void>("borrar_nota", { id });
export const carpetaOnedrive = () => invoke<string | null>("carpeta_onedrive");
export const sincronizarNotas = () => invoke<void>("sincronizar_notas");

export interface EstadoMusica {
  titulo: string;
  artista: string;
  album: string;
  origen: string;
  reproduciendo: boolean;
  /** Segundos */
  posicion: number;
  duracion: number;
  puede_anterior: boolean;
  puede_siguiente: boolean;
  /** Data URL de la carátula */
  caratula: string | null;
}

export const estadoMusica = () => invoke<EstadoMusica | null>("estado_musica");
export const musicaAlternar = () => invoke<void>("musica_alternar");
export const musicaSiguiente = () => invoke<void>("musica_siguiente");
export const musicaAnterior = () => invoke<void>("musica_anterior");

export type TipoAcceso = "app" | "carpeta" | "archivo" | "url";

export interface Acceso {
  id: string;
  nombre: string;
  destino: string;
  tipo: TipoAcceso;
  /** Data URL PNG; las URLs no llevan */
  icono: string | null;
  orden: number;
}

export const estadoAccesos = () => invoke<Acceso[]>("estado_accesos");
export const anadirAcceso = (destino: string, tipo: TipoAcceso) => invoke<void>("anadir_acceso", { destino, tipo });
export const renombrarAcceso = (id: string, nombre: string) => invoke<void>("renombrar_acceso", { id, nombre });
export const borrarAcceso = (id: string) => invoke<void>("borrar_acceso", { id });
export const reordenarAccesos = (ids: string[]) => invoke<void>("reordenar_accesos", { ids });
export const abrirAcceso = (id: string) => invoke<void>("abrir_acceso", { id });

export type EventoPanel =
  | "panel://mostrar"
  | "panel://ocultar"
  | "config://cambio"
  | "tema://cambio"
  | "sistema://muestra"
  | "calendario://estado"
  | "meteo://estado"
  | "notas://estado"
  | "musica://estado"
  | "accesos://estado";

export const alEvento = <T = unknown>(
  evento: EventoPanel,
  manejador: (carga: T) => void,
): Promise<UnlistenFn> => listen<T>(evento, (e) => manejador(e.payload));
