//! Добавление и обновление источников узлов.

use crate::error::{AppError, Result};
use crate::nodes::source_id::SourceId;
use crate::nodes::sources::own_proxies;
use crate::nodes::sources::taken_by_others;
use crate::nodes::sources::write;
use crate::nodes::sources::Source;
use crate::nodes::sources::SourceStore;
use crate::nodes::subscription::Subscription;

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
        let source = Source {
            id: id.clone(),
            name: host_of(url),
            url: Some(url.to_string()),
            updated: None,
            nodes: 0,
            records: false,
            skipped: Vec::new(),
        };
        let notices = download(source, &id).await?;
        Ok((SourceStore::get(&id)?, notices))
    }

    pub async fn refresh(id: &str) -> Result<(Source, Vec<String>)> {
        SourceId::parse(id)?;
        let source = SourceStore::get(id)?;
        if source.url.is_none() {
            return Err(AppError::invalid("Это не подписка: обновлять её неоткуда"));
        }
        let notices = download(source, id).await?;
        Ok((SourceStore::get(id)?, notices))
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

async fn download(mut source: Source, id: &str) -> Result<Vec<String>> {
    let url = source.url.clone().unwrap_or_default();
    let fetched = Subscription::fetch(&url).await?;
    let (lines, notices) = Subscription::links(&fetched.body);
    // Панель обычно называет себя сама — это понятнее хоста из адреса.
    if let Some(title) = fetched.title {
        source.name = title;
    }

    if lines.is_empty() {
        // Пустой ответ не имеет права стирать то, что уже есть (D-018): провайдер молчит по своим
        // причинам, а причину он написал в служебных записях.
        return Err(AppError::Subscription {
            message: "Подписка не вернула ни одного сервера".into(),
            notices,
        });
    }

    source.updated = crate::stamp::Stamp::now();
    write(&mut source, lines, id, &crate::config::awg::Mask::get())?;
    Ok(notices)
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
