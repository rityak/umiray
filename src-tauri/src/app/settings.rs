//! Настройки приложения: выбор пользователя, переживающий перезапуск (D-024).
//!
//! Настройки — это **клиент**: оформление, автозапуск, расписание обновлений, память
//! о выбранном узле. Всё, что касается ядра, живёт в его конфиге и правится там же —
//! режим и DNS уехали туда (D-052). Данные о серверах приходят из источников.
//!
//! Файл версионирован: схема будет меняться, и по `version` видно, что делать со старым файлом.

use serde::{Deserialize, Serialize};

use crate::config::direction::Direction;
use crate::config::mode::Mode;
use crate::core::EngineId;
use crate::error::{AppError, Result};
use crate::paths::Paths;

/// Версия схемы. Растёт, когда меняется **смысл** существующего поля. Добавление нового поля
/// с `#[serde(default)]` версию не двигает: старый файл читается как есть.
pub const VERSION: u32 = 2;

/// Оформление окна. Тема — это фоновая картинка плюс палитра, больше в ней ничего нет,
/// поэтому в настройках она одно поле, а не набор цветов: цвета знает фронтенд.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Midnight,
    Green,
    Purple,
}

/// Как открывается окно при запуске клиента (D-088, D-129).
///
/// Отдельно от автозапуска: «поднимайся с Windows» и «показывайся при этом» — разные
/// вопросы, и на автозапуске ответ на второй обычно «нет».
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Launch {
    /// Тихо, если клиента подняла система, и окном, если человек. Умолчание: пока
    /// автозапуск выключен, ничем не отличается от `Window` (D-129).
    #[default]
    Smart,
    Window,
    /// Тихо: окна нет, есть только значок в трее.
    Tray,
}

/// Декоративный слой включён по умолчанию (D-045). Выключение — осознанный выбор
/// пользователя, а `#[serde(default)]` на голом `bool` дал бы `false` и погасил бы
/// эффекты всем, у кого файл настроек лежит с прошлой сборки.
fn enabled() -> bool {
    true
}

/// Размытие сцены по умолчанию, пиксели (D-132). Резкий пиксель-арт спорил с панелями
/// за внимание, а трёх пикселей хватает, чтобы город остался городом.
fn soft() -> u8 {
    3
}

/// Предел размытия: дальше город становится пятнами цвета. Больше не записываем, что бы
/// ни прислало окно.
pub const MAX_BLUR: u8 = 16;

/// Когда обновлять подписки.
///
/// Два поля, а не одно: «только при запуске» — самостоятельное поведение, а не период.
/// Выпадающий список в окне — это готовые пары значений, а не отдельная сущность.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refresh {
    /// Обновлять при запуске клиента, если подписка успела устареть.
    pub on_start: bool,
    /// Период в минутах. Ноль — по времени не обновлять.
    pub every_minutes: u32,
}

