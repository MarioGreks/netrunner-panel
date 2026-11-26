import { useState } from 'react';
import { Alert, Text, View } from 'react-native';
import { Boton } from '../src/components/Boton';
import { CampoTexto } from '../src/components/CampoTexto';
import { Marco } from '../src/components/Marco';
import { Pantalla } from '../src/components/Pantalla';
import { PUERTO_DEFECTO, useConexion } from '../src/ConexionContext';
import { tema } from '../src/theme';

const alerta = { color: tema.colores.alerta };
const IPV4 = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/;

/** Devuelve el puerto o un texto de error. */
function validar(host: string, puerto: string, codigo?: string): { host: string; puerto: number } | string {
  const h = host.trim();
  if (!IPV4.test(h)) return 'IP NO VÁLIDA (EJ. 192.168.1.20)';
  const p = Number(puerto.trim());
  if (!/^\d+$/.test(puerto.trim()) || p < 1 || p > 65535) return 'PUERTO NO VÁLIDO (1-65535)';
  if (codigo !== undefined && !/^\d{6}$/.test(codigo.trim())) return 'EL CÓDIGO TIENE 6 CIFRAS';
  return { host: h, puerto: p };
}

export default function Ajustes() {
  const c = useConexion();
  const [host, setHost] = useState('');
  const [puerto, setPuerto] = useState(String(PUERTO_DEFECTO));
  const [codigo, setCodigo] = useState('');
  const [editando, setEditando] = useState(false);
  const [errorLocal, setErrorLocal] = useState<string | null>(null);
  const [ocupado, setOcupado] = useState(false);

  const mensaje = (e: unknown) => (e instanceof Error ? e.message : 'ERROR DESCONOCIDO');

  const emparejar = async () => {
    const v = validar(host, puerto, codigo);
    if (typeof v === 'string') return setErrorLocal(v);
    setErrorLocal(null);
    setOcupado(true);
    try {
      await c.emparejar(v.host, v.puerto, codigo.trim());
      setCodigo('');
    } catch (e) {
      setErrorLocal(mensaje(e));
    } finally {
      setOcupado(false);
    }
  };
  const abrirCambio = () => {
    setHost(c.panel?.host ?? '');
    setPuerto(String(c.panel?.puerto ?? PUERTO_DEFECTO));
    setErrorLocal(null);
    setEditando(true);
  };
  const guardarCambio = async () => {
    const v = validar(host, puerto);
    if (typeof v === 'string') return setErrorLocal(v);
    setErrorLocal(null);
    try {
      await c.cambiarDireccion(v.host, v.puerto);
      setEditando(false);
    } catch (e) {
      setErrorLocal(mensaje(e));
    }
  };
  const olvidar = () =>
    Alert.alert('OLVIDAR PANEL', 'Habrá que volver a emparejar. Las notas y tareas de la app se conservan.', [
      { text: 'Cancelar', style: 'cancel' },
      { text: 'Olvidar', style: 'destructive', onPress: () => void c.olvidar() },
    ]);

  const campoDireccion = (
    <>
      <CampoTexto value={host} onChangeText={setHost} placeholder="IP DEL PC (192.168.1.20)" keyboardType="numbers-and-punctuation" autoCapitalize="none" autoCorrect={false} />
      <CampoTexto value={puerto} onChangeText={setPuerto} placeholder="PUERTO" keyboardType="number-pad" />
    </>
  );
  const error = errorLocal ?? c.error;

  return (
    <Pantalla titulo="AJUSTES">
      <View style={{ gap: tema.esp[3] }}>
        <Marco titulo="PANEL DEL PC" variante="cian">
          {c.estado === 'sin-emparejar' ? (
            <View style={{ gap: tema.esp[3] }}>
              <Text style={tema.tenue}>EN EL PC: AJUSTES → CONEXIÓN MÓVIL → ACTIVAR → EMPAREJAR MÓVIL</Text>
              {campoDireccion}
              <CampoTexto value={codigo} onChangeText={setCodigo} placeholder="CÓDIGO (6 CIFRAS)" keyboardType="number-pad" maxLength={6} />
              <Boton texto={ocupado ? 'EMPAREJANDO…' : 'EMPAREJAR'} onPress={() => void (ocupado ? undefined : emparejar())} />
            </View>
          ) : editando ? (
            <View style={{ gap: tema.esp[3] }}>
              {campoDireccion}
              <Boton texto="GUARDAR" onPress={() => void guardarCambio()} />
              <Boton texto="CANCELAR" variante="contorno" onPress={() => setEditando(false)} />
            </View>
          ) : (
            <View style={{ gap: tema.esp[3] }}>
              <Text style={tema.dato}>
                {c.panel?.host}:{c.panel?.puerto}
              </Text>
              <Text style={c.alcanzable === true ? { ...tema.cuerpo, color: tema.colores.secundario } : tema.tenue}>
                {c.alcanzable === null ? 'COMPROBANDO…' : c.alcanzable ? 'ALCANZABLE' : 'NO ALCANZABLE'}
              </Text>
              {c.version !== undefined && <Text style={tema.tenue}>PANEL v{c.version}</Text>}
              <Boton texto="PROBAR" onPress={() => void c.comprobar()} />
              <Boton texto="CAMBIAR IP" variante="contorno" onPress={abrirCambio} />
              <Boton texto="OLVIDAR" variante="contorno" color={tema.colores.alerta} onPress={olvidar} />
            </View>
          )}
          {error !== null && <Text style={[tema.cuerpo, alerta, { marginTop: tema.esp[3] }]}>{error}</Text>}
        </Marco>
        <Text style={tema.tenue}>SOLO SINCRONIZA EN LA MISMA WIFI QUE EL PC CON EL PANEL ABIERTO. SIN CONEXIÓN, LA APP FUNCIONA CON SU COPIA.</Text>
      </View>
    </Pantalla>
  );
}
