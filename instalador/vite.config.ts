import { defineConfig } from "vite";

// Instalador HUD: ventana propia (root = instalador/), aparte de la app.
export default defineConfig({
  root: import.meta.dirname,
  clearScreen: false,
  base: "./",
  server: { port: 1430, strictPort: true },
  build: { target: "chrome105", outDir: "dist", emptyOutDir: true },
});
