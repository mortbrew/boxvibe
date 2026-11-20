//! Pandora Browse: `POST /api/v1/feed/getDirectory`.
//!
//! The main directory (`{}`) is a short teaser of every section. A section
//! may include `viewAll.webUrl`; that page is the same call with
//! `{"deviceType":"web","path":…,"feedType":"browse"}` and can hold many
//! sections (New Music: stations, recommended releases, all new albums).
//! Every item is kept — nothing is capped. Annotations fill in names when
//! a row is sparse.

use serde_json::{Value, json};

use super::{Client, Error};

/// One Browse section as the directory returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseSection {
    pub id: String,
    pub title: String,
    pub items: Vec<BrowseItem>,
    /// `viewAll.webUrl` on the main teaser (`/browse/music/new-music`, …).
    /// `None`: this section has no View-all page (Artists Picked For You).
    pub view_all_path: Option<String>,
}

/// One row under a Browse section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseItem {
    pub name: String,
    /// Second line (artist, listener count, …), when Pandora sent one.
    pub detail: String,
    pub pandora_id: String,
    /// `SF:…` when Pandora offers a station factory for this row.
    pub station_factory: String,
    /// Item `webUrl`, as a site path. A category shelf (Genres, Decades, Moods)
    /// uses it to open the next directory page. Absent: this row has no page.
    pub link: Option<String>,
}

impl BrowseItem {
    /// Id to pass to [`Client::create_station`], when this row can start one.
    pub fn create_id(&self) -> Option<&str> {
        let id = self.pandora_id.as_str();
        if id.starts_with("AR:") || id.starts_with("GE:") || id.starts_with("TR:") || id.starts_with("SF:") {
            return Some(id);
        }
        let sf = self.station_factory.as_str();
        if sf.starts_with("SF:") {
            return Some(sf);
        }
        None
    }
}

impl Client {
    /// Main Browse (`/browse`): body `{}`. Sections may carry [`BrowseSection::view_all_path`].
    pub fn directory(&mut self) -> Result<Vec<BrowseSection>, Error> {
        let v = self.post("/api/v1/feed/getDirectory", &json!({}))?;
        Ok(parse_directory(&v))
    }

    /// One View-all page. `path` is that section's `viewAll.webUrl`.
    ///
    /// Body is `{"deviceType":"web","path":…,"feedType":"browse"}`. Only `path` changes.
    pub fn directory_at(&mut self, path: &str) -> Result<Vec<BrowseSection>, Error> {
        let v = self.post(
            "/api/v1/feed/getDirectory",
            &json!({
                "deviceType": "web",
                "path": path,
                "feedType": "browse",
            }),
        )?;
        Ok(parse_directory(&v))
    }

    /// `v4/catalog/getDetails`: the episodes of a podcast program (`PC:`).
    ///
    /// `recentEpisodes` is a list of `PE:` ids, named from `annotations`.
    /// A show's `/podcast/…` path is not a directory page.
    pub fn podcast_episodes(&mut self, pandora_id: &str, title: &str) -> Result<Vec<BrowseSection>, Error> {
        let v = self.post("/api/v4/catalog/getDetails", &json!({ "pandoraId": pandora_id }))?;
        Ok(parse_podcast_details(&v, pandora_id, title))
    }
}

/// Sections and items from a getDirectory body (annotations fill sparse names).
///
/// Every item is returned. A View-all page can be hundreds of rows; the list scrolls.
pub fn parse_directory(v: &Value) -> Vec<BrowseSection> {
    let anns = v.get("annotations").and_then(Value::as_object);
    let Some(sections) = v.get("sections").and_then(Value::as_array) else {
        return Vec::new();
    };
    sections.iter().filter_map(|s| parse_section(s, anns)).collect()
}

fn parse_section(s: &Value, anns: Option<&serde_json::Map<String, Value>>) -> Option<BrowseSection> {
    let id = s.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
    let title =
        label_text(s.get("title"), "header").or_else(|| label_text(s.get("title"), "title")).unwrap_or_default();
    if title.is_empty() && id.is_empty() {
        return None;
    }
    let mut items = Vec::new();
    if let Some(list) = s.get("items").and_then(Value::as_array) {
        collect_items(list, anns, &mut items);
    }
    if let Some(subs) = s.get("subsections").and_then(Value::as_array) {
        for sub in subs {
            let Some(list) = sub.get("items").and_then(Value::as_array) else { continue };
            collect_items(list, anns, &mut items);
        }
    }
    Some(BrowseSection { id, title, items, view_all_path: section_view_all_path(s) })
}

