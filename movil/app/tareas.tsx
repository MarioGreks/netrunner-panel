import { Text } from 'react-native';
import { Marco } from '../src/components/Marco';
import { Pantalla } from '../src/components/Pantalla';
import { tema } from '../src/theme';

export default function Tareas() {
  return (
    <Pantalla titulo="TAREAS">
      <Marco>
        <Text style={tema.tenue}>SIN CONEXIÓN // CONECTA TU CUENTA EN AJUSTES</Text>
      </Marco>
    </Pantalla>
  );
}
