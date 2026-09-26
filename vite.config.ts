import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

import { version } from "./package.json";

export default defineConfig({
  // React Compiler мемоизирует компоненты сам: ручные `memo` и `useCallback` на горячих
  // местах остаются, но новое писать без них. Компонент, нарушающий правила React
  // (запись в ref во время рендера), компилятор молча пропускает — он работает как раньше.
  plugins: [react({ babel: { plugins: ["babel-plugin-react-compiler"] } }), tailwindcss()],
  // Версия клиента приезжает в окно из `package.json`: её называет отчёт диагностики,
  // и держать второе такое число в коде значило бы однажды соврать в отчёте.
  define: { __APP_VERSION__: JSON.stringify(version) },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