/// A row, plus any rows nested inside it. A container with no id of its own
/// is only there to hold the rows, so it is not listed.
fn collect_items(list: &[Value], anns: Option<&serde_json::Map<String, Value>>, out: &mut Vec<BrowseItem>) {
    for it in list {
        let nested = it.get("items").and_then(Value::as_array);
        if let Some(nested) = nested {
            collect_items(nested, anns, out);
        }
        if let Some(item) = parse_item(it, anns)
            && !(nested.is_some() && item.pandora_id.is_empty() && item.link.is_none())
        {
            out.push(item);
        }
    }
}

/// `webUrl` as a site path (`/browse/music/genres/pop`). Query and fragment dropped.
fn site_path(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let raw = raw.split(['?', '#']).next().unwrap_or(raw).trim();
    if raw.is_empty() || raw.contains("..") {
        return None;
    }
    let path = raw
        .strip_prefix("https://www.pandora.com")
        .or_else(|| raw.strip_prefix("http://www.pandora.com"))
        .unwrap_or(raw);
    if path.starts_with('/') {
        return Some(path.to_owned());
    }
    // `browse/podcasts/categories/comedy`, with the leading slash left off.
    (!path.contains("://") && !path.starts_with("//") && path.contains('/')).then(|| format!("/{path}"))
}

/// `viewAll.webUrl`, as a site path (`/browse/music/new-music`).
fn section_view_all_path(section: &Value) -> Option<String> {
    site_path(section.get("viewAll")?.get("webUrl")?.as_str()?)
}

fn parse_item(it: &Value, anns: Option<&serde_json::Map<String, Value>>) -> Option<BrowseItem> {
    let pandora_id = item_id(it);
    let station_factory = it.get("stationFactory").and_then(Value::as_str).unwrap_or("").to_owned();
    let ann = annotation(anns, &pandora_id);
    let mut name = label_text(Some(it), "title")
        .or_else(|| field_text(it.get("text").and_then(|t| t.get("title"))))
        .or_else(|| field_text(it.get("name")))
        .or_else(|| field_text(it.get("title")))
        .unwrap_or_default();
    let mut detail = label_styled(Some(it), "detail")
        .or_else(|| field_text(it.get("text").and_then(|t| t.get("subtitle"))))
        .unwrap_or_default();
    if let Some(ann) = ann {
        if name.is_empty() {
            name = ann_name(ann);
        }
        if detail.is_empty() {
            detail = field_text(ann.get("artistName"))
                .or_else(|| field_text(ann.get("programName")))
                .or_else(|| field_text(ann.get("description")))
                .unwrap_or_default();
        }
    }
    let link = item_link(it).or_else(|| ann.and_then(item_link));
    // A category row often has no label, only a path like
    // `/browse/podcasts/categories/true-crime`. The last segment is the name.
    if (name.is_empty() || name == pandora_id)
        && let Some(from_path) = link.as_deref().and_then(name_from_path)
    {
        name = from_path;
    }
    if name.is_empty() && pandora_id.is_empty() {
        return None;
    }
    if name.is_empty() {
        name = pandora_id.clone();
    }
    Some(BrowseItem { name, detail, pandora_id, station_factory, link })
}

/// `pandoraId`, or an `id` that is already a Pandora id (`PC:`, `BR:`, …).
fn item_id(it: &Value) -> String {
    for key in ["pandoraId", "id", "sourceId"] {
        let Some(id) = it.get(key).and_then(Value::as_str) else { continue };
        if id.contains(':') {
            return id.to_owned();
        }
    }
    String::new()
}

fn annotation<'a>(anns: Option<&'a serde_json::Map<String, Value>>, id: &str) -> Option<&'a Value> {
    if id.is_empty() {
        return None;
    }
    anns?.get(id)
}

fn ann_name(ann: &Value) -> String {
    for key in ["name", "title", "categoryName", "sortableName", "webName", "episodeName", "programName"] {
        if let Some(text) = field_text(ann.get(key)) {
            return text;
        }
    }
    String::new()
}

fn item_link(node: &Value) -> Option<String> {
    for key in ["webUrl", "urlPath", "shareableUrlPath", "href"] {
        if let Some(path) = field_text(node.get(key)).as_deref().and_then(site_path) {
            return Some(path);
        }
    }
    None
}

/// `true-crime` from `/browse/podcasts/categories/true-crime`.
fn name_from_path(path: &str) -> Option<String> {
    let slug = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if slug.is_empty() || slug.contains(':') || slug.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let name = slug
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    (!name.is_empty()).then_some(name)
}

