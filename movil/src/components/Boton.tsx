import { Pressable, Text, type StyleProp, type ViewStyle } from 'react-native';
import { tema } from '../theme';
import { Icono, type NombreIcono } from '../iconos';

interface Props {
  texto: string;
  onPress: () => void;
  /** relleno: acción principal; contorno: secundaria; texto: sin caja (diálogos). */
  variante?: 'relleno' | 'contorno' | 'texto';
  /** Sustituye al color de acento de la variante. */
  color?: string;
  /** Nombre de MaterialCommunityIcons. */
  icono?: NombreIcono;
  style?: StyleProp<ViewStyle>;
}

export function Boton({ texto, onPress, variante = 'relleno', color, icono, style }: Props) {
  const acento = color ?? tema.colores.primario;
  const relleno = variante === 'relleno';
  const fg = relleno ? '#000000' : acento;
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        {
          flexDirection: 'row',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 8,
          paddingVertical: variante === 'texto' ? 6 : 12,
          paddingHorizontal: variante === 'texto' ? 8 : 16,
          backgroundColor: relleno ? acento : 'transparent',
          borderWidth: variante === 'contorno' ? 1 : 0,
          borderColor: acento,
          opacity: pressed ? 0.6 : 1,
        },
        style,
      ]}
    >
      {icono !== undefined && <Icono nombre={icono} size={18} color={fg} />}
      <Text style={[tema.titulo, { fontSize: tema.tam.m, letterSpacing: tema.tam.m * 0.18, color: fg }]}>
        {texto.toUpperCase()}
      </Text>
    </Pressable>
  );
}
