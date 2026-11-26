# Netrunner móvil

Versión móvil del panel Netrunner (Expo + React Native + TypeScript + expo-router), con la estética HUD del panel de escritorio.

## Arrancar

```
npm install
npx expo start
```

Abre el código QR con Expo Go.

## Conectar con el panel del PC

La app sincroniza tareas y notas por la wifi de casa con un servidor HTTP del panel de escritorio (puerto 47821 por defecto). No usa cuentas ni la nube.

1. En el PC, con el panel abierto: Ajustes → Conexión móvil → Activar → Emparejar móvil (muestra un código de 6 cifras).
2. En el móvil (misma wifi): Ajustes → Panel del PC → escribe la IP del PC, el puerto y el código → Emparejar.
3. El token queda guardado en el almacén seguro del móvil. Si caduca, la app pide emparejar de nuevo.

Sin conexión, la app funciona con su copia local y sincroniza cuando el panel vuelve a estar alcanzable.

## Pendiente para el APK

El panel va por HTTP sin cifrar: Android lo bloquea en builds de producción. En el paso del APK habrá que añadir `expo-build-properties` con `android.usesCleartextTraffic: true` (no instalado todavía).
