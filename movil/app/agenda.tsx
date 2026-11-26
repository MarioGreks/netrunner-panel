import { useEffect, useMemo, useState } from 'react';
import { Pressable, RefreshControl, Text, View } from 'react-native';
import { useAgenda } from '../src/agenda/AgendaContext';
import { agruparPorDias, colorCategoria, diaCorto, etiquetaDia, horaDe, restante, type Evento } from '../src/agenda/modelo';
import { textoVacio, useConexion } from '../src/ConexionContext';
import { Marco } from '../src/components/Marco';
import { Pantalla } from '../src/components/Pantalla';
import { Icono } from '../src/iconos';
import { tema } from '../src/theme';

const c = tema.colores;
const dos = (n: number) => String(n).padStart(2, '0');
const fechaHora = (ms: number) => `${dos(new Date(ms).getDate())}/${dos(new Date(ms).getMonth() + 1)} ${horaDe(new Date(ms))}`;

function FilaEvento({ e, ahora }: { e: Evento; ahora: number }) {
  const [abierto, setAbierto] = useState(false);
  const terminado = !e.todo_el_dia && new Date(e.fin).getTime() <= ahora;
  const color = e.categorias.length > 0 ? colorCategoria(e.categorias[0]) : c.textoTenue;
  const cuando = e.todo_el_dia ? 'TODO EL DÍA' : `${horaDe(new Date(e.inicio))}–${horaDe(new Date(e.fin))}`;
  return (
    <Pressable
      onPress={() => setAbierto((a) => !a)}
      style={{ flexDirection: 'row', gap: tema.esp[3], opacity: terminado ? 0.5 : 1 }}
      accessibilityRole="button"
      accessibilityState={{ expanded: abierto }}
    >
      <View style={{ width: 3, backgroundColor: color }} />
      <View style={{ flex: 1, gap: 2 }}>
        <Text style={[tema.dato, { fontSize: tema.tam.s, color: terminado ? c.textoTenue : c.secundario }]}>{cuando}</Text>
        <Text style={[tema.cuerpo, terminado && { color: c.textoTenue }]}>{e.titulo}</Text>
        {e.ubicacion !== null && <Text style={[tema.cuerpo, { color: c.textoTenue, fontSize: tema.tam.s }]}>{e.ubicacion}</Text>}
        {abierto && <Text style={[tema.cuerpo, { color: c.textoTenue, marginTop: tema.esp[1] }]}>{e.descripcion ?? 'SIN DESCRIPCIÓN'}</Text>}
      </View>
    </Pressable>
  );
}

export default function Agenda() {
  const conexion = useConexion();
  const a = useAgenda();
  const [ahora, setAhora] = useState(Date.now());

  // Al entrar en la pantalla; y la hora se renueva cada minuto para las cuentas atrás.
  useEffect(() => {
    void a.actualizar();
    const t = setInterval(() => setAhora(Date.now()), 60000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    setAhora(Date.now());
  }, [a.agenda]);

  const cuentas = useMemo(
    () => a.agenda.cuentas.filter((x) => new Date(x.inicio).getTime() > ahora).sort((x, y) => new Date(x.inicio).getTime() - new Date(y.inicio).getTime()),
    [a.agenda.cuentas, ahora],
  );
  const dias = useMemo(() => agruparPorDias(a.agenda.eventos, new Date(ahora)), [a.agenda.eventos, ahora]);

  const estado =
    a.error !== null ? (
      <Text style={[tema.cuerpo, { color: c.alerta }]}>{a.error}</Text>
    ) : (
      <Text style={tema.tenue}>
        {a.aviso !== null && !a.cargando ? `${a.aviso} · ` : ''}
        {a.cargando ? 'ACTUALIZANDO' : a.actualizada !== null ? `ACTUALIZADO ${fechaHora(a.actualizada)}` : 'SIN ACTUALIZAR'}
      </Text>
    );

  return (
    <Pantalla
      titulo="AGENDA"
      acciones={
        <Pressable onPress={() => void a.actualizar()} hitSlop={12} disabled={a.cargando}>
          <Icono nombre="sync" size={26} color={a.cargando ? c.secundario : c.primario} />
        </Pressable>
      }
      refrescar={<RefreshControl refreshing={a.cargando} onRefresh={() => void a.actualizar()} tintColor={c.secundario} colors={[c.secundario]} />}
    >
      <View style={{ gap: tema.esp[4] }}>
        {estado}
        {cuentas.length > 0 && (
          <Marco titulo="Cuentas atrás" variante="cian">
            <View style={{ gap: tema.esp[3] }}>
              {cuentas.map((x) => {
                const ms = new Date(x.inicio).getTime();
                return (
                  <View key={x.id_evento} style={{ flexDirection: 'row', alignItems: 'center', gap: tema.esp[3] }}>
                    <View style={{ flex: 1 }}>
                      <Text style={tema.cuerpo}>{x.titulo}</Text>
                      <Text style={[tema.cuerpo, { color: c.textoTenue, fontSize: tema.tam.s }]}>
                        {diaCorto(new Date(ms))} {horaDe(new Date(ms))}
                      </Text>
                    </View>
                    <Text style={tema.dato}>{restante(ms - ahora)}</Text>
                  </View>
                );
              })}
            </View>
          </Marco>
        )}
        {dias.length === 0 ? (
          <Marco>
            <Text style={tema.tenue}>{conexion.estado === 'emparejado' ? 'SIN EVENTOS' : textoVacio(conexion)}</Text>
          </Marco>
        ) : (
          dias.map((d) => (
            <Marco key={d.clave} titulo={etiquetaDia(d.fecha, new Date(ahora))}>
              <View style={{ gap: tema.esp[4] }}>
                {d.eventos.map((e) => (
                  <FilaEvento key={`${e.id}|${e.inicio}`} e={e} ahora={ahora} />
                ))}
              </View>
            </Marco>
          ))
        )}
      </View>
    </Pantalla>
  );
}
