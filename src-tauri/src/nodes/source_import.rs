//! Добавление и обновление источников узлов.

use crate::error::{AppError, Result};
use crate::nodes::source_id::SourceId;
use crate::nodes::sources::assemble;
use crate::nodes::sources::commit;
use crate::nodes::sources::own_proxies;
use crate::nodes::sources::taken_by_others;
use crate::nodes::sources::write;
use crate::nodes::sources::Source;
use crate::nodes::sources::SourceStore;
use crate::nodes::subscription::{Fetched, Subscription};

/// Имя источника для ссылок, добавленных руками.
const MANUAL: &str = "Мои ссылки";

/// Имя источника для узлов, которые клиент пишет сам: собранных руками и принесённых
/// файлом (D-120). Отдельно от «Моих ссылок» не из вкуса: файл провайдера не бывает
/// наполовину списком ссылок, наполовину документом `proxies:`.
const OWN: &str = "Свои узлы";

pub struct SourceImporter;

impl SourceImporter {
    /// Завести подписку: скачать, вычистить имена, сохранить. Служебные записи провайдера
    /// возвращаются отдельно — в узлы они не идут, но показать их надо.
    pub async fn add_subscription(url: &str) -> Result<(Source, Vec<String>)> {
        let id = SourceId::new()?.as_str().to_string();
        let fetched = Subscription::fetch(url).await?;
        let source = Source {
            id: id.clone(),
            name: host_of(url),
            url: Some(url.to_string()),
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
            failed: None,
            renamed: false,
        };
        let notices = accept(source, &id, &fetched)?;
        Ok((SourceStore::get(&id)?, notices))
    }

    /// Обновить подписку. Отказ источник запоминает (`failed`): фоновое обновление
    /// человек не видел, и молчать о нём нельзя (D-038).
    pub async fn refresh(id: &str) -> Result<(Source, Vec<String>)> {
        SourceId::parse(id)?;
        let Some(url) = SourceStore::get(id)?.url else {
            return Err(AppError::invalid("Это не подписка: обновлять её неоткуда"));
        };
        refreshed(id, Subscription::fetch(&url).await)
    }

    /// Добавить ссылку руками (D-017): одиночная **дописывается**, а подписка список
    /// заменяет — она источник истины по серверам. Если источника «мои ссылки» ещё нет — заводим.
    pub fn add_link(uri: &str) -> Result<Source> {
        let uri = uri.trim();
        if !uri.contains("://") {
            return Err(AppError::invalid("Это не ссылка на сервер"));
        }
        // Дописываем **в список ссылок**, а не в первый попавшийся источник без адреса:
        // у «своих узлов» адреса тоже нет, и строка в их документе `proxies:` ломает его
        // целиком (GOTCHAS).
        let id = match SourceStore::list()
            .into_iter()
            .find(|source| source.url.is_none() && !source.records)
        {
            Some(existing) => existing.id,
            None => SourceId::new()?.as_str().to_string(),
        };
        let mut source = SourceStore::get(&id).unwrap_or(Source {
            id: id.clone(),
            name: MANUAL.into(),
            url: None,
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
            failed: None,
            renamed: false,
        });

        let mut lines: Vec<String> = SourceStore::raw(&id).lines().map(str::to_string).collect();
        lines.extend(crate::nodes::link::LinkParser::lines_of(uri));
        write(&mut source, lines, &id, &crate::config::awg::Mask::get())?;
        SourceStore::get(&id)
    }

