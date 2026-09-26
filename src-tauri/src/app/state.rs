//! Состояние приложения: то, что живёт дольше одной команды.
//!
//! Команды получают его целиком и не знают, как оно устроено внутри. Здесь же появится
//! клиент к `external-controller`, когда дойдёт очередь (D-007).

use std::sync::Mutex;

use crate::app::settings::{self, Settings};
use crate::config::direction::{self, Direction};
use crate::config::presets;
use crate::core::Supervisor;
use crate::error::Result;
use crate::nodes::ping;

pub struct AppState {
    pub supervisor: Supervisor,
    /// Один переход подключения за раз. Замок охватывает не только процесс, но и хуки
    /// вокруг него: иначе параллельные start/stop успевают рассинхронизировать ядро,
    /// системный прокси и брандмауэр.
    /// Все переходы соединения идут последовательно (D-134).
    transition: tokio::sync::Mutex<()>,
    /// Настройки держим в памяти, а не читаем файл на каждый вызов: статус опрашивается
    /// раз в 1.5 с, а меняются они только по действию пользователя.
    settings: Mutex<Settings>,
    /// Замеры до серверов (D-062). Живут дольше одной команды: собирает их отдельное
    /// нажатие, а показывает каждый опрос таблицы — держать их в окне значило бы терять
    /// при первом же переходе в другой раздел.
    pings: Mutex<ping::Table>,
    /// Что сейчас не так — по одной строке от каждого, кто это заметил (D-115).
    /// Пишут шаги фаз, читает статус, поэтому реестр здесь, а не в том или другом.
    pub notices: crate::app::notice::Notices,
}

impl AppState {
    /// Диск читается один раз при старте — дальше источник истины здесь.
    pub fn new() -> Self {
        Self {
            supervisor: Supervisor::new(),
            transition: tokio::sync::Mutex::new(()),
            settings: Mutex::new(settings::load()),
            pings: Mutex::new(ping::Table::new()),
            notices: Default::default(),
        }
    }

