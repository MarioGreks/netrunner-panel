import { useCanGoBack, useRouter } from 'expo-router';
import type { ReactElement, ReactNode } from 'react';
import { Pressable, ScrollView, type RefreshControlProps, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { tema } from '../theme';

interface Props {
  titulo: string;
  /** Muestra el botón de volver (false en el inicio). */
  conVolver?: boolean;
  /** Iconos/botones a la derecha del título. */
  acciones?: ReactNode;
  /** Tirar para refrescar. */
  refrescar?: ReactElement<RefreshControlProps>;
  children?: ReactNode;
}

/** Fondo HUD + zona segura + título. Lo reutilizan todas las pantallas. */
export function Pantalla({ titulo, conVolver = true, acciones, refrescar, children }: Props) {
  const router = useRouter();
  const puedeVolver = useCanGoBack();
  return (
    <SafeAreaView style={[estilos.raiz, { backgroundColor: tema.colores.fondo }]}>
      <ScrollView contentContainerStyle={estilos.contenido} keyboardShouldPersistTaps="handled" refreshControl={refrescar}>
        {conVolver && puedeVolver && (
          <Pressable onPress={() => router.back()} hitSlop={12} style={{ alignSelf: 'flex-start', height: 24, justifyContent: 'center' }}>
            <Text style={[tema.cuerpo, { color: tema.colores.secundario }]}>{'< VOLVER'}</Text>
          </Pressable>
        )}
        <View style={estilos.cabecera}>
          <Text style={[tema.encabezado, estilos.titulo]}>{titulo}</Text>
          {acciones}
        </View>
        <View>{children}</View>
      </ScrollView>
    </SafeAreaView>
  );
}

const estilos = StyleSheet.create({
  raiz: { flex: 1 },
  contenido: { padding: 20, gap: 12 },
  cabecera: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginBottom: 8 },
  titulo: { flex: 1 },
});
