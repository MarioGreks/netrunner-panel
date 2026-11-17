// Backend del instalador. La interfaz solo usa `Backend`; hay dos implementaciones:
//
// 1. Tauri (ventana real): se activa si existe `window.__TAURI_INTERNALS__`.
//    CONTRATO con el backend Rust (encargo 2):
//      - comando `estado_instalador`  () -> EstadoInstalador
//      - comando `instalar`           ({ opciones: OpcionesInstalacion }) -> () ; Err(String) en español si falla
//      - comando `abrir_app`          () -> ()
//      - comando `salir`              () -> ()
//      - evento  `instalador://paso`      payload Paso { texto, hecho }
//      - evento  `instalador://progreso`  payload { pct: number } (0-100)
//
// 2. Simulada (navegador), con parámetros en la URL:
//      ?modo=nueva|detectada|actualizacion   (por defecto nueva)
//      ?fallo=1                              la instalación falla a mitad
//      ?pantalla=arranque|bienvenida|opciones|instalando|fin   salta a esa pantalla
//      ?tema=<id>                            tema inicial simulado

export type Modo = "nueva" | "detectada" | "actualizacion";
export type Borde = "left" | "top" | "right";

export interface OpcionesInstalacion {
  tema: string;
  edge: Borde;
  autostart: boolean;
}

export interface EstadoInstalador {
  modo: Modo;
  version_nueva: string;
  version_instalada: string | null;
  opciones: OpcionesInstalacion;
}

export interface Paso {
  texto: string;
  hecho: boolean;
}

export interface Backend {
  estado(): Promise<EstadoInstalador>;
  instalar(
    op: OpcionesInstalacion,
    alPaso: (p: Paso) => void,
    alProgreso: (pct: number) => void,
  ): Promise<void>;
  abrirApp(): Promise<void>;
  salir(): Promise<void>;
}

const OPCIONES_DEFECTO: OpcionesInstalacion = { tema: "netrunner", edge: "right", autostart: true };

const hayTauri = (): boolean => "__TAURI_INTERNALS__" in window;

function backendTauri(): Backend {
  return {
    async estado() {
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke<EstadoInstalador>("estado_instalador");
    },
    async instalar(opciones, alPaso, alProgreso) {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");
      const quitar = [
        await listen<Paso>("instalador://paso", (e) => alPaso(e.payload)),
        await listen<{ pct: number }>("instalador://progreso", (e) => alProgreso(e.payload.pct)),
      ];
      try {
        await invoke("instalar", { opciones });
      } catch (e) {
        throw new Error(typeof e === "string" ? e : (e as Error)?.message ?? "Error desconocido.");
      } finally {
        quitar.forEach((q) => q());
      }
    },
    async abrirApp() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("abrir_app");
    },
    async salir() {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("salir");
    },
  };
}

const esperar = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function backendSimulado(): Backend {
  const q = new URLSearchParams(location.search);
  const m = q.get("modo");
  const modo: Modo = m === "detectada" || m === "actualizacion" ? m : "nueva";
  const instalada = modo === "nueva" ? null : "1.4.0";
  const opciones: OpcionesInstalacion =
    modo === "nueva" ? { ...OPCIONES_DEFECTO } : { tema: "hielo", edge: "left", autostart: false };
  const temaUrl = q.get("tema");
  if (temaUrl) opciones.tema = temaUrl;
  const fallo = q.get("fallo") === "1";

  const pasos: string[] = [
    ...(modo === "nueva" ? [] : ["Cerrando Netrunner Panel"]),
    "Copiando archivos",
    "Registrando la aplicación",
    "Guardando ajustes",
    "Creando accesos",
  ];

  return {
    async estado() {
      return { modo, version_nueva: "1.5.0", version_instalada: instalada, opciones };
    },
    async instalar(_op, alPaso, alProgreso) {
      const total = 4000;
      const trozo = total / pasos.length;
      for (let i = 0; i < pasos.length; i++) {
        alPaso({ texto: pasos[i], hecho: false });
        const sub = 5;
        for (let s = 1; s <= sub; s++) {
          await esperar(trozo / sub);
          alProgreso(Math.round(((i + s / sub) / pasos.length) * 100));
          if (fallo && i === 2 && s === 3) {
            throw new Error("No se pudo escribir en la carpeta de instalación (simulado).");
          }
        }
        alPaso({ texto: pasos[i], hecho: true });
      }
    },
    async abrirApp() {
      console.info("[simulado] abrirApp");
    },
    async salir() {
      console.info("[simulado] salir");
    },
  };
}

export const backend: Backend = hayTauri() ? backendTauri() : backendSimulado();

/** Solo en el simulado: pantalla a la que saltar directamente (para capturas). */
export const pantallaInicial = (): string | null =>
  hayTauri() ? null : new URLSearchParams(location.search).get("pantalla");
