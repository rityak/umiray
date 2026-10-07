import * as flags from "country-flag-icons/react/3x2";
import { memo } from "react";
import { Badge, Tooltip } from "rootik";
import { t } from "../i18n";

/// A flag by code, or nothing: `country-flag-icons` covers every ISO country plus the EU.
export function flagOf(code: string) {
  // biome-ignore lint/performance/noDynamicNamespaceImportAccess: any country can come, the whole set is wanted
  return flags[code.toUpperCase() as keyof typeof flags] as
    | ((props: { className?: string; title?: string }) => React.ReactNode)
    | undefined;
}

/// Every code the flag set knows, for pickers.
export const FLAG_CODES: string[] = Object.keys(flags).filter((code) => /^[A-Z]{2}$/.test(code));

/// The SVG alone at the 16×11 size used beside node names.
export function FlagSvg({ code, className }: { code: string; className?: string }) {
  const Svg = flagOf(code);
  if (Svg === undefined) return null;
  return (
    <Svg
      className={`h-[11px] w-4 shrink-0 rounded-[1.5px] outline outline-1 -outline-offset-1 outline-[oklch(1_0_0/0.1)] ${className ?? ""}`}
    />
  );
}

/**
 * Show the server country beside its name (D-084, D-085). SVG avoids Windows rendering flag
 * emoji as two letters and scales consistently.
 */
/// Traffic ticks do not change a flag's country.
export default memo(function Flag({ country }: { country: string | null }) {
  if (country === null || !/^[A-Za-z]{2}$/.test(country)) return null;
  const code = country.toUpperCase();
  const title = t("Server country: {code}", { code });
  return (
    <Tooltip content={title}>
      {flagOf(code) === undefined ? (
        <Badge size="sm" variant="outline">
          {code}
        </Badge>
      ) : (
        <span role="img" aria-label={title} className="inline-flex">
          <FlagSvg code={code} />
        </span>
      )}
    </Tooltip>
  );
});
