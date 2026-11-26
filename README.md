# Netrunner Panel

Panel de escritorio para Windows con estética cyberpunk HUD (amarillo y cian sobre fondo oscuro). Se despliega desde un borde de la pantalla y reúne en un solo sitio tu agenda de Outlook, el correo importante, el tiempo, las notas, los accesos directos y el estado del equipo.

Está hecho con Tauri 2, Rust y TypeScript sin framework.

## Funciones

**Panel y miniventana**
- Panel de 420 px que se abre llevando el ratón al tramo central de un borde de la pantalla (izquierdo, derecho o superior, a elegir en Ajustes) o desde la miniventana.
- Se cierra al sacar el ratón del panel o al hacer clic fuera. Animación holográfica al abrir y de apagado tipo CRT al cerrar.
- Miniventana pegada abajo del borde (o arriba, si el borde es el superior) con la hora, el tiempo, el próximo evento y hasta 4 cuentas atrás.
- Icono en la bandeja del sistema con menú: abrir el panel, ajustes, pausar la apertura por borde y salir. Arranque con Windows, que se puede desactivar.

**METEO**
- Tira con el tiempo actual (Open-Meteo), desplegable con gráfica de 24 horas y previsión de 5 días.
- Agenda de Outlook leída desde una URL ICS, a 6 meses vista, con las categorías de color de Outlook.
- Estrella (★) en un evento para convertirlo en cuenta atrás.
- Botón «+ EVENTO» para crear citas a través del Outlook clásico.

**CORREO**
- Correos importantes de los últimos 14 días, filtrados por remitentes VIP y palabras clave y leídos del Outlook clásico.

**ACCESOS**
- Control de la música con los controles multimedia de Windows (funciona con cualquier reproductor compatible).
- Accesos directos a programas, carpetas, archivos y direcciones web, con su icono.

**NOTAS**
- Tareas y notas rápidas, guardadas en local, con sincronización opcional mediante una carpeta de OneDrive.

**SISTEMA**
- CPU, RAM, GPU, discos y batería.

