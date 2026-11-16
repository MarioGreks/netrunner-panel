import "./styles/tokens.css";
import "./styles/tab.css";
import { alternarPanel } from "./lib/ipc";

document.getElementById("tira")!.addEventListener("click", () => {
  void alternarPanel();
});
