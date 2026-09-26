import { Card, Spacer, Text } from "rootik";

type Props = {
  /// What we look at and how: the "Form / Code" view, the document, the preset.
  start?: React.ReactNode;
  /// What to do: revert, reset, save. Always on the right — one place in every section.
  end?: React.ReactNode;
  /// What the document is — a second line in the same card, not text between cards.
  hint?: React.ReactNode;
};

/**
 * A section's bar — its own card above the content (D-146). One for both "Form / Code"
 * views, so it does not jump on switching (D-067).
 *
 * One rule for every section: on the left, the choice of what we look at; on the right,
 * actions on it. One control size, the kit default: a bar where the switch is smaller than
 * the button next to it reads as two different devices. The document is picked from a list
 * everywhere — preset, source, settings file: two switches in a row merged into one. The
 * explanation is its own line under the controls: in one row with the buttons it did not
 * fit and got cut mid-word.
 */
export default function SectionBar({ start, end, hint }: Props) {
  return (
    <Card padding="sm" className="shrink-0">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        {start}
        <Spacer />
        {end}
      </div>
      {hint && (
        <Text tone="muted" size="xs" className="mt-2 block">
          {hint}
        </Text>
      )}
    </Card>
  );
}
