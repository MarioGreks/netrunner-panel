import "./styles/tokens.css";
import "./styles/temas.css";
import "./styles/tab.css";
import { iniciarTema } from "./lib/tema";
import { alEvento, alternarPanel, obtenerConfig, type Borde } from "./lib/ipc";

// El borde decide la orientación: vertical en los lados, horizontal arriba
const aplicarBorde = (borde: Borde) => {
  document.documentElement.dataset.borde = borde;
};
iniciarTema();
aplicarBorde("right");
void alEvento<Borde>("config://cambio", aplicarBorde);
obtenerConfig().then((c) => aplicarBorde(c.edge)).catch(() => {});

document.getElementById("tira")!.addEventListener("click", () => {
  void alternarPanel();
});