    pub async fn transition(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.transition.lock().await
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn patch(&self, patch: settings::Patch) -> Result<()> {
        self.update(|settings| patch.apply(settings))
    }

    /// Узлы: состав знает диск, поверх ложатся замеры (D-061).
    ///
    /// У ядра список больше не спрашиваем: оно пропускает схемы, которых не понимает,
    /// и от этого узлы исчезали при подключении, а выбор слетал (B-006). Порядок здесь
    /// тоже свой — файловый, а не случайный, как у ответа ядра (B-007).
    pub fn nodes(&self) -> Vec<crate::nodes::Node> {
        let mut nodes = crate::nodes::source_catalog::nodes();
        crate::nodes::enrich(
            &mut nodes,
            &self.pings.lock().unwrap(),
            &crate::nodes::geo::load(),
        );
        nodes
    }

    /// Померить, сколько до каждого сервера, и запомнить (D-062).
    ///
    /// Способ берётся из клиентского конфига (D-069): разводка по способам живёт
    /// в `app/measure.rs`, здесь только «спросить и запомнить».
    ///
    /// Замеры заменяются целиком, а не сливаются: сервер, переставший отвечать, обязан
    /// показать прочерк, а не вчерашнее число.
    pub async fn measure(&self) -> Result<()> {
        let method = crate::app::client::ping()?;
        let nodes = self.nodes();
        // Последовательный способ показывает числа по мере готовности (D-072), поэтому
        // прошлые стираем: иначе таблица десятки секунд мешает старые с новыми, и понять,
        // где чей замер, невозможно. Быстрые способы заменяют таблицу разом, как и раньше.
        if method == ping::Method::ProxyKeepalive {
            self.pings.lock().unwrap().clear();
        }
        let table =
            crate::app::measure::run(&nodes, &self.supervisor, method, &|address, reply| {
                self.pings
                    .lock()
                    .unwrap()
                    .insert(address.to_string(), reply);
            })
            .await?;
        *self.pings.lock().unwrap() = table;
        Ok(())
    }

    /// Сменить способ замера (D-069).
    ///
    /// Прошлые замеры при этом **забываются**: они получены другим способом, а колонка
    /// называет тот, который выбран, — и числа под этим заголовком были бы враньём.
    /// Пустая колонка честнее и заполняется сама: «Соединение» меряет непроверенный
    /// список при открытии.
    pub fn set_ping(&self, method: ping::Method) -> Result<()> {
        crate::app::client::set_ping(method)?;
        self.pings.lock().unwrap().clear();
        Ok(())
    }

    /// Через что идёт трафик. На остановленном ядре — тот, что мы запомнили сами (D-039):
    /// отметку в списке решает направление, а не живость процесса (D-092).
    ///
    /// В `RULES` спрашивать ядро бесполезно: псевдоним `umiray` там указывает на `AUTO`
    /// (D-056), а маршрут решают правила, и ответ «AUTO» — правда не про то (B-014).
    /// Правда там — цель `MATCH` применённого набора: набор с `MATCH,RU-VLESS-GROUP`
    /// именно её и назначает всему непойманному.
    pub async fn selected(&self) -> Option<String> {
        if self.settings().direction == Direction::Rules {
            if let Some(target) = routed(self.routing().ok().flatten().as_deref()) {
                return Some(target);
            }
        }
        if self.supervisor.status().running {
            return self.supervisor.selected().await;
        }
        self.settings().selected
    }

    /// Выход целиком: что выбрано и куда оно ведёт — `AUTO → Poland 1` (D-145). Пусто —
    /// выхода нет. На остановленном ядре цепочка из одного звена: разворачивать группы
    /// умеет только живое ядро.
    pub async fn route(&self) -> Vec<String> {
        match self.selected().await {
            Some(start) => self.supervisor.route(&start).await,
            None => Vec::new(),
        }
    }

    /// Сменить направление (D-056).
    ///
    /// Теперь это чистая настройка (D-071): файлы никуда не перекладываются. Набор лежит
    /// на своём месте всегда, а направление решает ровно одно — участвует он в сборке
    /// или конфиг собирается клиентом целиком.
    ///
    /// Узел приходит вместе с направлением: нажатие по строке таблицы — это одно
    /// действие, а не два.
    /// «Прямое» гасит и UDP-группу (D-113): правило `NETWORK,udp` идёт мимо псевдонима,
    /// и без этого UDP продолжал бы уходить в туннель при выключенном VPN. Гасим галку,
    /// а не обходим её при сборке: снятая галка в окне честнее включённой, которая
    /// ничего не делает.
    pub fn set_direction(&self, direction: Direction, node: Option<String>) -> Result<()> {
        if direction == Direction::Direct && crate::config::udp::on() {
            crate::config::udp::write(false)?;
        }
        self.update(|settings| {
            settings.direction = direction;
            if let Some(node) = &node {
                settings.selected = Some(node.clone());
            }
        })
    }

    /// Маршрутизация, которая уходит в сборку (D-071, D-075).
    ///
    /// Пусто — значит правила соберёт рендер сам: вне `RULES` набор пользователя
    /// не участвует. Группы к этому отношения не имеют — они общий документ и идут
    /// в сборку всегда. Набор, на который ссылается настройка, мог быть удалён мимо нас,
    /// — тогда тоже пусто, а не отказ собрать конфиг.
    pub fn routing(&self) -> Result<Option<String>> {
        let Some(id) = self.applied_preset() else {
            return Ok(None);
        };
        if presets::get(&id).is_err() {
            return Ok(None);
        }
        Ok(Some(presets::content(&id)?))
    }

    /// Набор, чьи документы сейчас уходят ядру. Вне `RULES` таких нет: там маршрут
    /// собирает клиент, и подсвечивать чей-то набор применённым было бы враньём.
    pub fn applied_preset(&self) -> Option<String> {
        let settings = self.settings();
        (settings.direction == Direction::Rules)
            .then_some(settings.preset)
            .flatten()
    }

    /// Первый источник переводит направление в автовыбор (D-056).
    pub fn note_source_added(&self, had_sources: bool) -> Result<()> {
        self.update(|settings| {
            settings.direction = direction::on_source_added(settings.direction, had_sources);
        })
    }

    /// Завести набор из того, что клиент собирает сам из источников.
    ///
    /// Он же первый: разделу без единого набора нечего показывать, поэтому один существует
    /// всегда (D-071). Содержимое — собранное, а не шаблон с рассказом: человек открывает
    /// раздел, чтобы поправить рабочий конфиг, а не прочитать инструкцию.
    pub fn new_preset(&self) -> Result<presets::Preset> {
        presets::create(
            presets::default_name(),
            &crate::render::effective::generated_rules()?,
        )
    }

    /// Применить набор: с этого момента маршрут решают его документы.
    ///
    /// Направление при этом встаёт в `RULES` — это и значит «применить». Набор, лежащий
    /// в стороне от сборки, применённым не является ни в каком смысле.
    pub fn select_preset(&self, id: &str) -> Result<()> {
        presets::get(id)?;
        self.update(|settings| {
            settings.preset = Some(id.to_string());
            settings.direction = Direction::Rules;
        })
    }

    /// Удалить набор.
    ///
    /// Применённый удалять нельзя: маршрут остался бы без документа, который его решает.
    /// Последний — тоже: разделу «Маршрутизация» нечего было бы показать, а завести новый
    /// можно и поверх старого.
    pub fn delete_preset(&self, id: &str) -> Result<()> {
        if self.applied_preset().as_deref() == Some(id) {
            return Err(crate::error::AppError::invalid(
                "Этот набор сейчас применён — сначала переключитесь на другой",
            ));
        }
        if presets::list().len() <= 1 {
            return Err(crate::error::AppError::invalid(
                "Это единственный набор: его правят, а не удаляют",
            ));
        }
        presets::delete(id)?;
        // Запомненный, но не применённый: ссылку чистим, иначе возврат в RULES полезет
        // за набором, которого нет.
        if self.settings().preset.as_deref() == Some(id) {
            self.update(|settings| settings.preset = None)?;
        }
        Ok(())
    }

    /// Ссылается ли что-нибудь написанное человеком на этот источник — и если да, сказать
    /// об этом словами.
    ///
    /// Пересобрать чужие документы за него нельзя, а удаление источника оставит в `use`
    /// ссылку в никуда. Ядро на такой конфиг отвечает отказом стартовать **целиком**
    /// (B-005), а связь между «удалил подписку» и «VPN больше не поднимается» иначе
    /// не увидеть: ошибка приходит только при следующем запуске.
    ///
    /// Смотрим **везде, а не в применённом наборе**: группы общие и участвуют в сборке
    /// всегда (D-075), а набор правил можно применить и завтра. Прежняя проверка молчала
    /// в трёх направлениях из четырёх — ровно тот случай, когда предупреждение приходит
    /// слишком поздно.
    pub fn referenced_warning(&self, id: &str) -> Option<String> {
        let groups = crate::config::files::read(crate::config::files::GROUPS).unwrap_or_default();
        if mentions(&groups, id) {
            return Some(
                "Ваши группы ссылаются на этот источник. Уберите ссылку — иначе ядро не запустится."
                    .into(),
            );
        }
        let named: Vec<String> = presets::list()
            .into_iter()
            .filter(|preset| {
                presets::content(&preset.id).is_ok_and(|rules| mentions(&rules[..], id))
            })
            .map(|preset| format!("«{}»", preset.name))
            .collect();
        (!named.is_empty()).then(|| {
            format!(
                "На этот источник ссылается маршрутизация: {}. Уберите ссылку — иначе ядро не запустится, когда набор применят.",
                named.join(", ")
            )
        })
    }

    /// Навести псевдоним туда, куда указывает направление.
    ///
    /// На остановленном ядре молча ничего не делаем: наводить нечего, а цель всё равно
    /// вычислится заново при следующем запуске.
    ///
    /// Отдаёт, сдвинулся ли выход: соединения, открытые через прежний, живут дальше,
    /// и рвать их — забота того, кто сдвигал (D-143).
    pub async fn point_alias(&self) -> Result<bool> {
        if !self.supervisor.status().running {
            return Ok(false);
        }
        let settings = self.settings();
        let names = self.node_names();
        let target = direction::target(settings.direction, settings.selected.as_deref(), &names);
        let moved = self.supervisor.selected().await.as_deref() != Some(target.as_str());
        self.supervisor.select(&target).await?;
        Ok(moved)
    }

    /// Направление с поправкой на то, что есть на самом деле: выбранный узел мог исчезнуть
    /// из подписки, и показывать `manual` в этом случае было бы враньём.
    pub fn direction(&self) -> Direction {
        let settings = self.settings();
        direction::resolve(
            settings.direction,
            settings.selected.as_deref(),
            &self.node_names(),
        )
    }

    /// Имена узлов, которые ядро **правда** поднимет: узел, которого оно не понимает,
    /// целью псевдонима быть не может (D-063).
    fn node_names(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter(|node| node.supported)
            .map(|node| node.name)
            .collect()
    }

    /// Перечитать настройки с диска. Нужно после сброса: файл переписали мимо `update`,
    /// и копия в памяти иначе осталась бы от прошлой жизни.
    pub fn reload_settings(&self) {
        *self.settings.lock().unwrap() = settings::load();
    }

    /// Запомнить, что стояло в настройках Windows до нас (D-047). `None` означает
    /// «мы уже вернули как было» — и именно по наличию снимка при старте видно,
    /// что прошлый запуск не успел прибраться.
    pub fn remember_proxy(&self, backup: Option<crate::system::sysproxy::Backup>) -> Result<()> {
        self.update(|settings| settings.proxy_backup = backup)
    }

    /// То же для брандмауэра: наличие снимка и означает «запрет наш» (D-073).
    pub fn remember_kill_switch(
        &self,
        backup: Option<crate::system::killswitch::Backup>,
    ) -> Result<()> {
        self.update(|settings| settings.kill_switch_backup = backup)
    }

    /// Сначала диск, потом память: если запись не удалась, они не должны разъехаться.
    ///
    /// Замок держится и на время записи: импорт подписки — асинхронная команда, и переключение
    /// режима во время неё выполнится параллельно. Читать-менять-писать без замка означало бы
    /// потерянное обновление. Запись короткая, дожидаться её не жалко.
    fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<()> {
        let mut current = self.settings.lock().unwrap();
        let mut next = current.clone();
        change(&mut next);
        settings::save(&next)?;
        *current = next;
        Ok(())
    }
}

/// Упоминается ли источник в документе. Простое вхождение подстроки, и этого достаточно:
/// идентификатор источника — шестнадцатеричная строка в шестнадцать знаков, случайно
/// такая в тексте не встречается. Ложное срабатывание здесь дешевле пропуска: цена
/// пропуска — ядро, которое не поднимется вовсе.
fn mentions(document: &str, id: &str) -> bool {
    !id.is_empty() && document.contains(id)
}

/// Куда набор отправляет всё непойманное — его `MATCH` (B-014).
///
/// Набора нет вовсе — пусто: тогда правила собирает клиент, и там `MATCH` целится
/// в псевдоним, про который честнее спросить ядро. Документ, который не разбирается,
/// — тоже пусто: соврать имя группы хуже, чем промолчать.
fn routed(routing: Option<&str>) -> Option<String> {
    let text = routing?;
    crate::config::rules::parse(text)
        .ok()
        .map(|routing| routing.fallback)
        // `MATCH` в псевдоним — это «спроси у ядра»: псевдоним не сервер, а указатель
        // на него, и разворачивает его ядро.
        .filter(|target| target != crate::config::direction::SELECTOR)
}

#[cfg(test)]
mod tests {
    use super::{mentions, routed};

