//! Скачать список и положить его нейтральным (D-157).
//!
//! Здесь ровно загрузка и разбор. Что делать с новыми данными ядрам — собрать свой
//! формат, перечитать провайдера, — решает сервис `app::lists`.

use crate::collections::Collections;
use crate::error::{AppError, Result};
use crate::http::Http;
use crate::lists::parse::ListParser;
use crate::lists::store::{ListStore, RuleList};
use crate::slug::Slug;
use crate::stamp::Stamp;

/// Предел одного адреса. Самый большой список каталога — antizapret, 56 МБ JSON (S-028).
const LIMIT: usize = 128 * 1024 * 1024;

pub struct ListImporter;

impl ListImporter {
    /// Список, на который ссылается маршрут (D-158): скачанный — как есть, нет — скачать.
    ///
    /// `url` — свой список из документа; пусто — из каталога. Документ — источник истины:
    /// адрес в нём сменили — скачанное по старому адресу уже не то, и качаем заново.
    pub async fn ensure(id: &str, url: Option<&str>) -> Result<RuleList> {
        ListStore::valid(id)?;
        let cached = ListStore::get(id).ok();
        if let Some(list) = &cached {
            let same = url.is_none_or(|url| list.urls == [url]);
            if same && list.updated.is_some() {
                return Ok(list.clone());
            }
        }
        let list = match url {
            Some(url) => RuleList {
                id: id.to_string(),
                title: cached.map_or_else(|| id.to_string(), |list| list.title),
                urls: vec![url.to_string()],
                ..RuleList::default()
            },
            None => {
                let entry = Collections::lists()?
                    .lists
                    .into_iter()
                    .find(|entry| entry.id == id)
                    .ok_or_else(|| AppError::invalid(format!("В каталоге нет списка {id}")))?;
                RuleList {
                    id: entry.id,
                    title: entry.title,
                    title_en: entry.title_en,
                    urls: entry.urls,
                    ..RuleList::default()
                }
            }
        };
        ListImporter::fetch(list).await
    }

    /// Свой список по адресу. Имя пустое — берём имя файла из адреса.
    pub async fn add_url(url: &str, title: &str) -> Result<RuleList> {
        let url = url.trim();
        let parsed = reqwest::Url::parse(url)
            .ok()
            .filter(|parsed| matches!(parsed.scheme(), "http" | "https"))
            .ok_or_else(|| AppError::invalid("Нужен адрес http:// или https://"))?;
        let title = match title.trim() {
            "" => parsed
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .map(|name| name.split('.').next().unwrap_or(name).to_string())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| parsed.host_str().unwrap_or("list").to_string()),
            title => title.to_string(),
        };
        // Имена каталога тоже заняты: иначе свой список отнял бы имя у того, что человек
        // добавит из каталога потом.
        let mut taken: Vec<String> = ListStore::list().into_iter().map(|list| list.id).collect();
        taken.extend(
            Collections::lists()?
                .lists
                .into_iter()
                .map(|entry| entry.id),
        );
        ListImporter::fetch(RuleList {
            id: Slug::free(&Slug::of(&title, "list"), &taken),
            title,
            urls: vec![url.to_string()],
            ..RuleList::default()
        })
        .await
    }

    /// Скачать заново. Неудача оставляет прежние данные и записывает причину в список:
    /// окно покажет её рядом с ним, а ядро продолжит работать по вчерашнему.
    pub async fn refresh(id: &str) -> Result<RuleList> {
        let list = ListStore::get(id)?;
        match ListImporter::fetch(list.clone()).await {
            Ok(fresh) => Ok(fresh),
            Err(why) => {
                ListStore::note(&RuleList {
                    error: Some(why.to_string()),
                    ..list
                })?;
                Err(why)
            }
        }
    }

    async fn fetch(mut list: RuleList) -> Result<RuleList> {
        let client = Http::client()?;
        let mut parser = ListParser::default();
        let mut published = None;
        for url in &list.urls {
            let download = Http::download(&client, url, LIMIT).await?;
            parser
                .feed(&String::from_utf8_lossy(&download.body))
                .map_err(|why| AppError::invalid(format!("{why}: {url}")))?;
            published = published.max(download.modified);
        }
        let payload = parser.finish();
        if payload.domains.is_empty() && payload.cidrs.is_empty() {
            return Err(AppError::invalid(format!(
                "В списке «{}» не нашлось ни домена, ни подсети",
                list.title
            )));
        }
        list.updated = Stamp::now();
        list.published = published;
        list.domains = payload.domains.len();
        list.cidrs = payload.cidrs.len();
        list.skipped = payload.skipped;
        list.error = None;
        ListStore::save(&list, &payload)?;
        Ok(list)
    }
}
