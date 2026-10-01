import { Cloud, FilePlus2, Rss, SquarePen } from "lucide-react";
import type { ReactElement } from "react";
import { Menu, MenuItem } from "rootik";
import { t } from "../i18n";

/// Способы добавить (D-120): ссылка ходит в сеть, файл читает диск, «вручную» собирает
/// узел по полям, WARP клиент выпускает сам (D-165). Одно меню на все кнопки «+» (D-160).
export type AddKind = "link" | "file" | "node" | "warp";

type Props = {
  /// Кнопка, которая открывает меню: «+» в шапке, в карточке узлов или в пустом списке.
  trigger: ReactElement;
  onPick: (kind: AddKind) => void;
};

export default function AddMenu({ trigger, onPick }: Props) {
  return (
    <Menu trigger={trigger} placement="bottom-end">
      <MenuItem
        icon={<Rss />}
        hint={t("https://, vless://, qd://…")}
        onSelect={() => onPick("link")}
      >
        {t("Subscription or link")}
      </MenuItem>
      <MenuItem
        icon={<FilePlus2 />}
        hint={t("WireGuard .conf, OpenVPN .ovpn or usque config.json")}
        onSelect={() => onPick("file")}
      >
        {t("From file")}
      </MenuItem>
      <MenuItem
        icon={<SquarePen />}
        hint={t("A node field by field")}
        onSelect={() => onPick("node")}
      >
        {t("Manually")}
      </MenuItem>
      <MenuItem
        icon={<Cloud />}
        hint={t("A free node: MASQUE or WireGuard")}
        onSelect={() => onPick("warp")}
      >
        Cloudflare WARP
      </MenuItem>
    </Menu>
  );
}
