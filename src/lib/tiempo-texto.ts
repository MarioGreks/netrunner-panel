// Textos de fecha, hora y tiempo restante compartidos por la miniventana del borde y los widgets.
import type { EventoCalendario } from "./ipc";
import { dos } from "./texto";

export const DIAS = ["DOM", "LUN", "MAR", "MIÉ", "JUE", "VIE", "SÁB"];
export const MESES = ["ENE", "FEB", "MAR", "ABR", "MAY", "JUN", "JUL", "AGO", "SEP", "OCT", "NOV", "DIC"];
/** Minutos por debajo de los cuales un evento o una cuenta atrás se pinta como alerta. */
export const MIN_ALERTA = 15;

export const inicioDeDia = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** «MAR 24 NOV» */
export const fechaCorta = (d: Date) => `${DIAS[d.getDay()]} ${d.getDate()} ${MESES[d.getMonth()]}`;

/** «23D 04H 12M»; sin días «04H 12M»; sin horas «12M». */
export function restante(ms: number): string {
  const min = Math.max(0, Math.floor(ms / 60000));
  const d = Math.floor(min / 1440);
  const h = Math.floor((min % 1440) / 60);
  const m = min % 60;
  if (d > 0) return `${d}D ${dos(h)}H ${dos(m)}M`;
  if (h > 0) return `${dos(h)}H ${dos(m)}M`;
  return `${dos(m)}M`;
}

/** Para las filas compactas: «12D 03H»; con menos de un día «03H 20M»; con menos de una hora «20M». */
export function restanteCorto(ms: number): string {
  const min = Math.max(0, Math.floor(ms / 60000));
  const d = Math.floor(min / 1440);
  const h = Math.floor((min % 1440) / 60);
  const m = min % 60;
  if (d > 0) return `${d}D ${dos(h)}H`;
  if (h > 0) return `${dos(h)}H ${dos(m)}M`;
  return `${dos(m)}M`;
}

/** Cuándo es un evento: «HOY» (todo el día), «AHORA», «18:30», «MAÑ 09:00» o «3 DIC»; con alerta si falta poco. */
export function cuandoEvento(ev: EventoCalendario, ahora: Date): { texto: string; alerta: boolean } {
  if (ev.todo_el_dia) return { texto: "HOY", alerta: false };
  const ini = new Date(ev.inicio);
  if (ini.getTime() <= ahora.getTime()) return { texto: "AHORA", alerta: false };
  const hoy = inicioDeDia(ahora);
  const dia = inicioDeDia(ini);
  const hora = `${dos(ini.getHours())}:${dos(ini.getMinutes())}`;
  const texto = dia === hoy ? hora : dia === hoy + 86400000 ? `MAÑ ${hora}` : `${ini.getDate()} ${MESES[ini.getMonth()]}`;
  return { texto, alerta: ini.getTime() - ahora.getTime() < MIN_ALERTA * 60000 };
}
