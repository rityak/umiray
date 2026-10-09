<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" alt="umiray">
</p>

<h1 align="center">Umiray</h1>

<p align="center">
  Прокси-клиент для Windows и Linux на ядре <a href="https://github.com/MetaCubeX/mihomo">Mihomo</a>.<br>
  Tauri 2 · Rust · React · <a href="https://github.com/rityak/rootik">Rootik</a>
</p>

<p align="center">1.6.0 · <a href="README.md">English</a></p>

![Соединение](screenshots/connection.png)

## Возможности

- Подключение одним нажатием и отдельный выбор режима Proxy, System или TUN.
- Мастер при первом запуске: настройки ядра, подписка, режим и маршрут.
- Маршруты Direct, Auto, Manual или свои правила.
- Подписки, отдельные ссылки, файлы WireGuard и AmneziaWG, узлы вручную.
  Правки узлов сохраняются при обновлении подписки.
- Группы и маршрутизация формой или YAML, наборы правил и откат.
- Диагностика DNS, MTU, часов, утечек и скорости.
- Kill-switch, автозапуск и запуск от администратора через планировщик.
- Режим «на людях» скрывает адреса серверов и имена подписок для снимков и трансляций.
- Подписанные обновления клиента из GitHub Releases, с подтверждением и прогрессом загрузки.

## Установка и первый запуск

Скачайте из [Releases](https://github.com/rityak/umiray/releases/latest):

- **Windows x64** — `umiray_<версия>_x64-setup.exe`.
- **Debian, Ubuntu, Mint** (Debian 12+, Ubuntu 22.04+) — `umiray_<версия>_amd64.deb`:
  `sudo apt install ./umiray_<версия>_amd64.deb`.
- **Fedora** и другие системы с rpm — `umiray-<версия>-1.x86_64.rpm`:
  `sudo dnf install ./umiray-<версия>-1.x86_64.rpm`.
- **Arch** и производные — `umiray-bin` из AUR (`yay -S umiray-bin`) или `PKGBUILD`
  и `umiray.install` из релиза: `makepkg -si`.

Ядро mihomo клиент скачает с официальной страницы релизов при первом запуске.
Добавьте подписку, ссылку на прокси или файл конфигурации, выберите узел и нажмите подключение.
Proxy предоставляет локальный прокси для приложений; System включает системный прокси;
TUN перехватывает трафик через виртуальный интерфейс. Серверы в комплект не входят.

Для TUN нужны дополнительные права. На Windows клиент может создать задачу в планировщике,
чтобы запускаться с правами без подтверждения UAC каждый раз. На Linux клиент работает
от вас, а права на сеть через polkit получает только ядро; в своём сеансе пароль не спрашивается.

Данные хранятся в `%LOCALAPPDATA%\umiray` на Windows и в `~/.local/share/umiray` на Linux.

Если установлена русская раскладка клавиатуры, интерфейс выбирает русский язык.
В остальных случаях — английский.

## Umiray Core / VOLT

[umiray-core](https://github.com/rityak/umiray-core) содержит дополнительный Windows-механизм
обработки трафика VOLT. Он меняет разделение и порядок TCP-пакетов, может добавлять ложные
пакеты, сохраняя реальные данные.

Настройки → Umiray Settings → Anti-DPI → VOLT: прямые соединения и трафик VPN
настраиваются независимо. При включении клиент скачивает бинарники из релизов core;
для перехвата нужны права администратора. DIRECT-VOLT всегда использует Relay,
а DIRECT-AUTO сначала пробует обычное TLS-соединение. VOLT не предоставляет VPN-сервис.

## Зависимости и сборка

Нужны Node.js 22+, Rust stable и [зависимости Tauri](https://v2.tauri.app/start/prerequisites/):
Microsoft C++ Build Tools и WebView2 на Windows; WebKitGTK 4.1 и перечисленные пакеты
разработки на Linux. Зависимости JavaScript и Rust устанавливаются через npm и Cargo.

```sh
npm ci
npm run tauri dev
```

Сборка бинарника без установщика:

```sh
npm run tauri build -- --no-bundle
```

Бинарник: `src-tauri/target/release/umiray.exe` (`umiray` на Linux). Пакеты Linux:
`npx tauri build --bundles deb,rpm`; собирайте на Ubuntu 22.04 — тогда они встанут и на старые системы.

Проверки: `npm run lint`, `npm test`, `npx tsc --noEmit` и `cargo test` в `src-tauri`.
`npm run dev` открывает интерфейс в браузере с демонстрационными данными.

Ошибки — в [Issues](https://github.com/rityak/umiray/issues).
Код и сборки распространяются как есть; пользователь отвечает за соблюдение законов
и условий провайдеров. Авторы не несут ответственности за использование.
