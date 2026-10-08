<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" alt="umiray">
</p>

<h1 align="center">Umiray</h1>

<p align="center">
  VPN-клиент для Windows и Linux на ядре <a href="https://github.com/MetaCubeX/mihomo">Mihomo</a>.<br>
  Tauri 2 · Rust · React · <a href="https://github.com/rityak/rootik">Rootik</a>
</p>

<p align="center">1.5.0 · <a href="README.md">English</a></p>

![Соединение](screenshots/connection.png)

## Возможности

- Подключение одним нажатием и отдельный выбор режима Proxy, System или TUN.
- Маршруты Direct, Auto, Manual или свои правила.
- Подписки, отдельные ссылки, файлы WireGuard и AmneziaWG, узлы вручную.
  Правки узлов сохраняются при обновлении подписки.
- Группы и маршрутизация формой или YAML, наборы правил и откат.
- Диагностика DNS, MTU, часов, утечек и скорости.
- Kill-switch, автозапуск и запуск от администратора через планировщик.
- Режим «на людях» скрывает адреса серверов и имена подписок для снимков и трансляций.
- Подписанные обновления клиента из GitHub Releases, с подтверждением и прогрессом загрузки.

## Скриншоты

| | |
|---|---|
| ![Источники](screenshots/sources.png) | ![Группы](screenshots/groups.png) |
| ![Маршрутизация](screenshots/routing.png) | ![Настройки](screenshots/settings.png) |
| ![Инструменты](screenshots/tools.png) | ![Логи](screenshots/logs.png) |

На снимках демонстрационные данные и английский интерфейс. Язык можно выбрать в
«Настройки» → Umiray Settings → «Язык интерфейса»: автоматически, English или Русский.

## Запуск

Скачайте из [Releases](https://github.com/rityak/umiray/releases/latest):

- **Windows x64** — `umiray_<версия>_x64-setup.exe`.
- **Debian, Ubuntu, Mint** (Debian 12+, Ubuntu 22.04+) — `umiray_<версия>_amd64.deb`:
  `sudo apt install ./umiray_<версия>_amd64.deb`.
- **Fedora** и другие системы с rpm — `umiray-<версия>-1.x86_64.rpm`:
  `sudo dnf install ./umiray-<версия>-1.x86_64.rpm`.
- **Arch** и производные — `umiray-bin` из AUR (`yay -S umiray-bin`) или `PKGBUILD`
  и `umiray.install` из релиза: `makepkg -si`.

Ядро mihomo клиент скачает с официальной страницы релизов при первом запуске.

Для TUN нужны дополнительные права. На Windows клиент может создать задачу в планировщике,
чтобы запускаться с правами без подтверждения UAC каждый раз. На Linux клиент работает
от вас, а права на сеть через polkit получает только ядро; в своём сеансе пароль не спрашивается.

Данные хранятся в `%LOCALAPPDATA%\umiray` на Windows и в `~/.local/share/umiray` на Linux.

Если установлена русская раскладка клавиатуры, интерфейс выбирает русский язык.
В остальных случаях — английский.

## Сборка

Нужны Node.js 22+, Rust и [зависимости Tauri](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run tauri dev
npm run tauri build -- --no-bundle
```

Бинарник: `src-tauri/target/release/umiray.exe` (`umiray` на Linux). Пакеты Linux:
`npx tauri build --bundles deb,rpm`; собирайте на Ubuntu 22.04 — тогда они встанут и на старые системы.

Проверки: `npm run lint`, `npm test`, `npx tsc --noEmit` и `cargo test` в `src-tauri`.
`npm run dev` открывает интерфейс в браузере с демонстрационными данными.

## Статус

Windows x64 и Linux x86_64. Ошибки — в [Issues](https://github.com/rityak/umiray/issues).

Обновления проверяются при запуске и в «Настройки» → Umiray Settings → «Обслуживание».
Установка проверяет подпись, отключает VPN и сохраняет пользовательские данные.
На Arch обновления ставит менеджер пакетов; клиент только сообщает, что вышла новая версия.

Выпуск новой версии: [RELEASING.md](RELEASING.md).
