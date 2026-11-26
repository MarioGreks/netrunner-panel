import { useState, type ReactNode } from 'react';
import { Text, View } from 'react-native';
import Svg, { Polygon } from 'react-native-svg';
import { tema } from '../theme';

interface Props {
  titulo?: string;
  derecha?: ReactNode;
  variante?: 'amarillo' | 'cian';
  children?: ReactNode;
}

/** Marco HUD: esquinas cortadas arriba-izquierda y abajo-derecha, borde de 1 px. */
export function Marco({ titulo, derecha, variante = 'amarillo', children }: Props) {
  const [tamano, setTamano] = useState({ w: 0, h: 0 });
  const c = tema.colores;
  const color = variante === 'cian' ? c.secundario : c.primario;
  const { w, h } = tamano;
  const k = tema.corte;
  // 0.5 de margen para que el trazo de 1 px quede entero dentro del recuadro
  const puntos = `${k},0.5 ${w - 0.5},0.5 ${w - 0.5},${h - k} ${w - k},${h - 0.5} 0.5,${h - 0.5} 0.5,${k}`;
  return (
    <View
      onLayout={(e) => setTamano({ w: e.nativeEvent.layout.width, h: e.nativeEvent.layout.height })}
      style={{ padding: tema.esp[4] }}
    >
      {w > 0 && (
        <Svg width={w} height={h} style={{ position: 'absolute', top: 0, left: 0 }}>
          <Polygon points={puntos} fill={c.fondo} stroke={color} strokeWidth={1} />
        </Svg>
      )}
      {(titulo !== undefined || derecha !== undefined) && (
        <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginBottom: tema.esp[3] }}>
          <Text style={[tema.titulo, { color }]}>{titulo?.toUpperCase()}</Text>
          {derecha}
        </View>
      )}
      {children}
    </View>
  );
}
