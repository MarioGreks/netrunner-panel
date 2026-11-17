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
  /** Con true el borde de la pantalla no abre el panel (menú de la bandeja). */
  borde_pausado: boolean;
  /** Ocultar miniventana y borde mientras haya algo a pantalla completa. */
  ocultar_pantalla_completa: boolean;
  /** Id. de aplicación (cliente) de Entra; no es secreto. */
  ms_client_id: string | null;
  /** Cuentas Microsoft conectadas (sin tokens); las gestiona Rust. */
  cuentas_ms: unknown[];
  /** Abrir el Outlook clásico minimizado al arrancar si no está abierto. */
  outlook_auto: boolean;
  /** Remitentes VIP: dirección exacta o dominio (`@uah.es`, `uah.es`). */
  correo_vip: string[];
  /** Palabras clave del asunto o del remitente. */
  correo_palabras: string[];
  /** Etiqueta de cada cuenta de Outlook (de DisplayName a etiqueta). */
  etiquetas_cuentas_correo: Record<string, string>;
}

export const obtenerConfig = () => invoke<Config>("get_config");
export const guardarConfig = (config: Config) => invoke<void>("set_config", { config });
export const alternarPanel = () => invoke<void>("alternar_panel");
export const estadoPantallaCompleta = () => invoke<boolean>("estado_pantalla_completa");
export const ocultarPanel = () => invoke<void>("ocultar_panel");
/** Fija el alto lógico de la miniventana (Rust lo limita a 96-220 y la recoloca). */
export const ajustarMini = (alto: number) => invoke<void>("ajustar_mini", { alto });
/** Abre el panel y le pide ir a un evento de la agenda (pestaña METEO). */
export const irAEventoPanel = (id: string) => invoke<void>("ir_a_evento", { id });
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
  /** Estable por ocurrencia: `uid|inicio UTC` */
  id: string;
  uid: string;
  titulo: string;
  /** RFC 3339 con el desfase de la zona local */
  inicio: string;
  fin: string;
  todo_el_dia: boolean;
  ubicacion: string | null;
  categorias: string[];
  /** Primeras ~200 letras de la descripción (aún no se muestra) */
  descripcion: string | null;
}

export interface EstadoCalendario {
  eventos: EventoCalendario[];
  ultima_sync: string | null;
  sincronizando: boolean;
  error: string | null;
}

export const estadoCalendario = () => invoke<EstadoCalendario>("estado_calendario");
export const sincronizarCalendario = () => invoke<void>("sincronizar_calendario");

export interface CuentaAtras {
  id_evento: string;
  uid: string;
  titulo: string;
  /** RFC 3339 */
  inicio: string;
  categorias: string[];
  /** ms UTC */
  creada: number;
}

export const estadoCuentas = () => invoke<CuentaAtras[]>("estado_cuentas");
export const marcarCuenta = (evento: EventoCalendario) => invoke<void>("marcar_cuenta", { evento });
export const desmarcarCuenta = (idEvento: string) => invoke<void>("desmarcar_cuenta", { idEvento });

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

export interface HoraMeteo {
  /** AAAA-MM-DDTHH:MM, hora local de la ciudad */
  hora: string;
  temperatura: number | null;
  prob_lluvia: number | null;
  codigo: number;
}

export interface EstadoMeteo {
  ciudad: Ciudad | null;
  actual: ActualMeteo | null;
  dias: DiaMeteo[];
  /** Próximas 24 horas (vacío con una caché antigua) */
  por_horas?: HoraMeteo[];
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

export type EstadoCuentaMs = "conectada" | "reconectar" | "bloqueada" | "error";

export interface CuentaMs {
  id: string;
  correo: string;
  nombre: string;
  etiqueta: string;
  estado: EstadoCuentaMs;
  detalle: string | null;
}

export const estadoCuentasMs = () => invoke<CuentaMs[]>("estado_cuentas_ms");
export const conectarCuentaMs = () => invoke<void>("conectar_cuenta_ms");
export const desconectarCuentaMs = (id: string) => invoke<void>("desconectar_cuenta_ms", { id });
export const renombrarCuentaMs = (id: string, etiqueta: string) => invoke<void>("renombrar_cuenta_ms", { id, etiqueta });
export const probarCuentaMs = (id: string) => invoke<string>("probar_cuenta_ms", { id });

export type EstadoOutlook = "conectado" | "cerrado" | "conectando" | "error";

export interface CorreoImportante {
  id: string;
  store_id: string;
  /** Etiqueta de la cuenta (la del usuario o el DisplayName). */
  cuenta: string;
  remitente: string;
  direccion: string;
  asunto: string;
  /** RFC 3339 con el desfase local */
  recibido: string;
  no_leido: boolean;
  fragmento: string;
  /** «VIP» o la palabra clave que coincidió */
  motivo: string;
}

export interface EstadoCorreo {
  estado: EstadoOutlook;
  detalle: string;
  correos: CorreoImportante[];
  ultima_sync: string | null;
  /** DisplayName de las cuentas detectadas en Outlook. */
  cuentas: string[];
}

export const estadoCorreo = () => invoke<EstadoCorreo>("estado_correo");
export const sincronizarCorreo = () => invoke<void>("sincronizar_correo");
export const abrirCorreo = (entryId: string, storeId: string) => invoke<void>("abrir_correo", { entryId, storeId });
/** Lanza el Outlook clásico minimizado (una vez). */
export const abrirOutlook = () => invoke<void>("abrir_outlook");
export const categoriasOutlook = () => invoke<string[]>("categorias_outlook");

export interface NuevoEvento {
  titulo: string;
  /** Hora local `AAAA-MM-DDTHH:MM` */
  inicio: string;
  fin: string;
  todo_el_dia: boolean;
  ubicacion: string | null;
  categorias: string[];
}

export const crearEvento = (evento: NuevoEvento) => invoke<void>("crear_evento", { evento });

export type EventoPanel =
  | "panel://mostrar"
  | "panel://ocultar"
  | "panel://ajustes"
  | "panel://ir_evento"
  | "config://cambio"
  | "pantalla_completa://cambio"
  | "tema://cambio"
  | "sistema://muestra"
  | "calendario://estado"
  | "cuentas://estado"
  | "cuentas_ms://estado"
  | "meteo://estado"
  | "notas://estado"
  | "musica://estado"
  | "accesos://estado"
  | "correo://estado"
  | "actualizador://progreso"
  | "actualizador://instalando";

export const alEvento = <T = unknown>(
  evento: EventoPanel,
  manejador: (carga: T) => void,
): Promise<UnlistenFn> => listen<T>(evento, (e) => manejador(e.payload));

// --- Actualizador ---
export interface InfoActualizacion {
  version: string;
  version_actual: string;
  notas: string | null;
  fecha: string | null;
}
export interface ProgresoActualizacion {
  descargado: number;
  total: number | null;
}
export const buscarActualizacion = () => invoke<InfoActualizacion | null>("buscar_actualizacion");
export const infoActualizacion = () => invoke<InfoActualizacion | null>("info_actualizacion");
export const instalarActualizacion = () => invoke<void>("instalar_actualizacion");
export const posponerActualizacion = () => invoke<void>("posponer_actualizacion");
