#!/bin/sh
# umiray (D-173): запрет kill switch, оставшийся от упавшего клиента, пережил бы удаление
# пакета и оставил машину без сети — снять его больше было бы некому. Только при удалении:
# deb зовёт prerm с «remove», rpm — preun с 0; при обновлении запрет снимает сам клиент.
case "$1" in
  remove|purge|0)
    nft delete table inet umiray 2>/dev/null || true
    ;;
esac
exit 0
