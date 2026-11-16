// Indicador «LINK ●» de la cabecera: parpadea mientras calendario, meteo o notas sincronizan.
import {
  alEvento,
  estadoCalendario,
  estadoMeteo,
  estadoNotas,
  type EstadoCalendario,
  type EstadoMeteo,
  type EstadoNotas,
} from "../lib/ipc";

export function montarEnlace(el: HTMLElement): void {
  const activos = new Set<string>();
  const poner = (origen: string, ocupado: boolean) => {
    if (ocupado) activos.add(origen);
    else activos.delete(origen);
    el.classList.toggle("enlace--activo", activos.size > 0);
  };

  const cal = (s: EstadoCalendario) => poner("calendario", s.sincronizando);
  const met = (s: EstadoMeteo) => poner("meteo", s.cargando);
  const not = (s: EstadoNotas) => poner("notas", s.sincronizando);

  void alEvento<EstadoCalendario>("calendario://estado", cal);
  void alEvento<EstadoMeteo>("meteo://estado", met);
  void alEvento<EstadoNotas>("notas://estado", not);

  // Estado de partida por si ya hay una sincronización en marcha
  estadoCalendario().then(cal).catch(() => {});
  estadoMeteo().then(met).catch(() => {});
  estadoNotas().then(not).catch(() => {});
}
