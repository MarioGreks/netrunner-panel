import { defineConfig } from "vite";
import { resolve } from "node:path";

// Multipágina: index.html (panel) y tab.html (pestaña de borde).
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
      },
    },
  },
});
