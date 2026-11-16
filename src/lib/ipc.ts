// Envoltorios tipados de comandos y eventos de Tauri.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Borde = "left" | "right" | "top";

export interface Config {
  edge: Borde;
  autostart: boolean;
}

export const obtenerConfig = () => invoke<Config>("get_config");
export const guardarConfig = (config: Config) => invoke<void>("set_config", { config });
export const alternarPanel = () => invoke<void>("alternar_panel");
export const ocultarPanel = () => invoke<void>("ocultar_panel");

export type EventoPanel = "panel://mostrar" | "panel://ocultar" | "config://cambio";

export const alEvento = <T = unknown>(
  evento: EventoPanel,
  manejador: (carga: T) => void,
): Promise<UnlistenFn> => listen<T>(evento, (e) => manejador(e.payload));