    /// Добавить узел записью (D-120): собранный руками или принесённый файлом.
    ///
    /// Ссылку из формы не собираем — её пришлось бы **выдумать**: единого формата у неё нет,
    /// а запись ядро читает однозначно, и имена полей у неё из его документации.
    pub fn add_proxy(entry: serde_yaml::Mapping) -> Result<Source> {
        let (id, mut source) = own()?;
        let mut proxies = own_proxies(&id);
        let mut entry = entry;
        let wanted = entry
            .get(serde_yaml::Value::from("name"))
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("Узел")
            .to_string();
        // Свои имена и имена всех остальных источников: узел подписки с тем же именем
        // сделал бы выбор в группе неоднозначным (S-012).
        let taken: Vec<String> = proxies
            .iter()
            .filter_map(|proxy| proxy.get("name")?.as_str().map(str::to_string))
            .chain(taken_by_others(&id))
            .collect();
        crate::yaml::Yaml::set(
            &mut entry,
            "name",
            serde_yaml::Value::from(unique(&wanted, &taken)),
        );
        proxies.push(serde_yaml::Value::Mapping(SourceStore::ordered(entry)));

        let mut document = serde_yaml::Mapping::new();
        crate::yaml::Yaml::set(
            &mut document,
            "proxies",
            serde_yaml::Value::Sequence(proxies),
        );
        let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(document))
            .map_err(|e| AppError::invalid(e.to_string()))?;
        write(
            &mut source,
            vec![yaml],
            &id,
            &crate::config::awg::Mask::get(),
        )?;
        SourceStore::get(&id)
    }

    /// Запись узла из файла с конфигом. Понимаем только то, что узнаём: чужой формат получает
    /// отказ с текстом, а не узел, который молча не работает (D-120). В источник запись
    /// кладёт сервис — после того, как её проверило ядро.
    pub fn file_entry(path: &std::path::Path) -> Result<serde_yaml::Mapping> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| AppError::io(format!("Файл не читается: {e}")))?;
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Узел")
            .to_string();
        SourceImporter::file_proxy(&text, &name)
    }

    /// Запись узла из текста файла. Различаем по тексту, а не по расширению: `.conf`
    /// бывает и у OpenVPN.
    pub fn file_proxy(text: &str, name: &str) -> Result<serde_yaml::Mapping> {
        if crate::nodes::ovpn::Ovpn::looks_like(text) {
            crate::nodes::ovpn::Ovpn::to_proxy(text, name)
        } else if crate::nodes::usque::Usque::looks_like(text) {
            crate::nodes::usque::Usque::to_proxy(text, name)
        } else {
            crate::nodes::wgconf::WgConf::to_proxy(text, name)
        }
    }
}

/// Ответ панели — или отказ сети — в источник, который обновляли.
///
/// Источник читается **здесь**, после сети, а не до неё: пока шёл запрос, его могли удалить,
/// и ответ панели воскресил бы удалённый вместе с токеном (B-038). Отказ источник
/// запоминает — о фоновом обновлении иначе никто бы не узнал (D-038).
fn refreshed(id: &str, fetched: Result<Fetched>) -> Result<(Source, Vec<String>)> {
    let accepted = fetched.and_then(|fetched| {
        let source = SourceStore::get(id)
            .map_err(|_| AppError::invalid("Подписку удалили, пока она обновлялась"))?;
        accept(source, id, &fetched)
    });
    match accepted {
        Ok(notices) => Ok((SourceStore::get(id)?, notices)),
        Err(why) => {
            let _ = SourceStore::mark_failed(id, &why.to_string());
            Err(why)
        }
    }
}

/// Принять ответ панели и записать источник.
///
/// Ответ, из которого не собрался ни один узел, не имеет права стирать то, что есть (D-018).
/// Не только пустой: страница «подписка истекла», портал Wi-Fi и HTML приходят с кодом 200,
/// и раньше они молча оставляли источник без узлов, — а группа без узлов у ядра выходит
/// напрямую, при зелёном окне (B-039).
fn accept(mut source: Source, id: &str, fetched: &Fetched) -> Result<Vec<String>> {
    let (lines, notices) = Subscription::links(&fetched.body);
    // Панель обычно называет себя сама — это понятнее хоста из адреса. Имя, данное
    // человеком, главнее (D-172).
    if let Some(title) = fetched.title.as_ref().filter(|_| !source.renamed) {
        source.name = title.clone();
    }
    if lines.is_empty() {
        // Причину провайдер написал в служебных записях.
        return Err(AppError::Subscription {
            message: "Подписка не вернула ни одного сервера".into(),
            notices,
        });
    }

    SourceId::parse(id)?;
    let raw = lines.join("\n");
    let text = assemble(&mut source, id, &raw, &crate::config::awg::Mask::get())?;
    if source.nodes == 0 {
        let mut notices = notices;
        notices.push(format!("Ответ начинается так: {}", opening(&fetched.body)));
        return Err(AppError::Subscription {
            message: "В ответе подписки нет ни одного узла — узлы остались прежними".into(),
            notices,
        });
    }
    source.updated = crate::stamp::Stamp::now();
    source.failed = None;
    commit(&source, id, &raw, &text)?;
    Ok(notices)
}

/// Первая строка ответа, коротко: по ней видно, что прислали вместо узлов.
fn opening(body: &str) -> String {
    const SHOWN: usize = 120;
    let line = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("(пусто)");
    match line.char_indices().nth(SHOWN) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_string(),
    }
}

/// Источник для записей: найти или завести. Узнаём его по содержимому, а не по имени:
/// имя пользователь вправе сменить.
fn own() -> Result<(String, Source)> {
    if let Some(existing) = SourceStore::list()
        .into_iter()
        .find(|source| source.records)
    {
        let id = existing.id.clone();
        return Ok((id, existing));
    }
    let id = SourceId::new()?.as_str().to_string();
    let source = Source {
        id: id.clone(),
        name: OWN.into(),
        url: None,
        updated: None,
        nodes: 0,
        records: false,
        skipped: Vec::new(),
        failed: None,
        renamed: false,
    };
    Ok((id, source))
}