impl Default for Refresh {
    /// Раз в сутки и при запуске: панели сами подсказывают именно такой интервал
    /// (`profile-update-interval: 1` в ответе), а лишний трафик к провайдеру ни к чему.
    fn default() -> Self {
        Self {
            on_start: true,
            every_minutes: 24 * 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    #[serde(default)]
    pub refresh: Refresh,
    /// Куда идёт трафик (D-056). В `Patch` этого поля нет намеренно: переключение
    /// направления меняет не только настройку — оно наводит псевдоним у живого ядра
    /// и меняет набор конфигов, поэтому у него своя команда.
    #[serde(default)]
    pub direction: Direction,
    /// Ваш набор конфигов, активный в направлении `rules` (D-056). Пусто — своего набора
    /// ещё нет: он заведётся при первой правке «Групп» или «Маршрутизации».
    #[serde(default)]
    pub preset: Option<String>,
    /// Какой узел выбран для направления `manual`. Помним сами: `profile.store-selected`
    /// у ядра выбор не удержал через перезапуск (замерено, S-012).
    ///
    /// В `Patch` этого поля нет намеренно: выбор ставится не из формы настроек, а нажатием
    /// на узел, и проходит через ядро — там же он и проверяется на существование.
    #[serde(default)]
    pub selected: Option<String>,
    #[serde(default)]
    pub theme: Theme,
    /// Пиксельная сцена фоном. Отдельно от дождя (D-093): картинка — это вид, дождь —
    /// движение, и выключают их по разным поводам. Включена по умолчанию, как и дождь,
    /// поэтому свой умолчатель: голый `#[serde(default)]` дал бы `false` и погасил бы
    /// сцену всем, у кого файл настроек лежит с прошлой сборки.
    #[serde(default = "enabled")]
    pub scene: bool,
    /// Размытие сцены, пиксели (D-132). Дождь не трогает — он своим слоем поверх.
    #[serde(default = "soft")]
    pub scene_blur: u8,
    /// Дождь. Выключается ещё и системным `prefers-reduced-motion`, но это решает
    /// окно: настройка на диске означает «пользователь так захотел», а не «так решила ОС».
    #[serde(default = "enabled")]
    pub effects: bool,
    /// Режим «на людях»: адреса серверов и имена подписок закрыты точками. Нужен ровно
    /// затем, чтобы клиент можно было показать на экране — в трансляции, на скриншоте,
    /// в общей встрече, — не выдав, к какому серверу человек подключён.
    ///
    /// Хранится на диске, а не в памяти окна: включённый и забытый после перезапуска,
    /// он выдал бы адрес именно тогда, когда его и не должны были увидеть.
    #[serde(default)]
    pub private: bool,
    /// Прописывать ли прокси в систему при подключении (D-047). Выключено по умолчанию:
    /// это запись в реестр, и включать её молча за пользователя нельзя.
    #[serde(default)]
    pub system_proxy: bool,
    /// Что стояло в реестре до нас. Не настройка, а служебная память: в `Patch` её нет,
    /// пишет её только код включения.
    #[serde(default)]
    pub proxy_backup: Option<crate::system::sysproxy::Backup>,
    /// Подключаться сразу при запуске клиента (D-088). Что именно поднимать, помнить
    /// не нужно: направление лежит в этих же настройках, а режим перехвата — в конфиге
    /// ядра (D-060), и оба переживают перезапуск сами.
    #[serde(default)]
    pub auto_connect: bool,
    /// Показывать ли окно при запуске (D-088).
    #[serde(default)]
    pub launch: Launch,
    /// Предлагать ли «всегда от администратора», когда клиент запущен с правами, а задачи
    /// ещё нет (D-087). Отказ помним: предложение, которое возвращается каждый запуск, —
    /// это не предложение, а требование.
    #[serde(default = "enabled")]
    pub admin_offer: bool,
    /// Запирать ли выход мимо туннеля, пока работает TUN (D-073). Выключено по умолчанию:
    /// правило переживает падение клиента, и отнимать сеть у того, кто этого не просил,
    /// нельзя.
    #[serde(default)]
    pub kill_switch: bool,
    /// Что стояло в брандмауэре до нас. Служебная память, как и `proxy_backup`: её наличие
    /// и означает «запрет наш», в том числе после падения.
    #[serde(default)]
    pub kill_switch_backup: Option<crate::system::killswitch::Backup>,
    #[serde(default)]
    /// Какое ядро показывают разделы и поднимет кнопка питания (D-154).
    pub engine: EngineId,
}

/// Что меняем в настройках. Ровно одна команда на все опции: с ростом их числа отдельная
/// команда на каждую превратилась бы в десяток почти одинаковых.
///
/// Не `Settings` целиком: `version` остаётся на стороне Rust, а незнакомое поле serde
/// отвергнет на границе — недоверенный вебвью не выставит того, чего мы не ожидаем.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Patch {
    pub refresh: Option<Refresh>,
    pub engine: Option<EngineId>,
    pub theme: Option<Theme>,
    pub scene: Option<bool>,
    pub scene_blur: Option<u8>,
    pub effects: Option<bool>,
    pub private: Option<bool>,
    pub system_proxy: Option<bool>,
    pub kill_switch: Option<bool>,
    pub auto_connect: Option<bool>,
    pub launch: Option<Launch>,
    pub admin_offer: Option<bool>,
}

impl Patch {
    /// Накладывает только то, что пришло: отсутствующее поле остаётся прежним.
    pub fn apply(self, settings: &mut Settings) {
        if let Some(engine) = self.engine {
            settings.engine = engine;
        }
        if let Some(refresh) = self.refresh {
            settings.refresh = refresh;
        }
        if let Some(theme) = self.theme {
            settings.theme = theme;
        }
        if let Some(scene) = self.scene {
            settings.scene = scene;
        }
        if let Some(blur) = self.scene_blur {
            settings.scene_blur = blur.min(MAX_BLUR);
        }
        if let Some(effects) = self.effects {
            settings.effects = effects;
        }
        if let Some(private) = self.private {
            settings.private = private;
        }
        if let Some(system_proxy) = self.system_proxy {
            settings.system_proxy = system_proxy;
        }
        if let Some(kill_switch) = self.kill_switch {
            settings.kill_switch = kill_switch;
        }
        if let Some(auto_connect) = self.auto_connect {
            settings.auto_connect = auto_connect;
        }
        if let Some(launch) = self.launch {
            settings.launch = launch;
        }
        if let Some(admin_offer) = self.admin_offer {
            settings.admin_offer = admin_offer;
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            direction: Direction::default(),
            preset: None,
            refresh: Refresh::default(),
            selected: None,
            theme: Theme::default(),
            scene: true,
            scene_blur: soft(),
            effects: true,
            private: false,
            system_proxy: false,
            proxy_backup: None,
            kill_switch: false,
            kill_switch_backup: None,
            auto_connect: false,
            launch: Launch::default(),
            admin_offer: true,
            engine: EngineId::default(),
        }
    }
}

impl SettingsStore {
    /// Читает настройки с диска. **Не падает никогда**: настройки — это поведение, и битый файл
    /// должен стоить сброса к умолчаниям, а не неработающего приложения.
    pub fn load() -> Settings {
        match std::fs::read_to_string(Paths::settings()) {
            Ok(text) => decode(&text),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(settings: &Settings) -> Result<()> {
        Paths::ensure_root()?;
        let text = serde_json::to_string_pretty(settings)
            .map_err(|e| AppError::Io(format!("Не удалось записать настройки: {e}")))?;
        Ok(crate::atomic::AtomicFile::write(Paths::settings(), text)?)
    }

    pub fn load_v1() -> Option<V1> {
        let text = std::fs::read_to_string(Paths::settings()).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        if value.get("version")?.as_u64()? != 1 {
            return None;
        }
        serde_json::from_value(value).ok()
    }
}

/// Хранилище настроек клиента (D-155): единственный, кто знает, где они лежат на диске.
///
/// В памяти, а не чтением файла на каждый вызов: статус опрашивается раз в 1.5 с,
/// а меняются настройки только по действию пользователя. Диск читается один раз при старте.
pub struct SettingsStore(std::sync::Mutex<Settings>);

impl SettingsStore {
    pub fn open() -> Self {
        Self(std::sync::Mutex::new(SettingsStore::load()))
    }

    pub fn get(&self) -> Settings {
        self.0.lock().unwrap().clone()
    }

    pub fn patch(&self, patch: Patch) -> Result<()> {
        self.update(|settings| patch.apply(settings))
    }

    /// Перечитать с диска. Нужно после сброса: файл переписали мимо `update`, и копия
    /// в памяти иначе осталась бы от прошлой жизни.
    pub fn reload(&self) {
        *self.0.lock().unwrap() = SettingsStore::load();
    }

    /// Сначала диск, потом память: если запись не удалась, они не должны разъехаться.
    ///
    /// Замок держится и на время записи: импорт подписки — асинхронная команда, и переключение
    /// режима во время неё выполнится параллельно. Читать-менять-писать без замка означало бы
    /// потерянное обновление. Запись короткая, дожидаться её не жалко.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<()> {
        let mut current = self.0.lock().unwrap();
        let mut next = current.clone();
        change(&mut next);
        SettingsStore::save(&next)?;
        *current = next;
        Ok(())
    }
}

/// Настройки версии 1: адрес подписки лежал здесь, пока не было профилей.
/// Читает только миграция — обычному чтению файл прошлой версии виден как «сбросить».
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V1 {
    pub mode: Mode,
    #[serde(default)]
    pub subscription: Option<String>,
}

/// Разбор отделён от чтения файла, чтобы правила проверялись обычным `cargo test`,
/// не трогая настоящий `%LOCALAPPDATA%` пользователя.
fn decode(text: &str) -> Settings {
    match serde_json::from_str::<Settings>(text) {
        Ok(settings) if settings.version == VERSION => settings,
        // Чужая версия (файл от другой сборки) или мусор: что означают поля — неизвестно,
        // а угадать хуже, чем начать с умолчаний.
        _ => Settings::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_carry_the_version() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"version\":2"), "{json}");
    }