**CONSUMO** (integración de [Consumo](https://github.com/Kuerb0), de Kuerb0)
- Consumo de Claude: anillos del límite semanal y de la ventana de 5 h con el ritmo esperado, la estimación del día y el aviso si te vas a quedar sin límite.
- Contexto de cada sesión viva de Claude Code; un clic abre su chat en la app de Claude.
- Ventana flotante con los apartados Resumen, Ritmo, Días, Proyectos, Equipos, Uso e Histórico, informes HTML (semana, mes, año, histórico total) y guía (F1).
- Reparto estimado del % por proyecto, sesión, modelo y tarea, suma de varios equipos por una carpeta de OneDrive y archivo semanal que conserva el histórico.
- Instala y vigila el plugin `uso-consumo` de Claude Code, que es el que escribe los límites. Avisos al 80 % y al 100 %.

**AJUSTES**
- 6 temas de color y control de la transparencia del panel.
- Opcional y avanzado: conexión directa de cuentas Microsoft (Microsoft Graph) mediante OAuth 2.0 con PKCE.

## Requisitos

- Windows 10 u 11.
- WebView2, que ya viene con Windows 11.
- Outlook clásico (opcional): hace falta para la pestaña CORREO y para «+ EVENTO». La agenda de METEO no lo necesita, solo la URL ICS.

## Instalación

1. Descarga `Netrunner-Panel-Setup.exe` desde la última release del repositorio.
2. Ejecútalo. Es un instalador propio con estética HUD: eliges tema, borde de anclaje e inicio con Windows, y se instala solo para tu usuario, sin permisos de administrador.
3. El instalador no está firmado, así que Windows SmartScreen puede avisar. Pulsa **Más información** y después **Ejecutar de todas formas**.
4. Desde la 1.4.0 la app se actualiza sola: comprueba si hay versión nueva al arrancar, cada hora y al abrir el panel si la última comprobación tiene más de 10 minutos, y también puedes pulsar **BUSCAR ACTUALIZACIONES** en AJUSTES → SISTEMA.

## Configuración básica

Abre el panel y pulsa el engranaje para entrar en AJUSTES.

- **Calendario.** Publica tu calendario de Outlook como ICS: en Outlook en la web, ve a Configuración › Calendario › Calendarios compartidos › Publicar un calendario, elige el calendario y pulsa Publicar. Copia el enlace ICS y pégalo en CALENDARIO, dentro de AJUSTES. Esa URL es privada: no la compartas.
- **Ciudad.** En METEO de AJUSTES, busca tu ciudad y selecciónala.
- **OneDrive.** En NOTAS de AJUSTES, elige una carpeta de OneDrive para sincronizar las notas entre equipos. Sin ella, las notas se quedan solo en este equipo.
- **Palabras clave y VIP.** En CORREO de AJUSTES, añade remitentes VIP (una dirección o un dominio) y palabras clave del asunto o del remitente. Sin filtros, la pestaña CORREO no muestra nada.

**WIDGETS**
- Cinco widgets de escritorio: RELOJ, CUENTAS ATRÁS, SISTEMA, CONSUMO y TAREAS. Puede haber uno de cada tipo.
- Viven pegados al escritorio, por debajo de las demás ventanas. Se ven con Win+D.
- Para añadirlos: bandeja → Editar widgets → catálogo. En el modo edición se mueven y se redimensionan, con encaje a una rejilla de 20 px.
- Desde la barra de edición se ajusta la opacidad. Se sale con LISTO o con Esc.
- Un widget tapado por otra ventana no se actualiza hasta que vuelve a verse. El modo Ocultar también los esconde.

## Conexión móvil

El panel puede abrir un pequeño servidor en tu red local para que la app móvil sincronice notas y tareas y lea la agenda. Viene apagado.

1. En AJUSTES, sección CONEXIÓN MÓVIL, marca ACTIVAR. La primera vez Windows preguntará si permites el acceso a la red: pulsa Permitir.
2. Pulsa EMPAREJAR MÓVIL: aparece un código de 6 cifras que cambia cada 15 segundos (el modo emparejar dura 5 minutos y admite 5 fallos). Escríbelo en la app móvil, junto con la IP y el puerto (47821) que muestra el panel.
3. Solo hay un móvil emparejado a la vez; DESVINCULAR lo olvida.

Úsalo solo en redes de confianza (casa): los datos viajan sin cifrar por el WiFi.

## Privacidad

Los datos personales no salen de tu equipo salvo para hablar con los servicios que se indican abajo.

**Qué se guarda y dónde**
- Los widgets colocados, su posición, tamaño y opacidad se guardan en `widgets.json`, en esa misma carpeta.
- La configuración (incluida la URL ICS), las notas, los accesos, las cuentas atrás y las últimas copias del tiempo y de la agenda se guardan solo en tu carpeta de datos de la aplicación, en AppData. Desde AJUSTES puedes abrir esa carpeta.
- Si sincronizas las notas, se copian además en la carpeta de OneDrive que elijas.
- Si usas la conexión con Microsoft Graph, los tokens se guardan en el Administrador de credenciales de Windows, nunca en archivos de configuración.
- CONSUMO lee en local tus límites (`~/.claude/statusbar`) y las transcripciones de Claude Code (`~/.claude/projects`); de ellas solo saca tokens, modelo, proyecto y tipo de tarea, nunca el texto. Si eliges una carpeta de sincronización, publica ahí ese resumen por equipo y el archivo semanal.

**Servicios externos**
- Open-Meteo y su geocodificación: el tiempo y la búsqueda de ciudad. No requiere clave. Se envían las coordenadas o el texto de la ciudad que buscas.
- La URL ICS que tú publiques en Outlook: se descarga para leer la agenda.
- Microsoft Graph, solo si conectas una cuenta de forma opcional.

La aplicación no incluye analítica ni telemetría.

## Compilar desde el código

Necesitas:
- Node.js y npm.
- Rust (con el destino de Windows MSVC).
- Las Build Tools de Visual Studio con el componente «Desarrollo de escritorio con C++» (MSVC).

```
npm install
npm run tauri dev      # abre la app en modo desarrollo
npm run tauri build    # genera el instalador NSIS
```

El instalador queda en `src-tauri/target/release/bundle/nsis/`.

## Publicar una versión

1. Sube el número de versión en los cinco ficheros: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `instalador/src-tauri/Cargo.toml` e `instalador/src-tauri/tauri.conf.json` (y regenera las imágenes del instalador con `python scripts/generar-imagenes-instalador.py`).
2. Construye el instalador HUD completo y firmado. El script compila la app (setup NSIS), el frontend del instalador y el exe del instalador con el NSIS incrustado, y lo firma con la clave del actualizador (`~/.tauri/netrunner-panel.key`, fuera del repositorio; o la que indique `TAURI_SIGNING_PRIVATE_KEY`):

```
npm run instalador:construir
```

   Con poca memoria usa `npm run instalador:construir -- --ligero` (compila con `-j 1` y sin LTO, más lento). Con `--sin-app` reutiliza el setup NSIS ya compilado de la app.

3. Prepara los ficheros de la release (crea `release/` con el instalador y `latest.json`):

```
npm run release:preparar -- --notas "Qué hay de nuevo"
```

4. Publica la release con ambos ficheros:

```
gh release create v<versión> release/Netrunner-Panel-Setup.exe release/latest.json
```

## Estructura

```
index.html, tab.html   ventanas del panel y de la miniventana
consumo.html           ventana flotante de CONSUMO (y aviso-consumo.html, sus avisos)
widget.html            ventana de un widget de escritorio (y widgets-barra.html, la barra de edición)
src/                   interfaz en TypeScript y CSS
  lib/                 IPC con Rust, temas, categorías, navegación
  modulos/             una vista por pestaña (tiempo, correo, accesos, notas, sistema, consumo, ajustes)
  widgets/             el contenido de cada widget de escritorio (reloj, cuentas, sistema, consumo, tareas)
  styles/              estilos y temas de color
  ui/                  componentes comunes (pestañas, formularios, gráfica, resumen)
src-tauri/             núcleo en Rust
  src/                 un módulo por función (calendario, outlook, meteo, notas, accesos, musica, sistema, consumo, microsoft, ventanas, bandeja, config…)
  recursos/consumo-mod plugin uso-consumo de Claude Code, incrustado en el ejecutable
  tauri.conf.json      configuración de Tauri y del instalador
  icons/               iconos de la aplicación
public/                recursos estáticos
DISEÑO.md              decisiones de diseño
```

## Licencia

MIT. Consulta `LICENSE`. Los avisos de las dependencias y de las fuentes (Orbitron y JetBrains Mono, con licencia SIL OFL 1.1) están en `THIRD-PARTY-NOTICES.md`.
