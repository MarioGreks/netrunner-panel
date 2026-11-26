import { Text } from 'react-native';
import { MenuTerminal } from '../src/components/MenuTerminal';
import { Pantalla } from '../src/components/Pantalla';
import { tema } from '../src/theme';

export default function Inicio() {
  return (
    <Pantalla titulo="NETRUNNER" conVolver={false}>
      <Text style={[tema.tenue, { marginBottom: tema.esp[4] }]}>PANEL // MÓVIL</Text>
      <MenuTerminal
        opciones={[
          { titulo: 'TAREAS', ruta: '/tareas' },
          { titulo: 'NOTAS', ruta: '/notas' },
          { titulo: 'AGENDA', ruta: '/agenda' },
          { titulo: 'AJUSTES', ruta: '/ajustes' },
        ]}
      />
    </Pantalla>
  );
}
