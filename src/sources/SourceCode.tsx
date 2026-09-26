import { useEffect } from "react";
import { Card, EmptyState, Spinner } from "rootik";
import * as api from "../api";
import type { Draft } from "../config/draft";
import Editor from "../config/Editor";
import { t, tn } from "../i18n";
import { failure, type Message } from "../shell/Banner";

type Props = {
  /// Whose text is shown. Picked by the section's source list (D-070); absent — there are no
  /// sources at all.
  source: api.Source | undefined;
  hidden: boolean;
  draft: Draft | undefined;
  onDraft: (id: string, text: string) => void;
  onDisk: (id: string, text: string) => void;
  onMessage: (message: Message) => void;
};

/**
 * A source as text (D-065). The section's bar (`SaveActions`) writes it; only the editor
 * lives here — at the full remaining height.
 *
 * What is edited is the **raw** text — what the panel sent. The assembled file the core
 * reads is rebuilt from it on every refresh: an edit there would live until the first
 * "Refresh", and silently.
 */
export default function SourceCode({ source, hidden, draft, onDraft, onDisk, onMessage }: Props) {
  const id = source?.id;

  useEffect(() => {
    if (id === undefined || hidden) return;
    api.sourcesRead(id).then(
      (disk) => onDisk(id, disk),
      (e) => onMessage(failure(e)),
    );
  }, [hidden, id, onDisk, onMessage]);

  if (source === undefined) {
    return (
      <Card>
        <EmptyState
          title={t("No sources")}
          hint={t("Add a subscription or a link — what the panel sent will show up here.")}
        />
      </Card>
    );
  }

  if (hidden) {
    return (
      <Card>
        <EmptyState
          title={t("Code hidden")}
          hint={t("Turn off private mode to see the source's links and credentials.")}
        />
      </Card>
    );
  }

  if (draft === undefined) {
    return <Spinner label={t("Reading the source")} />;
  }

  return (
    <Card
      className="min-h-0 flex-1"
      padding="sm"
      title={tn(source.nodes, "{n} node", "{n} nodes")}
      description={
        source.url === null
          ? t("Your links — nobody overwrites them.")
          : t("Will be overwritten on the next subscription refresh.")
      }
    >
      {/* Not YAML: this is a list of links, and `#` in them is a node name, not a comment. */}
      <div className="h-full min-h-0">
        <Editor value={draft.text} onChange={(text) => onDraft(source.id, text)} plain />
      </div>
    </Card>
  );
}
