import { useState } from 'react';
import { Alert, Pressable, Text, View } from 'react-native';
import { textoVacio, useConexion } from '../src/ConexionContext';
import { Boton } from '../src/components/Boton';
import { CampoTexto } from '../src/components/CampoTexto';
import { Casilla } from '../src/components/Casilla';
import { Dialogo } from '../src/components/Dialogo';
import { Marco } from '../src/components/Marco';
import { Pantalla } from '../src/components/Pantalla';
import { Icono } from '../src/iconos';
import { AccionSync, LineaSync, TirarSync } from '../src/notas/IndicadoresSync';
import { useNotas } from '../src/notas/NotasContext';
import type { Tarea } from '../src/notas/fusion';
import { tema } from '../src/theme';

export default function Tareas() {
  const c = useConexion();
  const n = useNotas();
  const [nueva, setNueva] = useState('');
  const [plegada, setPlegada] = useState(true);
  const [editando, setEditando] = useState<Tarea | null>(null);
  const [texto, setTexto] = useState('');

  const hayDatos = n.tareasPendientes.length + n.tareasHechas.length > 0;
  const sinConexion = c.estado !== 'conectada' || c.carpeta === null;

  const crear = () => {
    n.crearTarea(nueva);
    setNueva('');
  };
  const abrir = (t: Tarea) => {
    setTexto(t.texto);
    setEditando(t);
  };
  const guardar = () => {
    if (editando !== null && texto.trim() !== '' && texto !== editando.texto) n.editarTarea(editando.id, texto.trim());
    setEditando(null);
  };
  const borrar = () => {
    if (editando === null) return;
    const id = editando.id;
    Alert.alert('BORRAR TAREA', '¿Seguro?', [
      { text: 'Cancelar', style: 'cancel' },
      {
        text: 'Borrar',
        style: 'destructive',
        onPress: () => {
          n.borrarTarea(id);
          setEditando(null);
        },
      },
    ]);
  };

  const fila = (t: Tarea) => (
    <View key={t.id} style={{ flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 8 }}>
      <Casilla marcada={t.hecha} onPress={() => n.alternarTarea(t.id)} />
      <Pressable onPress={() => abrir(t)} style={{ flex: 1 }}>
        <Text style={[tema.cuerpo, t.hecha && { color: tema.colores.textoTenue, textDecorationLine: 'line-through' }]}>{t.texto}</Text>
      </Pressable>
    </View>
  );

  return (
    <Pantalla titulo="TAREAS" acciones={<AccionSync />} refrescar={<TirarSync />}>
      <LineaSync />
      {sinConexion && !hayDatos ? (
        <Marco>
          <Text style={tema.tenue}>{textoVacio(c)}</Text>
        </Marco>
      ) : (
        <View style={{ gap: 12, marginTop: 12 }}>
          <View style={{ flexDirection: 'row', gap: 8, alignItems: 'center' }}>
            <CampoTexto value={nueva} onChangeText={setNueva} onSubmitEditing={crear} placeholder="NUEVA TAREA" returnKeyType="done" style={{ flex: 1 }} />
            <Pressable onPress={crear} hitSlop={8}>
              <Icono nombre="plus" size={32} color={tema.colores.primario} />
            </Pressable>
          </View>
          <Marco>
            {n.tareasPendientes.length === 0 ? <Text style={tema.tenue}>SIN TAREAS PENDIENTES</Text> : n.tareasPendientes.map(fila)}
          </Marco>
          {n.tareasHechas.length > 0 && (
            <View>
              <Pressable onPress={() => setPlegada(!plegada)} style={{ paddingVertical: 8 }}>
                <Text style={tema.titulo}>
                  {plegada ? '▸' : '▾'} COMPLETADAS ({n.tareasHechas.length})
                </Text>
              </Pressable>
              {!plegada && n.tareasHechas.map(fila)}
            </View>
          )}
        </View>
      )}
      <Dialogo
        visible={editando !== null}
        titulo="EDITAR TAREA"
        onCerrar={() => setEditando(null)}
        acciones={
          <>
            <Boton texto="Borrar" variante="contorno" color={tema.colores.alerta} onPress={borrar} />
            <Boton texto="Guardar" onPress={guardar} />
          </>
        }
      >
        <CampoTexto value={texto} onChangeText={setTexto} multiline autoFocus />
      </Dialogo>
    </Pantalla>
  );
}
