import { useRouter, type Href } from 'expo-router';
import { Pressable, Text } from 'react-native';
import { tema } from '../theme';

export function MenuTerminal({ opciones }: { opciones: { titulo: string; ruta: Href }[] }) {
  const router = useRouter();
  const c = tema.colores;
  return (
    <>
      {opciones.map((o, i) => (
        <Pressable
          key={o.titulo}
          accessibilityRole="button"
          accessibilityLabel={o.titulo}
          onPress={() => router.push(o.ruta)}
          style={({ pressed }) => ({
            height: 48,
            flexDirection: 'row',
            alignItems: 'center',
            gap: 12,
            paddingHorizontal: 4,
            borderBottomWidth: 1,
            borderColor: c.primarioTenue,
            backgroundColor: pressed ? c.primario : 'transparent',
          })}
        >
          {({ pressed }) => {
            const negro = pressed ? '#000000' : undefined;
            return (
              <>
                <Text style={[tema.cuerpo, { color: negro ?? c.primario }]}>{'>'}</Text>
                <Text style={[tema.cuerpo, { color: negro ?? c.textoTenue }]}>{String(i + 1).padStart(2, '0')}</Text>
                <Text style={[tema.cuerpo, { fontFamily: 'Orbitron_700Bold', color: negro ?? c.texto }]}>{o.titulo}</Text>
              </>
            );
          }}
        </Pressable>
      ))}
    </>
  );
}
