# Выпуск umiray

Репозиторий: https://github.com/rityak/umiray. Сборки — Windows x64 (NSIS) и Linux x86_64
(deb и rpm, собираются на Ubuntu 22.04; для Arch — `PKGBUILD` с sha256 этого deb).
Тег `vX.Y.Z` запускает проверки и сборку; завершённый релиз публикуется автоматически.
Клиенты читают `releases/latest/download/latest.json` из этого репозитория: в нём
`windows-x86_64`, `linux-x86_64-deb` и `linux-x86_64-rpm` (последние два дописывает задание
`publish` через `tools/linux-release.mjs`).

### AUR (по желанию)

Чтобы `umiray-bin` в AUR обновлялся сам с каждым выпуском:

1. Заведите учётную запись на https://aur.archlinux.org и добавьте в неё публичный SSH-ключ
   (отдельный, только для AUR).
2. Добавьте закрытый ключ как Actions secret `AUR_SSH_PRIVATE_KEY`.

Первый выпуск с секретом создаст пакет `umiray-bin`; без секрета шаг пропускается, а `PKGBUILD`
и `umiray.install` всё равно лежат в ассетах релиза для `makepkg -si`.

## Один раз перед первым тегом

Публичный ключ уже в `src-tauri/tauri.conf.json`. Закрытый ключ создан локально
в `.release/updater.key` и исключён из Git. Сохраните резервную копию вне проекта.
Это ключ всей линии обновлений: новый ключ не примут уже установленные клиенты.

Добавьте содержимое `.release/updater.key` как Actions secret `TAURI_SIGNING_PRIVATE_KEY`:
[Settings → Secrets and variables → Actions](https://github.com/rityak/umiray/settings/secrets/actions).
Либо, после `gh auth login`:

```powershell
npm run updater:setup -- --github
```

Созданный ключ без пароля; `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` не требуется.
`GITHUB_TOKEN` предоставляет Actions. Отдельный сервер обновлений не нужен.
В репозитории должны быть включены Actions; разрешение публикации задано в workflow.

## Выпуск 1.0.1

Из корня проекта в PowerShell, после добавления секрета:

```powershell
npm run release:check
git add .
git commit -m "fix: restore icons, reorganize settings and isolate dev" `
  -m "Restore the app logo in GitHub builds. Move client version and update controls into the Client section. Add English rule-set titles. Separate stable/dev data, processes and autostart with safe migration of existing files. Publish release notes from CHANGELOG."
git push -u origin HEAD
git tag v1.0.1
git push origin v1.0.1
```

В этой рабочей копии `origin` уже настроен на `https://github.com/rityak/umiray.git`.
В другой копии проверьте `git remote -v`; если remote отсутствует, добавьте его через
`git remote add origin https://github.com/rityak/umiray.git`.
Workflow сверяет тег, `package.json`, оба lock-файла и `Cargo.toml`.
Сначала создаёт draft с установщиком, `.sig` и `latest.json`; успешная сборка публикует его.
Текст страницы релиза — английский раздел текущей версии из `CHANGELOG.md`, вместе с подзаголовками
и инструкцией установки. `node tools/release-check.mjs --notes` показывает этот текст
перед публикацией. Без непустого раздела текущей версии сборка не начинается.
Устанавливается `umiray_1.0.1_x64-setup.exe`, не portable exe.
Подпись обновления Tauri не является сертификатом Windows Authenticode.

## Следующие версии

1. `npm version patch --no-git-tag-version`.
2. Та же версия в `src-tauri/Cargo.toml`; `cargo check --manifest-path src-tauri/Cargo.toml`
   обновит `Cargo.lock`.
3. Обновить CHANGELOG и версии в README; выполнить `npm run release:check`.
4. Закоммитить, отправить код, создать и отправить тег `v<номер из package.json>`.

Нужен больший номер версии. Повторный тег или замена файлов старого релиза — не обновление.
Этот workflow выпускает стабильные версии; beta-теги отклоняются проверкой.

## Проверка опубликованного обновления

Дождитесь успешного Release в [Actions](https://github.com/rityak/umiray/actions).
В Releases должны быть установщик, подпись и `latest.json`: номер выпуска,
непустая подпись, URL установщика именно этого тега.

Установите 1.0.0, опубликуйте 1.0.1. В старом клиенте проверьте обновление,
подтвердите установку: после перезапуска версия должна стать 1.0.1,
источники, настройки и HWID — сохраниться. Повторите с System и TUN + kill-switch:
сетевые настройки должны восстановиться до запуска установщика.
Потеря сети или неверная подпись при скачивании должны оставить подключение работающим.

До первой публикации в пустом репозитории `latest.json` отсутствует: ручная проверка
сообщит об ошибке; фоновая проверка не мешает открытию окна.

## Локальная сборка и очистка

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = (Resolve-Path .release/updater.key).Path
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
npm run tauri build -- --bundles nsis -- --locked
```

Установщик и подпись: `src-tauri/target/release/bundle/nsis/`.
Для разработки без подписи: `npm run tauri build -- --no-bundle`.
`cargo clean --manifest-path src-tauri/Cargo.toml` удаляет кэш и все сборки Rust;
сохраните нужный установщик отдельно. Следующая сборка будет полной.
Пользовательские данные лежат вне проекта и этой командой не затрагиваются.

Проверка интерфейса обновления без установки: при работающем `npm run dev` откройте
его в отдельном браузере с CDP-портом 9233 и выполните `node tools/update-check.mjs`.
Тест отказывается работать в живом Tauri-клиенте; проверяет подтверждение, несохранённые
формы и YAML, прогресс, блокировку Escape и повтор после ошибки загрузки.

## Stable и dev

Stable: `%LOCALAPPDATA%\umiray`, `umiray.exe`, `mihomo.exe`. Debug:
`%LOCALAPPDATA%\umiray-dev`, `umiray-dev.exe`, `mihomo-dev.exe`.
Прежние `umiray-client` и `umiray-client-dev` копируются при первом запуске;
существующие файлы нового каталога и прежние папки сохраняются.
Каталог выбирается профилем сборки, независимо от расположения exe. При установке
в Program Files конфиги всё равно остаются в LocalAppData.

`npm run tauri dev` собирает `src-tauri/target/debug/umiray-dev.exe`.
Если нужен прямой запуск с CDP: `npm run dev` отдельно, затем
`src-tauri\target\debug\umiray-dev.exe --scheduled`.
Cargo называет бинарь `umiray-dev`; `tauri build` переименовывает release в `umiray.exe`.
Задачи автозапуска, Run-записи, mutex, WebView и имена firewall-правил раздельные.
Debug не устанавливает обновления stable. Новый dev-конфиг использует порт 3091;
заданный пользователем порт не переписывается. Два Proxy работают одновременно,
System/TUN и глобальная политика firewall остаются общими для Windows.

После обеих сборок при работающем `npm run dev`: `node tools/dev-check.mjs`.
Проверка оставляет работающий stable на месте; закрыть нужно только существующий dev.
Порт тестового dev 3091 должен быть свободен; если stable не открыт, нужен и свободный 3090.
Создаёт временные данные без подписок, проверяет миграцию, оба языка, кнопки версии,
запуск dev, повторный запуск и сохранность stable после остановки dev. Если stable
не открыт, поднимает обе тестовые сборки и проверяет их одновременную работу.
Процессы и временные данные убирает. `UI_CHECK_CORE` задаёт путь к существующему ядру.
`tools\stop-client.cmd dev` завершает только dev; без аргумента — только stable.
