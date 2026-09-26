import { Badge, type Tone, Tooltip } from "rootik";
import { delayHint, delayLabel, type Node } from "../api";
import { t } from "../i18n";

/// One "near · tolerable · far" scale for every measuring method (D-069). The fallback
/// measurement gets its own tone: there is a number, but it answers a different question.
export function delayTone(node: Node): Tone {
  if (node.delay === null) return "neutral";
  if (node.fallback) return "info";
  if (node.delay < 150) return "success";
  if (node.delay < 400) return "warn";
  return "danger";
}

/// Delay to the server as a badge. The fallback is named in words for screen readers, not by colour alone.
export default function NodeDelay({ node }: { node: Node }) {
  return (
    <Tooltip content={delayHint(node)}>
      <Badge size="sm" tone={delayTone(node)} className="rk-num">
        {delayLabel(node)}
        {node.fallback && <span className="sr-only">{t(", measured by the fallback method")}</span>}
      </Badge>
    </Tooltip>
  );
}
