import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/600.css";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/600.css";
import "./styles.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app";

const page = document.documentElement;
window.addEventListener(
  "keydown",
  () => page.toggleAttribute("data-keys", true),
  true
);
window.addEventListener(
  "pointerdown",
  () => page.toggleAttribute("data-keys", false),
  true
);

window.addEventListener("contextmenu", (event) => {
  const { target } = event;
  const editable =
    target instanceof Element &&
    target.closest("input, textarea, [contenteditable]") !== null;
  if (!editable && !window.getSelection()?.toString()) {
    event.preventDefault();
  }
});

const root = document.querySelector("#root");

if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>
  );
}
