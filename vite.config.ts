import { defineConfig } from "vite";
import { resolve } from "node:path";

// Multipágina: index.html (panel), tab.html (pestaña de borde) y las ventanas de aviso.
export default defineConfig({
  clearScreen: false,
  // Vite no debe vigilar el Rust: de eso se encarga Tauri (y target/ da EBUSY)
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: {
    target: "chrome105",
    rollupOptions: {
      input: {
        panel: resolve(import.meta.dirname, "index.html"),
        tab: resolve(import.meta.dirname, "tab.html"),
        aviso: resolve(import.meta.dirname, "aviso.html"),
        avisoConsumo: resolve(import.meta.dirname, "aviso-consumo.html"),
        consumo: resolve(import.meta.dirname, "consumo.html"),
        tarjeta: resolve(import.meta.dirname, "tarjeta.html"),
        fantasma: resolve(import.meta.dirname, "fantasma.html"),
        widget: resolve(import.meta.dirname, "widget.html"),
        widgetsBarra: resolve(import.meta.dirname, "widgets-barra.html"),
      },
    },
  },
});
