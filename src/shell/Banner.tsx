import { Button, Callout } from "rootik";
import { type AppError, asAppError } from "../api";
import { t, tk } from "../i18n";

/// A message to the user: a line and, as a separate list, the reasons — core log lines or
/// the provider's answer. They must not be glued into the text: there can be many (D-028).
export type Message = {
  tone: "error" | "info";
  text: string;
  details: string[];
  /// The machine reason from the backend. The action is picked by it: an error text without
  /// a button is a sign of an unfinished state (STYLEGUIDE).
  kind?: string;
};

export const failure = (raw: unknown): Message => {
  const error: AppError = asAppError(raw);
  return { tone: "error", text: error.message, details: error.details, kind: error.kind };
};

export const notice = (text: string, details: string[] = []): Message => ({
  tone: "info",
  text,
  details,
});

type Props = {
  message: Message | null;
  onInstall: () => void;
  onElevate: () => void;
  onRestart: () => void;
  onDismiss?: () => void;
};

/// The `kind`s that have something to offer pressing. Other errors have no action —
/// inventing one would be lying. `restartNeeded` comes from state, not from an error
/// (D-060), but the button means the same — "do what the text says".
const ACTIONS: Record<string, { label: string; pick: (props: Props) => () => void }> = {
  coreMissing: { label: tk("Download the core"), pick: (props) => props.onInstall },
  needsElevation: { label: tk("Restart as admin"), pick: (props) => props.onElevate },
  restartNeeded: { label: tk("Restart VPN"), pick: (props) => props.onRestart },
};

export default function Banner(props: Props) {
  const { message, onDismiss } = props;
  if (!message) return null;
  const action = message.kind ? ACTIONS[message.kind] : undefined;

  return (
    <Callout
      live
      tone={message.tone === "error" ? "danger" : "info"}
      title={message.text}
      onDismiss={onDismiss}
      actions={
        action && (
          <Button size="sm" variant="primary" onClick={action.pick(props)}>
            {t(action.label)}
          </Button>
        )
      }
    >
      {message.details.length > 0 && (
        <ul className="selectable m-0 list-disc pl-4">
          {message.details.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}
    </Callout>
  );
}
