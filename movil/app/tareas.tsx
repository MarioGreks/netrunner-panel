import { Text } from 'react-native';
import { textoVacio, useConexion } from '../src/ConexionContext';
import { Marco } from '../src/components/Marco';
import { Pantalla } from '../src/components/Pantalla';
import { tema } from '../src/theme';

export default function Tareas() {
  const c = useConexion();
  return (
    <Pantalla titulo="TAREAS">
      <Marco>
        <Text style={tema.tenue}>{textoVacio(c)}</Text>
      </Marco>
    </Pantalla>
  );
}
