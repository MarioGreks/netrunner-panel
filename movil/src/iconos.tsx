import { MaterialCommunityIcons } from '@expo/vector-icons';
import type { ComponentProps } from 'react';
import { View } from 'react-native';
import { tema } from './theme';

export type NombreIcono = ComponentProps<typeof MaterialCommunityIcons>['name'];

interface Props {
  nombre: NombreIcono;
  size?: number;
  color?: string;
}

export function Icono({ nombre, size = 24, color = tema.colores.primario }: Props) {
  return (
    <View style={{ width: size, height: size, alignItems: 'center', justifyContent: 'center' }}>
      <MaterialCommunityIcons name={nombre} size={size} color={color} />
    </View>
  );
}
