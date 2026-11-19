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

export interface EstadoInterfaz {
  tira_meteo_abierta: boolean;
  ultima_pestana: string;
  /** Milisegundos UTC */
  modificada: number;
}

export const estadoInterfaz = () => invoke<EstadoInterfaz>("estado_interfaz");
export const guardarInterfaz = (tiraMeteoAbierta: boolean, ultimaPestana: string) =>
  invoke<void>("guardar_interfaz", { tiraMeteoAbierta, ultimaPestana });

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
  | "consumo://estado"
  | "consumo://contexto"
  | "consumo://detalle"
  | "consumo://diagnostico"
  | "consumo://datos"
  | "consumo://ajustes"
  | "consumo://aviso"
  | "cuentas://estado"
  | "cuentas_ms://estado"
  | "meteo://estado"
  | "notas://estado"
  | "interfaz://estado"
  | "ajustes://cambio"
  | "musica://estado"
  | "accesos://estado"
  | "correo://estado"
  | "actualizador://progreso"
  | "actualizador://instalando";

export const alEvento = <T = unknown>(
  evento: EventoPanel,
  manejador: (carga: T) => void,
): Promise<UnlistenFn> => listen<T>(evento, (e) => manejador(e.payload));

// --- Consumo de Claude ---
export type NivelConsumo = "bien" | "encima" | "riesgo";
/** Una ventana de límite (semanal o de 5 h) ya calculada. */
export interface VentanaConsumo {
  hayDatos: boolean;
  pct: number;
  /** % esperado por el tiempo transcurrido. */
  esperado: number;
  /** % estimado al acabar el día (semanal) o la ventana (5 h). */
  fin: number | null;
  /** 0..1. */
  riesgo: number;
  nivel: NivelConsumo;
  /** % estimado al acabar la semana (solo semanal). */
  proyeccionSemana: number | null;
  /** % por encima del 100 % a ese ritmo. */
  exceso: number;
  textoRitmo: string | null;
  textoSemana: string | null;
  textoSinLimite: string | null;
  resetsAt: number | null;
  segundosHastaReset: number | null;
  textoFalta: string;
}
export interface EstadoConsumo {
  /** Sin datos de Claude (fichero ausente, vacío o de hace más de 6 h). */
  sinDatos: boolean;
  /** Segundos desde la última escritura del plugin. */
  antiguedad: number | null;
  semanal: VentanaConsumo;
  cincoHoras: VentanaConsumo;
}
export const estadoConsumo = () => invoke<EstadoConsumo>("estado_consumo");
/** Se emite cuando cambia usage.json (no cada segundo: la cuenta atrás se recalcula con `estadoConsumo`). */
export const alEstadoConsumo = (cb: (estado: EstadoConsumo) => void) =>
  alEvento<EstadoConsumo>("consumo://estado", cb);

/** Lo lleno que está el contexto de una sesión: bien (< 50 %), medio (< 80 %) o alto. */
export type NivelContexto = "bien" | "medio" | "alto";
/** Una sesión viva de Claude Code con el contexto que lleva. */
export interface SesionContexto {
  /** Id de la sesión (nombre de su transcripción). */
  id: string;
  proyecto: string;
  tokens: number;
  /** Tokens sobre la ventana máxima, de 0 a 1. */
  fraccion: number;
  nivel: NivelContexto;
  /** Título del chat en la app de Claude; si no hay, el proyecto. */
  nombre: string;
  /** Tiene chat en la app de Claude (se puede abrir con `abrirChatConsumo`). */
  tieneChat: boolean;
}
/** Último contexto calculado (vacío hasta que `vistaConsumo(true)` provoque el primer cálculo). */
export const contextoConsumo = () => invoke<SesionContexto[]>("contexto_consumo");
/** Abre el chat de la sesión en la app de Claude. Falla si la sesión no tiene chat. */
export const abrirChatConsumo = (id: string) => invoke<void>("abrir_chat_consumo", { id });
/** Avisa de que la pestaña CONSUMO o la ventana flotante se ven (true) o se ocultan (false): solo con la vista a la vista se calcula el contexto. */
export const vistaConsumo = (visible: boolean) => invoke<void>("vista_consumo", { visible });
/** Se emite cada 5 s (si cambió algo) mientras la vista está a la vista. */
export const alContextoConsumo = (cb: (sesiones: SesionContexto[]) => void) =>
  alEvento<SesionContexto[]>("consumo://contexto", cb);

// --- Detalle del consumo: reparto del % semanal (todos los equipos) ---
/** Nombre y % (un modelo, una tarea, una sesión...). Los nombres especiales: "(otros)" = no atribuido, "(resto)" = proyectos menores juntos, "(pequeñas)" = sesiones diminutas juntas. */
export interface ParteConsumo {
  nombre: string;
  pct: number;
}
export interface GrupoProyectoConsumo {
  nombre: string;
  pct: number;
  /** De mayor a menor. */
  sesiones: ParteConsumo[];
}
export interface SerieConsumo {
  nombre: string;
  valores: number[];
}
export interface PorHoraConsumo {
  /** Instante (s) de la medianoche local de hace 6 días: la hora i empieza en inicio + 3600·i. */
  inicio: number;
  /** 168 valores (% de esa hora) por serie. */
  series: SerieConsumo[];
}
export interface EquipoConsumo {
  nombre: string;
  pct: number;
  /** Última actividad (s desde la época). */
  ultima: number | null;
}
export interface DiasConsumo {
  /** `AAAA-MM-DD` de cada día natural de la semana (8 si el reset no es a medianoche). */
  fechas: string[];
  total: number[];
  /** % de cada día por proyecto (los 5 mayores, "(resto)" y "(otros)"). */
  series: SerieConsumo[];
}
export interface PreviasConsumo {
  reset: number;
  pct: number;
  proyectos: GrupoProyectoConsumo[];
  equipos: ParteConsumo[];
  modelos: ParteConsumo[];
  tareas: ParteConsumo[];
}
export interface DetalleConsumo {
  /** Fin de la semana del reparto (s): es la vigente si coincide (±10 min) con `semanal.resetsAt`. */
  reset: number;
  pct: number | null;
  /** Cuándo se calculó (s). */
  calculado: number;
  proyectos: GrupoProyectoConsumo[];
  porHora: PorHoraConsumo;
  equipos: EquipoConsumo[];
  modelos: ParteConsumo[];
  tareas: ParteConsumo[];
  dias: DiasConsumo;
  /** Equipos cuyo Consumo no guarda el tipo de tarea. */
  sinTarea: string[];
  previas: PreviasConsumo | null;
}
/** Último reparto calculado (no calcula; null hasta la primera lectura de transcripciones o sin semana). */
export const detalleConsumo = () => invoke<DetalleConsumo | null>("detalle_consumo");
/** Se emite tras cada lectura de transcripciones (cada ~10 min), sin carga: pedir `detalleConsumo()`. */
export const alDetalleConsumo = (cb: () => void) => alEvento<null>("consumo://detalle", () => cb());