/// Имя, которого ещё нет рядом. Ядро различает узлы по имени, и два одинаковых оно
/// не примет вовсе.
fn unique(wanted: &str, taken: &[String]) -> String {
    if !taken.iter().any(|name| name == wanted) {
        return wanted.to_string();
    }
    (2..)
        .map(|n| format!("{wanted} {n}"))
        .find(|name| !taken.iter().any(|taken| taken == name))
        .unwrap_or_else(|| wanted.to_string())
}

/// Имя источника по адресу — его хост. Пользователь узнаёт свою панель по нему.
pub(super) fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .filter(|host| !host.is_empty())
        .unwrap_or("Подписка")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Sandbox;

    const LIVE: &str = "vless://11111111-1111-1111-1111-111111111111@a.example:443?encryption=none&security=tls&sni=a.example#Alpha";

    fn answer(body: &str) -> Result<Fetched> {
        Ok(Fetched {
            body: body.to_string(),
            title: None,
        })
    }

    /// Подписка с одним живым узлом, как после удачного обновления.
    fn subscribed() -> String {
        let id = SourceId::new().unwrap().as_str().to_string();
        let mut source = Source {
            id: id.clone(),
            name: "Панель".into(),
            url: Some("https://panel.example/sub".into()),
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
            failed: None,
            renamed: false,
        };
        write(
            &mut source,
            vec![LIVE.into()],
            &id,
            &crate::config::awg::Mask::default(),
        )
        .unwrap();
        id
    }

    /// B-039: страница «подписка истекла», портал Wi-Fi, HTML с кодом 200 — строки есть,
    /// узлов нет. Узлы остаются прежними, а причина запоминается и видна.
    #[test]
    fn an_answer_without_a_single_node_keeps_the_nodes_and_says_why() {
        let _sandbox = Sandbox::new("answer-without-nodes");
        let id = subscribed();
        let before = SourceStore::content(&id);

        for body in [
            "Ваша подписка истекла. Продлите её в личном кабинете.",
            "<html>\n<body><a href=\"https://panel.example/renew\">renew</a></body>\n</html>",
        ] {
            let refused = refreshed(&id, answer(body)).unwrap_err();
            assert!(
                refused
                    .details()
                    .iter()
                    .any(|line| line.contains(body.lines().next().unwrap())),
                "окно видит, что пришло: {:?}",
                refused.details()
            );
            assert_eq!(SourceStore::content(&id), before, "узлы не тронуты");
            assert_eq!(SourceStore::get(&id).unwrap().nodes, 1);
            assert!(
                SourceStore::get(&id).unwrap().failed.is_some(),
                "отказ запомнен"
            );
        }

        // Удачное обновление снимает отметку.
        refreshed(&id, answer(LIVE)).unwrap();
        assert_eq!(SourceStore::get(&id).unwrap().failed, None);
    }

    /// B-038: ответ панели пришёл, когда подписку уже удалили, — и не воскрешает её.
    #[test]
    fn an_answer_after_the_delete_does_not_bring_the_source_back() {
        let _sandbox = Sandbox::new("answer-after-delete");
        let id = subscribed();
        SourceStore::delete(&id).unwrap();

        assert!(refreshed(&id, answer(LIVE)).is_err());
        assert!(SourceStore::list().iter().all(|source| source.id != id));
        assert!(!SourceStore::provider(&id).exists());
    }

    /// Отказ сети тоже запоминается: о фоновом обновлении иначе никто не узнает (D-038).
    #[test]
    fn a_network_failure_is_remembered_by_the_source() {
        let _sandbox = Sandbox::new("answer-network");
        let id = subscribed();
        let failed = refreshed(&id, Err(AppError::network("Подписка ответила 502")));
        assert!(failed.is_err());
        assert_eq!(
            SourceStore::get(&id).unwrap().failed.as_deref(),
            Some("Подписка ответила 502")
        );
        assert_eq!(SourceStore::get(&id).unwrap().nodes, 1);
    }

    #[test]
    fn the_opening_of_an_answer_is_one_short_line() {
        assert_eq!(opening("\n  <html>\n<body>"), "<html>");
        assert_eq!(opening(""), "(пусто)");
        let long = "я".repeat(300);
        assert_eq!(
            opening(&long).chars().count(),
            121,
            "120 знаков и многоточие"
        );
    }
}