    #[test]
    fn saved_settings_read_back_the_same() {
        let settings = Settings {
            version: VERSION,
            direction: Direction::Manual,
            preset: Some("0123456789abcdef".into()),
            refresh: Refresh {
                on_start: false,
                every_minutes: 30,
            },
            selected: Some("Sweden 0".into()),
            theme: Theme::Purple,
            scene: false,
            scene_blur: 7,
            effects: false,
            private: true,
            system_proxy: true,
            proxy_backup: None,
            kill_switch: false,
            kill_switch_backup: None,
            auto_connect: true,
            launch: Launch::Tray,
            admin_offer: false,
            engine: EngineId::Qd,
        };
        let json = serde_json::to_string_pretty(&settings).unwrap();
        assert_eq!(decode(&json), settings);
    }

    /// Файл прошлой сборки не знает про `effects`. Голый `#[serde(default)]` дал бы `false`
    /// и погасил бы фон всем, кто обновился, — поэтому у поля свой умолчатель.
    #[test]
    fn a_file_without_effects_reads_as_enabled() {
        let old = r#"{"version":2}"#;
        let settings = decode(old);
        assert!(settings.effects, "эффекты должны включаться по умолчанию");
        assert!(settings.scene, "сцена должна включаться по умолчанию");
        assert_eq!(
            settings.scene_blur, 3,
            "сцена слегка размыта по умолчанию (D-132)"
        );
        assert_eq!(settings.theme, Theme::Midnight);
        assert!(
            !settings.auto_connect,
            "молча подключаться за пользователя нельзя"
        );
        assert_eq!(
            settings.launch,
            Launch::Smart,
            "с Windows тихо, ярлыком — окном (D-129)"
        );
        assert!(
            settings.admin_offer,
            "файл прошлой сборки об отказе не знает"
        );
    }

