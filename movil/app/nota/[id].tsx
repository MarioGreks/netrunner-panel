import { useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { Alert, View } from 'react-native';
import { Boton } from '../../src/components/Boton';
import { CampoTexto } from '../../src/components/CampoTexto';
import { Pantalla } from '../../src/components/Pantalla';
import { useNotas } from '../../src/notas/NotasContext';
import { tema } from '../../src/theme';

export default function EditorNota() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();
  const n = useNotas();
  const nota = n.notas.find((x) => x.id === id);
  const [titulo, setTitulo] = useState(nota?.titulo ?? '');
  const [cuerpo, setCuerpo] = useState(nota?.cuerpo ?? '');
  // último contenido guardado y contenido actual (refs para guardar al salir sin cierres obsoletos)
  const guardado = useRef({ titulo: nota?.titulo ?? '', cuerpo: nota?.cuerpo ?? '' });
  const actual = useRef(guardado.current);
  const borrada = useRef(false);
  const editar = useRef(n.editarNota);
  editar.current = n.editarNota;

  const volcar = () => {
    const a = actual.current;
    if (borrada.current || id === undefined) return;
    if (a.titulo !== guardado.current.titulo || a.cuerpo !== guardado.current.cuerpo) {
      guardado.current = a;
      editar.current(id, a.titulo, a.cuerpo);
    }
  };

  useEffect(() => {
    actual.current = { titulo, cuerpo };
    const t = setTimeout(volcar, 1000);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [titulo, cuerpo]);

  // al salir de la pantalla
  useEffect(() => volcar, []); // eslint-disable-line react-hooks/exhaustive-deps

  const borrar = () =>
    Alert.alert('BORRAR NOTA', '¿Seguro?', [
      { text: 'Cancelar', style: 'cancel' },
      {
        text: 'Borrar',
        style: 'destructive',
        onPress: () => {
          if (id !== undefined) n.borrarNota(id);
          borrada.current = true;
          router.back();
        },
      },
    ]);

  return (
    <Pantalla titulo="NOTA">
      <View style={{ gap: 12 }}>
        <CampoTexto value={titulo} onChangeText={setTitulo} placeholder="TÍTULO" style={{ fontFamily: 'Orbitron_700Bold', color: tema.colores.primario }} />
        <CampoTexto
          value={cuerpo}
          onChangeText={setCuerpo}
          placeholder="ESCRIBE AQUÍ"
          multiline
          textAlignVertical="top"
          style={{ minHeight: 260 }}
        />
        <Boton texto="Borrar" variante="contorno" color={tema.colores.alerta} onPress={borrar} />
      </View>
    </Pantalla>
  );
}
