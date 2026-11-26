import type { TextStyle } from 'react-native';

// Tema HUD «netrunner»: valores de src/styles/tokens.css y temas.css del panel de escritorio.
const colores = {
  fondo: '#07080c',
  panel: 'rgba(9, 11, 16, 0.94)',
  primario: '#f5e50a',
  secundario: '#2de2e6',
  alerta: '#ff2a4d',
  texto: '#d9dbe3',
  textoTenue: '#7c8090',
  primarioTenue: 'rgba(245, 229, 10, 0.35)',
  secundarioTenue: 'rgba(45, 226, 230, 0.35)',
};

const esp = { 1: 4, 2: 8, 3: 12, 4: 16, 5: 24 };
const tam = { xs: 10, s: 12, m: 14, l: 18, xl: 28 };

const base: TextStyle = { includeFontPadding: false, textAlignVertical: 'center' };
const orbitron: TextStyle = { ...base, fontFamily: 'Orbitron_700Bold' };
const mono: TextStyle = { ...base, fontFamily: 'JetBrainsMono_400Regular' };

export const tema = {
  colores,
  corte: 12,
  esp,
  tam,
  titulo: { ...orbitron, fontSize: tam.s, letterSpacing: tam.s * 0.18, color: colores.primario } satisfies TextStyle,
  encabezado: { ...orbitron, fontSize: tam.xl, letterSpacing: tam.xl * 0.18, color: colores.primario } satisfies TextStyle,
  cuerpo: { ...mono, fontSize: tam.m, color: colores.texto } satisfies TextStyle,
  dato: { ...mono, fontSize: tam.m, color: colores.secundario } satisfies TextStyle,
  tenue: { ...mono, fontSize: tam.m, letterSpacing: tam.m * 0.2, color: colores.textoTenue } satisfies TextStyle,
};