// --- Plugin «uso-consumo» y espacio en disco ---
export type CodigoDiagnosticoConsumo =
  | "ok"
  | "fallo_carga"
  | "sin_claude"
  | "error"
  | "sin_plugin"
  | "desactivado"
  | "sin_datos";
export interface DiagnosticoConsumo {
  codigo: CodigoDiagnosticoConsumo;
  /** Texto para el usuario (puede tener saltos de línea). */
  texto: string;
  /** Se puede ofrecer «Instalar/Arreglar plugin» (sin_plugin o fallo_carga, con `claude`). */
  puedeInstalar: boolean;
}
/** Diagnóstico a petición (llama a `claude plugin list`: tarda unos segundos; también devuelve «ok»). */
export const diagnosticoConsumo = () => invoke<DiagnosticoConsumo>("diagnostico_consumo");
/** Instala o arregla el plugin (hasta ~6 min en el peor caso). Resuelve con el texto de éxito; rechaza con el texto de error. Solo a petición del usuario. */
export const instalarPluginConsumo = () => invoke<string>("instalar_plugin_consumo");
/** Se emite (una vez, no se repite hasta que vuelvan datos o pasen 6 h) cuando, con Claude abierto, faltan datos y el diagnóstico automático no es «ok». No instala nada. */
export const alDiagnosticoConsumo = (cb: (d: DiagnosticoConsumo) => void) =>
  alEvento<DiagnosticoConsumo>("consumo://diagnostico", cb);

export type NivelDatosConsumo = "ok" | "aviso" | "lleno";
export interface DatosDisco {
  /** GB que ocupan los datos (null hasta la primera medida, ~30 s tras arrancar). */
  gb: number | null;
  /** Máximo elegido en la app Consumo (0 = sin máximo). */
  gbMax: number;
  /** GB libres en la unidad de los datos (null si no se pudo saber). */
  libre: number | null;
  /** «◔ 2,50 / 5 GB» (vacío sin medir). */
  texto: string;
  /** 0-1 respecto al máximo (null sin máximo). */
  fraccion: number | null;
  nivel: NivelDatosConsumo;
}
/** Lo último medido (se mide como mucho cada 30 min). */
export const datosConsumo = () => invoke<DatosDisco>("datos_consumo");
/** Se emite una vez por arranque al pasar del máximo de GB. */
export const alDatosConsumo = (cb: (d: DatosDisco) => void) => alEvento<DatosDisco>("consumo://datos", cb);

/** Ventana flotante con la información completa de Consumo (encargo 6): el comando aún no existe en Rust. */
export const abrirVentanaConsumo = () => invoke<void>("abrir_ventana_consumo");

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

// --- Ajustes de Consumo y aviso del 80 % / 100 % ---
export type CtxMaxConsumo = 200000 | 500000 | 1000000;
export interface AjustesConsumo {
  ctxMax: CtxMaxConsumo;
  /** Avisos al 80 % y al 100 %. */
  avisos: boolean;
  /** Máximo de espacio de datos en GB (0 = sin máximo). */
  gbMax: number;
  /** Carpetas de sincronización, la principal primero. */
  carpetasSync: string[];
  /** La que se usa si no se elige ninguna (solo al leer). */
  carpetaDefecto: string;
}
export const ajustesConsumo = () => invoke<AjustesConsumo>("ajustes_consumo");
/** Guarda y aplica sin reiniciar; resuelve con lo que quedó guardado. Rechaza con el texto del fallo. */
export const guardarAjustesConsumo = (ajustes: AjustesConsumo) =>
  invoke<AjustesConsumo>("guardar_ajustes_consumo", { ajustes });
/** Se emite al guardar los ajustes de Consumo. */
export const alAjustesConsumo = (cb: () => void) => alEvento<null>("consumo://ajustes", () => cb());

export interface AvisoConsumo {
  titulo: string;
  texto: string;
  /** Límite alcanzado (100 %): se pinta en rojo. */
  alerta: boolean;
}
/** Lo que enseña la ventana de aviso de consumo (null si ya se cerró). */
export const avisoConsumoActual = () => invoke<AvisoConsumo | null>("aviso_consumo_actual");
export const cerrarAvisoConsumo = () => invoke<void>("cerrar_aviso_consumo");
/** La ventana de aviso ya abierta recibe un texto nuevo: volver a pedir `avisoConsumoActual()`. */
export const alAvisoConsumo = (cb: () => void) => alEvento<null>("consumo://aviso", () => cb());
