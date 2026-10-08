import { describe, expect, it } from "vitest";
import { blocks, spans } from "./notes";

describe("release notes", () => {
  it("reads a CHANGELOG section into headings, lists and paragraphs", () => {
    const notes =
      "### Changes\r\n\r\n- Update rootik, groups\r\n  and settings\r\n- Second\r\n\r\n### Fixes\r\n- One\r\n\r\nFor Windows x64, download\r\nthe setup.";
    expect(blocks(notes)).toEqual([
      { kind: "heading", text: "Changes" },
      { kind: "list", items: ["Update rootik, groups and settings", "Second"] },
      { kind: "heading", text: "Fixes" },
      { kind: "list", items: ["One"] },
      { kind: "text", text: "For Windows x64, download the setup." },
    ]);
  });

  it("marks code and bold, leaves the rest as text", () => {
    expect(spans("get `umiray_1.4.0.exe` **now** <b>")).toEqual([
      { kind: "text", text: "get " },
      { kind: "code", text: "umiray_1.4.0.exe" },
      { kind: "text", text: " " },
      { kind: "strong", text: "now" },
      { kind: "text", text: " <b>" },
    ]);
  });
});
