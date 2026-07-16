import { mount } from "svelte";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import App from "./App.svelte";
import "./app.css";

// Одно приложение обслуживает все окна; компонент выбирается по label окна.
const label = getCurrentWebviewWindow().label;

mount(App, {
  target: document.getElementById("app")!,
  props: { windowLabel: label },
});
