import { JetBrainsMono_400Regular, JetBrainsMono_700Bold } from '@expo-google-fonts/jetbrains-mono';
import { Orbitron_400Regular, Orbitron_700Bold, useFonts } from '@expo-google-fonts/orbitron';
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { ConexionProvider } from '../src/ConexionContext';
import { NotasProvider } from '../src/notas/NotasContext';
import { tema } from '../src/theme';

export default function Layout() {
  const [cargadas] = useFonts({ Orbitron_400Regular, Orbitron_700Bold, JetBrainsMono_400Regular, JetBrainsMono_700Bold });
  if (!cargadas) return null;
  return (
    <>
      <StatusBar style="light" />
      <ConexionProvider>
        <NotasProvider>
          <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: tema.colores.fondo } }} />
        </NotasProvider>
      </ConexionProvider>
    </>
  );
}
