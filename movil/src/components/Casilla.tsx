import { Pressable } from 'react-native';
import { tema } from '../theme';
import { Icono } from '../iconos';

interface Props {
  marcada: boolean;
  onPress: () => void;
}

/** Casilla de verificación: borde cian; marcada = relleno cian con check negro. */
export function Casilla({ marcada, onPress }: Props) {
  return (
    <Pressable
      onPress={onPress}
      hitSlop={10}
      style={{
        width: 24,
        height: 24,
        alignItems: 'center',
        justifyContent: 'center',
        borderWidth: 1,
        borderColor: tema.colores.secundario,
        backgroundColor: marcada ? tema.colores.secundario : 'transparent',
      }}
    >
      {marcada && <Icono nombre="check" size={16} color="#000000" />}
    </Pressable>
  );
}
