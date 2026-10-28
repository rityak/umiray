// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SectionGuard from "./SectionGuard";

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  // React пишет пойманное исключение в консоль сам; в выводе теста это шум.
  vi.spyOn(console, "error").mockImplementation(() => undefined);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  vi.restoreAllMocks();
});

let broken = true;

function Routing() {
  if (broken) throw new Error("rules is not iterable");
  return <p>routes</p>;
}

/// B-054: исключение в «Маршрутизации» снимало всё дерево React — окно уходило в голый
/// фон, без шапки и dock, и уйти из раздела было нечем.
test("a section that throws shows the failure in its place; header and dock stay", () => {
  broken = true;
  act(() =>
    root.render(
      <>
        <header>title bar</header>
        <SectionGuard>
          <Routing />
        </SectionGuard>
        <nav>dock</nav>
      </>,
    ),
  );
  expect(host.querySelector("header")?.textContent).toBe("title bar");
  expect(host.querySelector("nav")?.textContent).toBe("dock");
  expect(host.textContent).toContain("rules is not iterable");
  expect(host.textContent).not.toContain("routes");

  broken = false;
  const retry = [...host.querySelectorAll("button")].find((button) =>
    button.textContent?.includes("Try again"),
  );
  expect(retry).toBeDefined();
  act(() => retry?.click());
  expect(host.textContent).toContain("routes");
});
