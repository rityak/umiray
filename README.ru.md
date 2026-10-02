<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" alt="umiray">
</p>

<h1 align="center">Umiray</h1>

<p align="center">
  VPN-клиент для Windows на ядре <a href="https://github.com/MetaCubeX/mihomo">Mihomo</a>.<br>
  Tauri 2 · Rust · React · <a href="https://github.com/rityak/rootik">Rootik</a>
</p>

<p align="center">1.3.2 · <a href="README.md">English</a></p>

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

Скачайте установщик Windows x64 `*-setup.exe` из [Releases](https://github.com/rityak/umiray/releases/latest).
Ядро mihomo клиент скачает с официальной страницы релизов при первом запуске.

Для TUN нужны права администратора. Клиент может создать задачу в планировщике,
чтобы запускаться с правами без подтверждения UAC каждый раз.

Данные хранятся в `%LOCALAPPDATA%\umiray`.

Если установлена русская раскладка клавиатуры, интерфейс выбирает русский язык.
В остальных случаях — английский.

## Сборка

Нужны Node.js 22+, Rust и [зависимости Tauri](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run tauri dev
npm run tauri build -- --no-bundle
```

Бинарник: `src-tauri/target/release/umiray.exe`.

Проверки: `npm run lint`, `npm test`, `npx tsc --noEmit` и `cargo test` в `src-tauri`.
`npm run dev` открывает интерфейс в браузере с демонстрационными данными.

## Статус

Версия 1.0 для Windows x64. Ошибки — в [Issues](https://github.com/rityak/umiray/issues).

Обновления проверяются при запуске и в «Настройки» → Umiray Settings → «Обслуживание».
Установка проверяет подпись, отключает VPN и сохраняет пользовательские данные.

Выпуск новой версии: [RELEASING.md](RELEASING.md).
