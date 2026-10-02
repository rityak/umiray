import { Cloud, FilePlus2, Rss, SquarePen } from "lucide-react";
import { cloneElement, type ReactElement } from "react";
import { Menu, MenuItem } from "rootik";
import { t } from "../i18n";

/// Способы добавить (D-120): ссылка ходит в сеть, файл читает диск, «вручную» собирает
/// узел по полям, WARP клиент выпускает сам (D-165). Одно меню на все кнопки «+» (D-160).
export type AddKind = "link" | "file" | "node" | "warp";

type Props = {
  /// Кнопка, которая открывает меню: «+» в шапке, в карточке узлов или в пустом списке.
  trigger: ReactElement<{ onClick?: () => void }>;
  /// Что предлагать — его выбирает ядро на виде (`ENGINES[...].adds`): qd берёт только ссылку.
  /// Не передано — всё: виды mihomo.
  kinds?: AddKind[];
  onPick: (kind: AddKind) => void;
};

const ALL: AddKind[] = ["link", "file", "node", "warp"];

export default function AddMenu({ trigger, kinds = ALL, onPick }: Props) {
  // Одно — меню из одного пункта лишнее: кнопка открывает его сразу.
  if (kinds.length === 1) return cloneElement(trigger, { onClick: () => onPick(kinds[0]) });
  const has = (kind: AddKind) => kinds.includes(kind);
  return (
    <Menu trigger={trigger} placement="bottom-end">
      {has("link") && (
        <MenuItem
          icon={<Rss />}
          hint={t("https://, vless://, qd://…")}
          onSelect={() => onPick("link")}
        >
          {t("Subscription or link")}
        </MenuItem>
      )}
      {has("file") && (
        <MenuItem
          icon={<FilePlus2 />}
          hint={t("WireGuard .conf, OpenVPN .ovpn or usque config.json")}
          onSelect={() => onPick("file")}
        >
          {t("From file")}
        </MenuItem>
      )}
      {has("node") && (
        <MenuItem
          icon={<SquarePen />}
          hint={t("A node field by field")}
          onSelect={() => onPick("node")}
        >
          {t("Manually")}
        </MenuItem>
      )}
      {has("warp") && (
        <MenuItem
          icon={<Cloud />}
          hint={t("A free node: MASQUE or WireGuard")}
          onSelect={() => onPick("warp")}
        >
          Cloudflare WARP
        </MenuItem>
      )}
    </Menu>
  );
}
