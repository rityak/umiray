import { Component, type ReactNode } from "react";
import { Button, Callout } from "rootik";
import { t } from "../i18n";

type Props = { children: ReactNode };
type State = { error: Error | null };

/// Исключение при отрисовке раздела остаётся в разделе: без этого React снимает всё дерево,
/// и окно уходит в голый фон без шапки и dock (B-054). Сбрасывается сменой раздела —
/// у панели раздела свой `key` — или кнопкой.
///
/// Класс, а не функция: ловить исключения отрисовки React умеет только через
/// `getDerivedStateFromError`, хука для этого нет.
export default class SectionGuard extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: unknown): State {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <Callout
        live
        tone="danger"
        title={t("This section failed to open")}
        actions={
          <Button size="sm" onClick={() => this.setState({ error: null })}>
            {t("Try again")}
          </Button>
        }
      >
        <p className="selectable m-0">{error.message}</p>
      </Callout>
    );
  }
}