/// A label's text, a plain string, or `{ "value": "…" }` / `{ "text": "…" }`.
fn field_text(v: Option<&Value>) -> Option<String> {
    let v = v?;
    let text = match v {
        Value::String(s) => s.trim().to_owned(),
        Value::Object(_) => {
            return field_text(v.get("value"))
                .or_else(|| field_text(v.get("text")))
                .or_else(|| field_text(v.get("url")));
        }
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

/// Episodes from `v4/catalog/getDetails`. Names come from the episode or its annotation.
///
/// `recentEpisodes` is a list of `PE:` ids, episode objects, or both. A program
/// with no list still contributes annotation episodes. Rows have no site link:
/// Enter plays the episode.
pub fn parse_podcast_details(v: &Value, program_id: &str, title: &str) -> Vec<BrowseSection> {
    let anns = v.get("annotations").or_else(|| v.pointer("/details/annotations")).and_then(Value::as_object);
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    walk_episode_lists(v, anns, &mut items, &mut seen);
    if items.is_empty()
        && let Some(map) = anns
    {
        for (key, ann) in map {
            let id = item_id(ann);
            let id = if id.starts_with("PE:") {
                id
            } else if key.starts_with("PE:") {
                key.clone()
            } else {
                continue;
            };
            push_episode_node(&id, ann, ann, &mut items, &mut seen);
        }
    }
    let title = if title.is_empty() { program_id.to_owned() } else { title.to_owned() };
    vec![BrowseSection { id: program_id.to_owned(), title, items, view_all_path: None }]
}

fn walk_episode_lists(
    v: &Value,
    anns: Option<&serde_json::Map<String, Value>>,
    items: &mut Vec<BrowseItem>,
    seen: &mut std::collections::HashSet<String>,
) {
    match v {
        Value::Array(rows) => {
            for row in rows {
                walk_episode_lists(row, anns, items, seen);
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                if matches!(key.as_str(), "recentEpisodes" | "episodes" | "allEpisodes") {
                    let Some(rows) = val.as_array() else { continue };
                    for row in rows {
                        push_episode(row, anns, items, seen);
                    }
                } else {
                    walk_episode_lists(val, anns, items, seen);
                }
            }
        }
        _ => {}
    }
}

fn push_episode(
    entry: &Value,
    anns: Option<&serde_json::Map<String, Value>>,
    items: &mut Vec<BrowseItem>,
    seen: &mut std::collections::HashSet<String>,
) {
    match entry {
        Value::String(id) if id.starts_with("PE:") => {
            let ann = annotation(anns, id).unwrap_or(&Value::Null);
            push_episode_node(id, ann, ann, items, seen);
        }
        Value::Array(rows) => {
            for row in rows {
                push_episode(row, anns, items, seen);
            }
        }
        Value::Object(_) => {
            let id = item_id(entry);
            if !id.starts_with("PE:") {
                return;
            }
            let ann = annotation(anns, &id).unwrap_or(&Value::Null);
            push_episode_node(&id, entry, ann, items, seen);
        }
        _ => {}
    }
}

fn push_episode_node(
    id: &str,
    node: &Value,
    ann: &Value,
    items: &mut Vec<BrowseItem>,
    seen: &mut std::collections::HashSet<String>,
) {
    if !seen.insert(id.to_owned()) {
        return;
    }
    let mut name = {
        let episode = field_text(node.get("episodeName")).or_else(|| field_text(ann.get("episodeName")));
        episode.unwrap_or_else(|| ann_name(node))
    };
    if name.is_empty() || name == id {
        name = ann_name(ann);
    }
    if name.is_empty() || name == id {
        name = id.to_owned();
    }
    items.push(BrowseItem {
        name,
        detail: episode_length(node).or_else(|| episode_length(ann)).unwrap_or_default(),
        pandora_id: id.to_owned(),
        station_factory: String::new(),
        link: None,
    });
}

fn episode_length(node: &Value) -> Option<String> {
    let seconds = node.get("duration").and_then(Value::as_u64)?;
    if seconds == 0 {
        return None;
    }
    let minutes = seconds.div_ceil(60);
    if minutes < 60 {
        return Some(format!("{minutes} min"));
    }
    Some(format!("{}h {}m", minutes / 60, minutes % 60))
}

/// The label with `style`, or the first label when the row does not mark one.
fn label_text(node: Option<&Value>, style: &str) -> Option<String> {
    label_styled(node, style).or_else(|| {
        let labels = node?.get("labels")?.as_array()?;
        field_text(labels.first()?.get("text"))
    })
}

fn label_styled(node: Option<&Value>, style: &str) -> Option<String> {
    let labels = node?.get("labels")?.as_array()?;
    labels
        .iter()
        .find(|l| l.get("style").and_then(Value::as_str) == Some(style))
        .and_then(|l| field_text(l.get("text")))
}

#[cfg(test)]
mod tests;
