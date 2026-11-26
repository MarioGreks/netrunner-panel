import { useState } from 'react';
import { Text } from 'react-native';
import { Boton } from '../src/components/Boton';
import { Dialogo } from '../src/components/Dialogo';
import { Marco } from '../src/components/Marco';
import { Pantalla } from '../src/components/Pantalla';
import { tema } from '../src/theme';

export default function Ajustes() {
  const [aviso, setAviso] = useState(false);
  return (
    <Pantalla titulo="AJUSTES">
      <Marco titulo="CUENTA MICROSOFT" variante="cian">
        <Text style={[tema.tenue, { marginBottom: tema.esp[4] }]}>NO CONECTADA</Text>
        <Boton texto="CONECTAR" onPress={() => setAviso(true)} />
      </Marco>
      <Dialogo
        visible={aviso}
        titulo="AVISO"
        onCerrar={() => setAviso(false)}
        acciones={<Boton texto="CERRAR" variante="contorno" onPress={() => setAviso(false)} />}
      >
        <Text style={tema.cuerpo}>Disponible en el siguiente encargo</Text>
      </Dialogo>
    </Pantalla>
  );
}
