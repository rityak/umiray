import { memo } from "react";
import { Badge, Tooltip } from "rootik";
import { t } from "../i18n";

/**
 * Recognizable flags built from simple shapes. Complex emblems fall back to a
 * country-code badge rather than an inaccurate drawing.
 */
type Drawn =
  /// Horizontal stripes from top to bottom.
  | { rows: string[] }
  /// Vertical stripes from left to right.
  | { cols: string[] }
  /// Nordic cross, optionally with an inner cross.
  | { field: string; cross: string; inner?: string }
  /// Centered cross, as on Switzerland's flag.
  | { field: string; plus: string }
  /// Stripes and an upper-left canton containing stars or a cross.
  | { rows: string[]; canton: string; stars?: number; cantonCross?: string }
  /// Solid field and disc, as on Japan's flag.
  | { field: string; disc: string }
  /// Corner stars, as on China's flag.
  | { field: string; corner: string; count: number }
  /// Hoist stripe alongside horizontal stripes.
  | { hoist: string; rows: string[] };

const FLAGS: Record<string, Drawn> = {
  nl: { rows: ["#AE1C28", "#ffffff", "#21468B"] },
  ru: { rows: ["#ffffff", "#0039A6", "#D52B1E"] },
  ee: { rows: ["#0072CE", "#000000", "#ffffff"] },
  de: { rows: ["#000000", "#DD0000", "#FFCE00"] },
  at: { rows: ["#ED2939", "#ffffff", "#ED2939"] },
  lv: { rows: ["#9E3039", "#ffffff", "#9E3039"] },
  lt: { rows: ["#FDB913", "#006A44", "#C1272D"] },
  hu: { rows: ["#CD2A3E", "#ffffff", "#436F4D"] },
  bg: { rows: ["#ffffff", "#00966E", "#D62612"] },
  ua: { rows: ["#0057B7", "#FFD700"] },
  pl: { rows: ["#ffffff", "#DC143C"] },
  id: { rows: ["#CE1126", "#ffffff"] },
  fr: { cols: ["#002395", "#ffffff", "#ED2939"] },
  it: { cols: ["#009246", "#ffffff", "#CE2B37"] },
  ro: { cols: ["#002B7F", "#FCD116", "#CE1126"] },
  ie: { cols: ["#169B62", "#ffffff", "#FF883E"] },
  be: { cols: ["#000000", "#FDDA24", "#EF3340"] },
  se: { field: "#006AA7", cross: "#FECC00" },
  fi: { field: "#ffffff", cross: "#003580" },
  dk: { field: "#C8102E", cross: "#ffffff" },
  no: { field: "#BA0C2F", cross: "#ffffff", inner: "#00205B" },
  is: { field: "#02529C", cross: "#ffffff", inner: "#DC1E35" },
  ch: { field: "#DA291C", plus: "#ffffff" },
  jp: { field: "#ffffff", disc: "#BC002D" },
  us: {
    rows: Array.from({ length: 13 }, (_, at) => (at % 2 === 0 ? "#B31942" : "#ffffff")),
    canton: "#0A3161",
    stars: 12,
  },
  gr: {
    rows: [
      "#0D5EAF",
      "#ffffff",
      "#0D5EAF",
      "#ffffff",
      "#0D5EAF",
      "#ffffff",
      "#0D5EAF",
      "#ffffff",
      "#0D5EAF",
    ],
    canton: "#0D5EAF",
    cantonCross: "#ffffff",
  },
  th: { rows: ["#A51931", "#ffffff", "#2D2A4A", "#ffffff", "#A51931"] },
  cn: { field: "#EE1C25", corner: "#FFFF00", count: 5 },
  ae: { hoist: "#FF0000", rows: ["#00732F", "#ffffff", "#000000"] },
};

/// Match the compact 16×11 flag proportions used beside node names.
const W = 16;
const H = 11;

function Stripes({ colors, vertical }: { colors: string[]; vertical: boolean }) {
  const step = (vertical ? W : H) / colors.length;
  return (
    <>
      {colors.map((color, at) => (
        <rect
          key={color + String(at)}
          x={vertical ? at * step : 0}
          y={vertical ? 0 : at * step}
          width={vertical ? step : W}
          height={vertical ? H : step}
          fill={color}
        />
      ))}
    </>
  );
}

/// Alternate inner and outer radii at 36° intervals for a five-pointed star.
function star(cx: number, cy: number, r: number): string {
  return Array.from({ length: 10 }, (_, at) => {
    const radius = at % 2 === 0 ? r : r * 0.382;
    const angle = (Math.PI / 5) * at - Math.PI / 2;
    return `${(cx + radius * Math.cos(angle)).toFixed(2)},${(cy + radius * Math.sin(angle)).toFixed(2)}`;
  }).join(" ");
}

