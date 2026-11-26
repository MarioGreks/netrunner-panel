import { Pressable, RefreshControl, Text } from 'react-native';
import { Icono } from '../iconos';
import { tema } from '../theme';
import { useNotas } from './NotasContext';

const dos = (n: number) => String(n).padStart(2, '0');
export const hora = (ms: number) => `${dos(new Date(ms).getHours())}:${dos(new Date(ms).getMinutes())}`;

/** Icono de sincronizar: cian mientras sincroniza. */
export function AccionSync() {
  const n = useNotas();
  return (
    <Pressable onPress={() => void n.sincronizar()} hitSlop={12} disabled={n.sincronizando}>
      <Icono nombre="sync" size={26} color={n.sincronizando ? tema.colores.secundario : tema.colores.primario} />
    </Pressable>
  );
}

/** «SYNC hh:mm» tenue, el aviso (panel no alcanzable) tenue, o el error en alerta. */
export function LineaSync() {
  const n = useNotas();
  if (n.error !== null) return <Text style={[tema.cuerpo, { color: tema.colores.alerta }]}>{n.error}</Text>;
  if (n.aviso !== null && !n.sincronizando) return <Text style={tema.tenue}>{n.aviso}{n.ultimaSync !== null ? ` · SYNC ${hora(n.ultimaSync)}` : ''}</Text>;
  return <Text style={tema.tenue}>{n.sincronizando ? 'SINCRONIZANDO' : n.ultimaSync !== null ? `SYNC ${hora(n.ultimaSync)}` : 'SIN SINCRONIZAR'}</Text>;
}

/** Tirar para sincronizar. */
export function TirarSync() {
  const n = useNotas();
  return <RefreshControl refreshing={n.sincronizando} onRefresh={() => void n.sincronizar()} tintColor={tema.colores.secundario} colors={[tema.colores.secundario]} />;
}
