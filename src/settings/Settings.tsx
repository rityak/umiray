import { X } from "lucide-react";
import type { ReactNode } from "react";
import { AppearanceSettings, IconButton, PageHeader, ruTranslate } from "rootik";
import { getLang, t } from "../i18n";
import { APPEARANCE_RU } from "../locales/appearance-ru";
import Scroll from "../shell/Scroll";

/// The kit translates first; ours only fills what it lacks (ROOTIK §3).
const translateRu = (key: string, fallback: ReactNode) =>
  ruTranslate(key, APPEARANCE_RU[key] ?? fallback);

/** Window appearance — rootik's own form in full (D-142): theme, material, layout, density. */
export default function Settings({ onClose }: { onClose: () => void }) {
  return (
    <>
      <PageHeader
        title={t("Appearance")}
        description={t(
          "rootik kit settings. Kept in this window, never sent to the client or core.",
        )}
        actions={<IconButton variant="ghost" icon={<X />} label={t("Close")} onClick={onClose} />}
      />
      {/* The form scrolls on its own: the window page never scrolls. */}
      <Scroll>
        <AppearanceSettings resettable t={getLang() === "ru" ? translateRu : undefined} />
      </Scroll>
    </>
  );
}
