import { describe, expect, it } from "vitest";
import type * as api from "../api";
import { activeFirst, entries, groupLabel, groupName, iconOf, inAuto, load, share } from "./groups";

const node = (name: string, source: string, kind = "Vless"): api.Node => ({
  name,
  kind,
  source,
  supported: true,
  delay: null,
  method: null,
  fallback: false,
  address: null,
  country: null,
  edited: false,
});

const group = (name: string, sources: string[], proxies: string[] = []): api.Group => ({
  name,
  kind: "select",
  sources,
  proxies,
  filter: null,
  url: null,
  interval: null,
  tolerance: null,
  strategy: null,
  extra: [],
  origin: 0,
});

const nodes = [node("P1", "a"), node("P2", "a", "TUIC"), node("R1", "b")];
const choices = { sources: [], nodes };

describe("groups tab", () => {
  it("puts AUTO first, own groups next, other client groups last", () => {
    const list = entries(
      [
        { name: "AUTO", kind: "load-balance", members: ["P1"] },
        { name: "umiray-geo-pl", kind: "url-test", members: ["P1", "P2"] },
      ],
      [group("Mine", ["a"], ["DIRECT"])],
      choices,
    );
    expect(list.map((entry) => entry.name)).toEqual(["AUTO", "Mine", "umiray-geo-pl"]);
    expect(list[1]).toMatchObject({ own: true, members: ["P1", "P2", "DIRECT"] });
    expect(list[2].own).toBe(false);
    expect(activeFirst(list, "umiray-geo-pl").map((entry) => entry.name)).toEqual([
      "umiray-geo-pl",
      "AUTO",
      "Mine",
    ]);
    expect(activeFirst(list, "gone")).toBe(list);
  });

  it("names client groups for the eye and gives them a meaning icon", () => {
    const proto = { name: "umiray-proto-tuic", kind: "url-test", members: ["P2"], own: false };
    expect(groupLabel(proto, nodes)).toBe("TUIC");
    expect(iconOf(proto, {})).toBe("lucide:Layers");
    expect(iconOf({ ...proto, name: "umiray-geo-pl" }, {})).toBe("flag:pl");
    expect(iconOf({ ...proto, name: "Mine", own: true }, {})).toBeNull();
    expect(iconOf({ ...proto, name: "Mine", own: true }, { Mine: "flag:de" })).toBe("flag:de");
  });

  // Служебные имена сборки человек не видит нигде: ни в «Маршрутизации», ни в карточке выхода.
  it("never shows the build's service names", () => {
    expect(groupName("umiray")).not.toContain("umiray");
    expect(groupName("umiray-proto-hysteria2")).toBe("Hysteria2");
    expect(groupName("umiray-udp")).not.toContain("umiray");
    expect(groupName("umiray-geo-pl")).not.toContain("umiray");
    expect(groupName("My servers")).toBe("My servers");
  });

  it("names the busiest member and counts the group's connections", () => {
    const rates = {
      P1: { down: 10, up: 0, connections: 2 },
      P2: { down: 900, up: 0, connections: 6 },
      R1: { down: 0, up: 0, connections: 0 },
    };
    expect(load(["P1", "P2", "R1"], rates)).toEqual({ lead: "P2", connections: 8 });
    expect(load(["R1"], rates)).toEqual({ lead: null, connections: 0 });
    expect(share("P1", rates)).toBeCloseTo(1 / 3);
    expect(share("R1", {})).toBe(0);
  });

  it("knows who is in AUTO under exclusions", () => {
    const exclude = { sources: ["b"], nodes: ["P2"] };
    expect(nodes.map((item) => inAuto(item, exclude))).toEqual([true, false, false]);
  });
});
