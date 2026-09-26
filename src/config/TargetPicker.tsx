import { Select } from "rootik";
import { targetLook } from "./kinds";

type Props = {
  value: string;
  /// Groups and route ends: what a rule usually targets.
  groups: string[];
  /// Nodes the core will actually bring up (D-082).
  nodes: string[];
  label: string;
  onChange: (target: string) => void;
};

/**
 * "Where to" — one list: groups and route ends first, then nodes (D-082). An item's icon
 * and hint say what it is: through VPN, bypass, block, group or server. The current value
 * is always in the list — otherwise the field would show emptiness where the document
 * says something.
 */
export default function TargetPicker({ value, groups, nodes, label, onChange }: Props) {
  const all = [...new Set([value, ...groups, ...nodes])];
  const options = all.map((target) => {
    const look = targetLook(target, nodes);
    return {
      value: target,
      label: target,
      hint: look.word,
      // The icon goes straight into the kit's slot: rootik sets size (1em) and stroke, no
      // wrapper of our own. Only outcomes are coloured, and the colour comes from the icon.
      icon: <look.Icon className="um-tone" data-tone={look.tone} />,
    };
  });
  return <Select aria-label={label} value={value} onChange={onChange} options={options} />;
}