    /// В RULES «через какой сервер» отвечает документ, а не ядро: у ядра там всегда `AUTO`.
    #[test]
    fn the_set_says_where_everything_unmatched_goes() {
        assert_eq!(
            routed(Some(
                "rules:
  - DOMAIN,a.ru,DIRECT
  - MATCH,RU-VLESS-GROUP
"
            )),
            Some("RU-VLESS-GROUP".to_string())
        );
        assert_eq!(
            routed(Some(
                "rules:
  - DOMAIN,a.ru,DIRECT
  - MATCH,umiray
"
            )),
            None,
            "MATCH в псевдоним — не ответ: куда он ведёт, знает ядро"
        );
        assert_eq!(routed(None), None, "набора нет — спрашиваем ядро");
        assert_eq!(
            routed(Some(
                "rules: 12
"
            )),
            None,
            "документ не разобрался — молчим, а не выдумываем имя"
        );
    }

    #[test]
    fn a_source_is_found_wherever_it_is_written() {
        let groups = "proxy-groups:
  - name: Европа
    use: [aaaa1111bbbb2222]
";
        assert!(mentions(groups, "aaaa1111bbbb2222"));
        assert!(!mentions(groups, "cccc3333dddd4444"));
        assert!(mentions(
            "rules:
  - RULE-SET,aaaa1111bbbb2222,umiray
",
            "aaaa1111bbbb2222"
        ));
        assert!(!mentions("", "aaaa1111bbbb2222"));
        assert!(
            !mentions("что угодно", ""),
            "пустой идентификатор не совпадает со всем"
        );
    }
}
