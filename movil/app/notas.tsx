import { useRouter } from 'expo-router';
import { Pressable, Text, View } from 'react-native';
import { textoVacio, useConexion } from '../src/ConexionContext';
import { Marco } from '../src/components/Marco';
import { Pantalla } from '../src/components/Pantalla';
import { Icono } from '../src/iconos';
import { AccionSync, LineaSync, TirarSync } from '../src/notas/IndicadoresSync';
import { useNotas } from '../src/notas/NotasContext';
import { tema } from '../src/theme';

const fecha = (ms: number) => {
  const d = new Date(ms);
  const dos = (x: number) => String(x).padStart(2, '0');
  return `${dos(d.getDate())}/${dos(d.getMonth() + 1)}/${d.getFullYear()} ${dos(d.getHours())}:${dos(d.getMinutes())}`;
};
const primeraLinea = (cuerpo: string) => cuerpo.split('\n').find((l) => l.trim() !== '')?.trim() ?? '';

export default function Notas() {
  const c = useConexion();
  const n = useNotas();
  const router = useRouter();
  const sinConexion = c.estado !== 'emparejado';

  return (
    <Pantalla titulo="NOTAS" acciones={<AccionSync />} refrescar={<TirarSync />}>
      <LineaSync />
      {sinConexion && n.notas.length === 0 ? (
        <Marco>
          <Text style={tema.tenue}>{textoVacio(c)}</Text>
        </Marco>
      ) : (
        <View style={{ gap: 12, marginTop: 12 }}>
          <Pressable
            onPress={() => router.push({ pathname: '/nota/[id]', params: { id: n.crearNota('', '') } })}
            style={{ alignSelf: 'flex-end' }}
            hitSlop={8}
          >
            <Icono nombre="plus" size={32} color={tema.colores.primario} />
          </Pressable>
          {n.notas.length === 0 ? (
            <Marco>
              <Text style={tema.tenue}>SIN NOTAS</Text>
            </Marco>
          ) : (
            n.notas.map((nota) => (
              <Pressable key={nota.id} onPress={() => router.push({ pathname: '/nota/[id]', params: { id: nota.id } })}>
                <Marco>
                  <Text style={[tema.titulo, { fontSize: tema.tam.s }]} numberOfLines={1}>
                    {nota.titulo.trim() || 'SIN TÍTULO'}
                  </Text>
                  <Text style={[tema.cuerpo, { color: tema.colores.textoTenue }]} numberOfLines={1}>
                    {primeraLinea(nota.cuerpo) || ' '}
                  </Text>
                  <Text style={[tema.cuerpo, { fontSize: tema.tam.xs, color: tema.colores.textoTenue }]}>{fecha(nota.modificada)}</Text>
                </Marco>
              </Pressable>
            ))
          )}
        </View>
      )}
    </Pantalla>
  );
}