/// Nordic crosses are offset toward the hoist.
function Cross({ field, cross, inner }: { field: string; cross: string; inner?: string }) {
  return (
    <>
      <rect width={W} height={H} fill={field} />
      <rect x={4.6} width={2.8} height={H} fill={cross} />
      <rect y={4.1} width={W} height={2.8} fill={cross} />
      {inner !== undefined && (
        <>
          <rect x={5.4} width={1.2} height={H} fill={inner} />
          <rect y={4.9} width={W} height={1.2} fill={inner} />
        </>
      )}
    </>
  );
}

/// New flag shapes extend this renderer and the data table.
function Shape({ drawn }: { drawn: Drawn }) {
  if ("canton" in drawn) {
    const high = (Math.ceil(drawn.rows.length / 2) / drawn.rows.length) * H;
    const wide = W * 0.4;
    return (
      <>
        <Stripes colors={drawn.rows} vertical={false} />
        <rect width={wide} height={high} fill={drawn.canton} />
        {drawn.cantonCross !== undefined && (
          <>
            <rect x={wide / 2 - 0.5} width={1} height={high} fill={drawn.cantonCross} />
            <rect y={high / 2 - 0.5} width={wide} height={1} fill={drawn.cantonCross} />
          </>
        )}
        {Array.from({ length: drawn.stars ?? 0 }, (_, at) => (
          <circle
            // biome-ignore lint/suspicious/noArrayIndexKey: stars differ only by position
            key={at}
            cx={((at % 4) + 0.5) * (wide / 4)}
            cy={(Math.floor(at / 4) + 0.5) * (high / 3)}
            r={0.4}
            fill="#ffffff"
          />
        ))}
      </>
    );
  }
  if ("corner" in drawn) {
    return (
      <>
        <rect width={W} height={H} fill={drawn.field} />
        <polygon points={star(3.2, 3.4, 2)} fill={drawn.corner} />
        {Array.from({ length: drawn.count - 1 }, (_, at) => (
          <polygon
            // biome-ignore lint/suspicious/noArrayIndexKey: stars differ only by position
            key={at}
            points={star(6.4, 1.2 + at * 1.5, 0.62)}
            fill={drawn.corner}
          />
        ))}
      </>
    );
  }
  if ("disc" in drawn) {
    return (
      <>
        <rect width={W} height={H} fill={drawn.field} />
        <circle cx={W / 2} cy={H / 2} r={3.3} fill={drawn.disc} />
      </>
    );
  }
  if ("plus" in drawn) {
    return (
      <>
        <rect width={W} height={H} fill={drawn.field} />
        <rect x={W / 2 - 1.1} y={2.2} width={2.2} height={6.6} fill={drawn.plus} />
        <rect x={W / 2 - 3.3} y={4.4} width={6.6} height={2.2} fill={drawn.plus} />
      </>
    );
  }
  if ("hoist" in drawn) {
    return (
      <>
        <g transform={`translate(${W * 0.28} 0) scale(${1 - 0.28} 1)`}>
          <Stripes colors={drawn.rows} vertical={false} />
        </g>
        <rect width={W * 0.28} height={H} fill={drawn.hoist} />
      </>
    );
  }
  if ("cross" in drawn) {
    return <Cross field={drawn.field} cross={drawn.cross} inner={drawn.inner} />;
  }
  if ("cols" in drawn) return <Stripes colors={drawn.cols} vertical={true} />;
  return <Stripes colors={drawn.rows} vertical={false} />;
}

/**
 * Show the server country beside its name (D-084, D-085). SVG avoids Windows
 * rendering flag emoji as two letters and scales consistently.
 */
/// Traffic ticks do not change a flag's country.
export default memo(function Flag({ country }: { country: string | null }) {
  if (country === null || !/^[A-Za-z]{2}$/.test(country)) return null;
  const code = country.toUpperCase();
  const drawn = FLAGS[country.toLowerCase()];
  const title = t("Server country: {code}", { code });

  // Country codes preserve the meaning of unsupported flag shapes.
  if (drawn === undefined) {
    return (
      <Tooltip content={title}>
        <Badge size="sm" variant="outline">
          {code}
        </Badge>
      </Tooltip>
    );
  }

  return (
    <Tooltip content={title}>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        width={W}
        height={H}
        role="img"
        aria-label={title}
        className="shrink-0 rounded-[1.5px] outline outline-1 -outline-offset-1 outline-[oklch(1_0_0/0.1)]"
      >
        <Shape drawn={drawn} />
      </svg>
    </Tooltip>
  );
});
