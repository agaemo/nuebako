import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { HistoryWindow } from "./features/history/HistoryWindow";
import { SettingsWindow } from "./features/settings/SettingsWindow";
import "./styles/index.css";

/** ウィンドウのラベル(history / settings)で、表示する画面を切り替える。 */
const label = getCurrentWindow().label;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{label === "settings" ? <SettingsWindow /> : <HistoryWindow />}</React.StrictMode>,
);
