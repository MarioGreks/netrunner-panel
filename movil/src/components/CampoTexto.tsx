import { useState } from 'react';
import { TextInput, type TextInputProps } from 'react-native';
import { tema } from '../theme';

/** TextInput HUD: borde texto-tenue, amarillo con foco. */
export function CampoTexto({ style, onFocus, onBlur, ...resto }: TextInputProps) {
  const [foco, setFoco] = useState(false);
  return (
    <TextInput
      placeholderTextColor={tema.colores.textoTenue}
      selectionColor={tema.colores.primario}
      {...resto}
      onFocus={(e) => {
        setFoco(true);
        onFocus?.(e);
      }}
      onBlur={(e) => {
        setFoco(false);
        onBlur?.(e);
      }}
      style={[
        tema.cuerpo,
        {
          borderWidth: 1,
          borderColor: foco ? tema.colores.primario : tema.colores.textoTenue,
          paddingHorizontal: 12,
          paddingVertical: 8,
        },
        style,
      ]}
    />
  );
}
