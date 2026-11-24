# netrunner-panel: decisiones de diseño

## Plataforma y stack
- Widget de escritorio para **Windows**.
- **Tauri 2 + TypeScript** (interfaz HTML/CSS, núcleo en Rust).

## Comportamiento del panel
- Entra deslizándose desde un **borde configurable** (izquierdo, derecho o superior) que se elige en los ajustes.
- Se abre de dos formas: **llevando el ratón al borde** o con una **pestaña neón fina siempre visible** (clic o arrastrar).
- Abierto, va **superpuesto**: flota encima de todo y se esconde al hacer clic fuera, sin mover las demás ventanas.
- **Arranca con Windows**, escondido, con una casilla en los ajustes para desactivarlo.

## Distribución
- **Pestañas**: barra de iconos HUD y un módulo por vista.

## Módulos (v1)
1. **Sistema**: CPU, RAM, GPU (uso y temperatura si está disponible), disco por unidad y batería. Sin red.
2. **Reloj / calendario / tiempo**
   - Reloj grande y fecha.
   - Calendario de Outlook mediante una **URL ICS publicada** (solo lectura). Se refresca automáticamente cada 5–15 min, **se sincroniza al abrir el panel** y tiene un **botón de sincronización manual**.
   - La URL ICS es privada: se guarda en la configuración local y **nunca va en el repo**.
   - Tiempo con **Open-Meteo** (sin clave de API) y ciudad configurable.
3. **Notas / tareas**: se guardan en **local** (JSON en AppData), con la **opción de sincronizar** con una carpeta de OneDrive elegida por el usuario.
4. **Accesos y música**
   - Accesos directos añadidos **desde el panel** (botón +: .exe, carpeta o URL), con el icono sacado del ejecutable.
   - Música con los **controles multimedia de Windows (SMTC)**: funciona con cualquier reproductor y muestra la carátula, el título y play/pausa/siguiente.

## Estética
- Cyberpunk HUD: fondo casi negro, marcos angulares con esquinas cortadas, trazos de circuito y bandas de advertencia.
- Paleta: **amarillo neón** (marcos y datos principales), **cian** (gráficas y wireframes) y **rojo** solo para alertas.
- Animación **viva pero contenida**: entrada con glitch o escaneo, gráficas en movimiento y algún wireframe 3D ligero.
- Tipografía: **Orbitron** para los títulos y **JetBrains Mono** para los datos, incluidas en la app.

## Widgets de escritorio
- Viven en una capa de escritorio: cada ventana tiene como dueño a Progman y se coloca justo encima de él. Quedan por debajo de las demás ventanas y se ven con Win+D.
- Una ventana por widget, y solo uno de cada tipo.
- La edición se activa desde la bandeja. Una barra con el catálogo permite añadir, quitar, ajustar la opacidad y salir con LISTO o Esc.
- Rejilla de 20 px para mover y redimensionar, con un tamaño mínimo por tipo.
- Marco HUD semitransparente. El texto lleva sombra para leerse sobre cualquier fondo.
- Con el widget tapado o la app en modo Ocultar, el contenido deja de trabajar: sin temporizadores y sin repintar. Al volver a verse se repinta al instante.
- Los datos llegan por eventos. El sistema se mide cada 3 s cuando solo lo mira el widget.

## Fuera de la v1
- Escribir en el calendario (Microsoft Graph).
- Microsoft To Do y la API de Spotify.
- Métricas de red.
