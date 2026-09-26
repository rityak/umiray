// Разговор с вебвью приложения по CDP — тем же протоколом, что у devtools.
//
// Окно Tauri нативное, снаружи по нему не щёлкнешь. Но WebView2 умеет отдавать отладочный
// порт, и через него доступны и клики, и клавиши, и снимки экрана, и замеры.
//
// Приложение поднимается так:
//   set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
//   npm run tauri dev
//
// Зависимостей нет намеренно: WebSocket и fetch есть в самом Node с 22-й версии,
// а тащить ради полусотни строк puppeteer — это полторы сотни мегабайт на проверку.

import { mkdirSync, writeFileSync } from "node:fs";

/// Куда падают снимки. Не корень репозитория: это вывод проверки, а не исходник, и в корне
/// он оседал под git — шесть PNG, часть от инструмента, которого уже нет.
export const SHOTS_DIR = "shots";

/** Соединение с вкладкой: запрос-ответ по номеру, как того требует CDP. */
export class Session {
  constructor(url, shots = SHOTS_DIR) {
    this.socket = new WebSocket(url);
    this.shots = shots;
    this.next = 0;
    this.pending = new Map();
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const waiting = this.pending.get(message.id);
      if (!waiting) return;
      this.pending.delete(message.id);
      if (message.error) waiting.reject(new Error(message.error.message));
      else waiting.resolve(message.result);
    });
  }

  ready() {
    return new Promise((resolve, reject) => {
      this.socket.addEventListener("open", resolve, { once: true });
      this.socket.addEventListener("error", () => reject(new Error("вебвью не отвечает")), {
        once: true,
      });
    });
  }

  send(method, params = {}) {
    this.next += 1;
    const id = this.next;
    this.socket.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
  }

  /// Значение выражения из страницы. `await` внутри разрешён — им ждут перерисовку.
  async eval(expression) {
    const { result, exceptionDetails } = await this.send("Runtime.evaluate", {
      expression: `(async () => { ${expression} })()`,
      awaitPromise: true,
      returnByValue: true,
    });
    if (exceptionDetails) {
      throw new Error(exceptionDetails.exception?.description ?? "ошибка в странице");
    }
    return result.value;
  }

  async click(selector) {
    return this.eval(`
      const node = document.querySelector(${JSON.stringify(selector)});
      if (!node) return false;
      node.click();
      await new Promise((r) => setTimeout(r, 250));
      return true;
    `);
  }

  /// Настоящее нажатие клавиши, а не синтетическое событие: Esc у <dialog> обрабатывает
  /// сам браузер, и на `dispatchEvent` из скрипта он не реагирует.
  async key(key, code) {
    for (const type of ["rawKeyDown", "keyUp"]) {
      await this.send("Input.dispatchKeyEvent", {
        type,
        key,
        windowsVirtualKeyCode: code,
        nativeVirtualKeyCode: code,
      });
    }
    await this.eval("await new Promise((r) => setTimeout(r, 250)); return 1;");
  }

  /// Настоящее нажатие мышью по центру элемента. Отличается от `node.click()` тем,
  /// что переносит фокус: без этого <dialog> нечего возвращать при закрытии.
  async clickReal(selector) {
    const box = await this.eval(`
      const node = document.querySelector(${JSON.stringify(selector)});
      if (!node) return null;
      const r = node.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    `);
    if (!box) return false;
    await this.clickAt(box.x, box.y);
    return true;
  }

  async clickAt(x, y) {
    for (const type of ["mousePressed", "mouseReleased"]) {
      await this.send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
    }
    await this.eval("await new Promise((r) => setTimeout(r, 250)); return 1;");
  }

  /// Ждём условие, а не «подольше»: сон на глазок либо тормозит проверку, либо врёт.
  async until(expression, { timeout = 20000, step = 500 } = {}) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (await this.eval(`return ${expression};`)) return true;
      await new Promise((r) => setTimeout(r, step));
    }
    return false;
  }

  async shot(name) {
    const { data } = await this.send("Page.captureScreenshot", { format: "png" });
    const path = `${this.shots}/${name}.png`;
    mkdirSync(this.shots, { recursive: true });
    writeFileSync(path, Buffer.from(data, "base64"));
    return path;
  }
}

/// Адрес dev-сервера. Отладочный порт бывает только у dev-сборки, так что другого тут
/// и не встретится.
const DEV = "http://localhost:1420/";

/// Подключиться к единственной вкладке приложения и перезагрузить её.
///
/// Перезагрузка обязательна: горячая замена модулей может оставить в окне прежний
/// компонент, и проверка отчитается о состоянии, которого в коде уже нет (S-014).
///
/// Окно иногда поднимается с пустой страницей — гонка между созданием вебвью и готовностью
/// dev-сервера. Снаружи это выглядит как «приложение сломалось»: проверки падают на
/// `document.querySelector(...) === null`. Поэтому здесь же и лечим.
export async function attach({ port = 9222, shots = SHOTS_DIR, reload = true } = {}) {
  const pages = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
  // Своя вкладка — по адресу dev-сервера: у браузера рядом бывают служебные страницы.
  const page =
    pages.find((target) => target.type === "page" && target.url.startsWith(DEV)) ??
    pages.find((target) => target.type === "page");
  if (!page) {
    console.error("вкладки нет: приложение запущено без --remote-debugging-port?");
    process.exit(2);
  }
  const session = new Session(page.webSocketDebuggerUrl, shots);
  await session.ready();
  await session.send("Page.enable");
  await session.send("Runtime.enable");
  if (reload) {
    const blank = await session.eval("return location.href === 'about:blank';");
    if (blank) await session.send("Page.navigate", { url: DEV });
    else await session.send("Page.reload", {});
    // Ждём признак готовности, а не «подольше»: сон на глазок либо тормозит проверку,
    // либо срабатывает раньше, чем окно нарисовалось, — и то и другое уже случалось.
    if (!(await session.until("document.querySelector('header') !== null", { timeout: 20000 }))) {
      console.error("окно не нарисовалось: dev-сервер запущен? (`npm run dev`)");
      process.exit(2);
    }
  }
  return session;
}
