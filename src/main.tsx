import React from "react";
import ReactDOM from "react-dom/client";
import { en, RootikProvider, ru } from "rootik";
import App from "./App";
import { systemFeatures, systemLanguage } from "./api";
import { setFeatures } from "./features";
import { getLang, setLang, t } from "./i18n";
import * as splash from "./splash";
import "./styles.css";

// The kit owns the look entirely (D-142): the provider stores it itself, the window only reads.
const render = () =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <RootikProvider storageKey="umiray:appearance" labels={getLang() === "ru" ? ru : en}>
        <App />
      </RootikProvider>
    </React.StrictMode>,
  );

// The language comes first: every text below depends on it. A backend that does not answer
// is no reason to keep the window closed — English it is, with every feature shown.
// The OS features are as fixed as the language (D-174) and come with it.
const start = () =>
  Promise.all([
    systemLanguage().then(setLang, () => setLang("en")),
    systemFeatures().then(setFeatures, () => undefined),
  ]).then(() => {
    // The bundle has arrived; from here on we wait for the backend. `App` removes the
    // splash when the first data comes: an empty shell is not readiness either.
    splash.step(t("building the window"));
    render();
  });

// A plain browser has no Tauri — a stub answers there. The branch never reaches a build.
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  import("./dev/mock").then(start);
} else {
  start();
}
