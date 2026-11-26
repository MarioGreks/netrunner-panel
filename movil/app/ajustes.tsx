import { useState } from 'react';
import { Text, View } from 'react-native';
import { Boton } from '../src/components/Boton';
import { CampoTexto } from '../src/components/CampoTexto';
import { Marco } from '../src/components/Marco';
import { Pantalla } from '../src/components/Pantalla';
import { useConexion } from '../src/ConexionContext';
import { redirectUri } from '../src/microsoft';
import { buscarCarpetas, carpetaPorRuta, listarFicheros, type Carpeta } from '../src/onedrive';
import { tema } from '../src/theme';

const REQUERIDOS = ['notas.json', 'cuentas.json', 'ajustes.json'];
type Fichero = Awaited<ReturnType<typeof listarFicheros>>[number];
const alerta = { color: tema.colores.alerta };
const mensaje = (e: unknown) => (e instanceof Error ? e.message : 'ERROR DESCONOCIDO');

export default function Ajustes() {
  const c = useConexion();
  const [resultados, setResultados] = useState<Carpeta[] | null>(null);
  const [ruta, setRuta] = useState('');
  const [errorCarpeta, setErrorCarpeta] = useState<string | null>(null);
  const [ficheros, setFicheros] = useState<Fichero[] | null>(null);
  const [errorPrueba, setErrorPrueba] = useState<string | null>(null);

  const elegir = async (f: Carpeta) => {
    await c.elegirCarpeta(f);
    setResultados(null);
    setFicheros(null);
  };
  const buscar = async () => {
    setErrorCarpeta(null);
    try {
      setResultados(await buscarCarpetas());
    } catch (e) {
      setErrorCarpeta(mensaje(e));
    }
  };
  const usarRuta = async () => {
    setErrorCarpeta(null);
    try {
      const f = await carpetaPorRuta(ruta);
      if (f === null) setErrorCarpeta('RUTA NO ENCONTRADA O NO ES UNA CARPETA');
      else await elegir(f);
    } catch (e) {
      setErrorCarpeta(mensaje(e));
    }
  };
  const cambiar = async () => {
    await c.olvidarCarpeta();
    setFicheros(null);
  };
  const probar = async () => {
    setErrorPrueba(null);
    try {
      setFicheros(await listarFicheros());
    } catch (e) {
      setFicheros(null);
      setErrorPrueba(mensaje(e));
    }
  };

  const conSesion = c.estado === 'conectada';
  return (
    <Pantalla titulo="AJUSTES">
      <View style={{ gap: tema.esp[3] }}>
        <Marco titulo="CUENTA MICROSOFT" variante="cian">
          {c.estado === 'sin-cliente' && (
            <Text style={[tema.cuerpo, alerta]}>
              FALTA EL CLIENT ID: copia movil/.env.example a movil/.env y pon EXPO_PUBLIC_MS_CLIENT_ID; reinicia expo
            </Text>
          )}
          {c.estado === 'conectando' && <Text style={tema.tenue}>CONECTANDO…</Text>}
          {c.estado === 'sin-sesion' && <Boton texto="CONECTAR" onPress={c.conectar} />}
          {conSesion && (
            <>
              <Text style={tema.dato}>{c.cuenta?.nombre}</Text>
              <Text style={[tema.cuerpo, { marginBottom: tema.esp[4] }]}>{c.cuenta?.correo}</Text>
              <Boton texto="DESCONECTAR" variante="contorno" onPress={() => void c.desconectar()} />
            </>
          )}
          {c.error !== null && <Text style={[tema.cuerpo, alerta, { marginTop: tema.esp[3] }]}>{c.error}</Text>}
          <Text style={[tema.tenue, { marginTop: tema.esp[4], letterSpacing: 0 }]}>URI DE REDIRECCIÓN:</Text>
          <Text selectable style={[tema.tenue, { letterSpacing: 0 }]}>{redirectUri()}</Text>
        </Marco>

        {conSesion && (
          <Marco titulo="CARPETA ONEDRIVE">
            {c.carpeta !== null ? (
              <>
                <Text selectable style={[tema.dato, { marginBottom: tema.esp[4] }]}>{c.carpeta.ruta}</Text>
                <Boton texto="CAMBIAR" variante="contorno" onPress={() => void cambiar()} />
              </>
            ) : (
              <View style={{ gap: tema.esp[3] }}>
                <Boton texto="BUSCAR" onPress={() => void buscar()} />
                {resultados?.map((f) => (
                  <Boton key={f.folderId} texto={f.ruta} variante="contorno" color={tema.colores.secundario} onPress={() => void elegir(f)} />
                ))}
                {resultados?.length === 0 && (
                  <>
                    <Text style={tema.tenue}>NO ENCONTRADA</Text>
                    <CampoTexto value={ruta} onChangeText={setRuta} placeholder="Sincronización Netrunner/netrunner-panel" autoCapitalize="none" autoCorrect={false} />
                    <Boton texto="USAR RUTA" onPress={() => void usarRuta()} />
                  </>
                )}
              </View>
            )}
            {errorCarpeta !== null && <Text style={[tema.cuerpo, alerta, { marginTop: tema.esp[3] }]}>{errorCarpeta}</Text>}
          </Marco>
        )}

        {conSesion && c.carpeta !== null && (
          <Marco titulo="PRUEBA" variante="cian">
            <Boton texto="PROBAR CONEXIÓN" onPress={() => void probar()} />
            {errorPrueba !== null && <Text style={[tema.cuerpo, alerta, { marginTop: tema.esp[3] }]}>{errorPrueba}</Text>}
            {ficheros !== null && (
              <View style={{ marginTop: tema.esp[3], gap: tema.esp[1] }}>
                {ficheros.map((f) => (
                  <Text key={f.name} style={tema.cuerpo}>
                    {REQUERIDOS.includes(f.name) ? '✓ ' : '  '}
                    {f.name} · {(f.size / 1024).toFixed(1)} KB · {new Date(f.lastModifiedDateTime).toLocaleString('es-ES')}
                  </Text>
                ))}
                {REQUERIDOS.filter((n) => !ficheros.some((f) => f.name === n)).map((n) => (
                  <Text key={n} style={[tema.cuerpo, alerta]}>{n} · FALTA</Text>
                ))}
              </View>
            )}
          </Marco>
        )}
      </View>
    </Pantalla>
  );
}
