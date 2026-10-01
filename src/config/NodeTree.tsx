import { Text, Tree, type TreeNode } from "rootik";
import { t } from "../i18n";
import { type Choices, nodesOf } from "./groups";

type Props = {
  choices: Choices;
  /// Checked nodes by name. "A whole source" means "all of its nodes are checked".
  picked: string[];
  /// The group is not the form's to edit (its own filter in the file): shown, not touched.
  disabled: boolean;
  onPicked: (picked: string[]) => void;
};

/** A "source → its nodes" tree with checkboxes: checking a source checks all its nodes. */
export default function NodeTree({ choices, picked, disabled, onPicked }: Props) {
  if (choices.sources.length === 0) {
    return (
      <Text tone="muted" size="xs" className="block">
        {t("No sources, so no nodes. Add a subscription or a link.")}
      </Text>
    );
  }
  const items: TreeNode[] = choices.sources.map((source) => ({
    id: `source:${source.id}`,
    label: source.name,
    disabled,
    children: nodesOf(choices, source.id).map((node) => ({
      id: node.name,
      label: node.name,
      trailing: node.kind,
      disabled,
    })),
  }));
  const known = new Set(choices.nodes.map((node) => node.name));
  return (
    <Tree
      aria-label={t("Group nodes")}
      checkable
      items={items}
      checked={picked.filter((name) => known.has(name))}
      // Names missing from the catalog (other groups, DIRECT) are not shown in the tree —
      // and must not be lost when checkboxes change.
      onCheckedChange={(leaves) =>
        onPicked([...leaves, ...picked.filter((name) => !known.has(name))])
      }
    />
  );
}