    /// Файл прошлой сборки помнит режим и пресет DNS. Они уехали в конфиг ядра (D-052),
    /// и их присутствие не должно стоить пользователю темы и расписания: лишние поля
    /// игнорируются, версия остаётся прежней.
    #[test]
    fn fields_that_moved_into_the_core_config_do_not_break_an_older_file() {
        let old = r#"{"version":2,"mode":"tun","dns":"adguard","theme":"purple"}"#;
        assert_eq!(decode(old).theme, Theme::Purple);
    }

    #[test]
    fn unusable_files_fall_back_to_defaults_instead_of_failing() {
        let default = Settings::default();
        // Версия из другой сборки: поля называются так же, но значат неизвестно что.
        assert_eq!(decode(r#"{"version":99,"theme":"green"}"#), default);
        // Файл прошлой версии: его разбирает миграция, обычное чтение обязано отступить.
        assert_eq!(decode(r#"{"version":1,"theme":"green"}"#), default);
        // Неизвестная тема отвергается на границе, а не превращается в «что-то».
        assert_eq!(decode(r#"{"version":2,"theme":"неон"}"#), default);
        assert_eq!(decode("не json вовсе"), default);
        assert_eq!(decode(""), default);
    }

    #[test]
    fn a_patch_touches_only_what_it_carries() {
        let mut settings = Settings::default();
        let patch: Patch = serde_json::from_str(r#"{"theme":"purple"}"#).unwrap();
        patch.apply(&mut settings);

        assert_eq!(settings.theme, Theme::Purple);
        assert!(settings.effects, "чего не прислали, то не тронуто");
        assert_eq!(settings.refresh, Refresh::default());
    }

    /// Размытие приходит из вебвью, и число оттуда — недоверенное: больше предела не пишем,
    /// а отрицательное не проходит границу вовсе (D-132).
    #[test]
    fn scene_blur_is_clamped_at_the_boundary() {
        let mut settings = Settings::default();
        let patch: Patch = serde_json::from_str(r#"{"sceneBlur":200}"#).unwrap();
        patch.apply(&mut settings);
        assert_eq!(settings.scene_blur, MAX_BLUR);
        assert!(serde_json::from_str::<Patch>(r#"{"sceneBlur":-1}"#).is_err());
        assert!(serde_json::from_str::<Patch>(r#"{"sceneBlur":1.5}"#).is_err());
    }

    /// Незнакомое поле отвергается на границе, а не молча игнорируется: иначе опечатка
    /// в имени настройки выглядела бы как «сохранилось».
    #[test]
    fn an_unknown_field_is_refused_at_the_boundary() {
        assert!(serde_json::from_str::<Patch>(r#"{"нетакой":1}"#).is_err());
        assert!(
            serde_json::from_str::<Patch>(r#"{"mode":"tun"}"#).is_err(),
            "режим ставится не настройкой, а записью в конфиг ядра"
        );
        assert!(serde_json::from_str::<Patch>(r#"{"version":3}"#).is_err());
        assert!(
            serde_json::from_str::<Patch>(r#"{"selected":"Sweden 0"}"#).is_err(),
            "выбор узла ставится не через настройки, а нажатием на узел"
        );
        assert!(
            serde_json::from_str::<Patch>(r#"{"direction":"auto"}"#).is_err(),
            "направление меняет не форма настроек, а своя команда: у него есть последствия"
        );
    }

    #[test]
    fn a_missing_optional_field_is_not_a_broken_file() {
        // Файл, записанный сборкой без поля расписания, должен читаться, а не сбрасываться.
        //
        // Написанное здесь значение — не украшение, а вся суть проверки: `decode` при
        // неудаче возвращает **умолчания целиком**, поэтому сравнивать умолчание
        // с умолчанием бессмысленно — так проходит и разбор, и его провал. Отличить их
        // можно только по полю, значение которого от умолчания отличается.
        let settings = decode(r#"{"version":2,"theme":"purple"}"#);
        assert_eq!(
            settings.theme,
            Theme::Purple,
            "файл не разобрался: у какого-то поля нет `serde(default)`, \
             и оно унесло с собой все настройки пользователя разом"
        );
        assert_eq!(
            settings.refresh,
            Refresh::default(),
            "поле новее файла — берём умолчание"
        );
    }
}
