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
    if let Some(subs) = s.get("subsections").and_then(Value::as_array) {
        for sub in subs {
            let Some(list) = sub.get("items").and_then(Value::as_array) else { continue };
            for it in list {
                if let Some(item) = parse_item(it, anns) {
                    items.push(item);
                }
            }
        }
    }
    Some(BrowseSection { id, title, items, view_all_path: section_view_all_path(s) })
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
    path.starts_with('/').then(|| path.to_owned())
}

/// `viewAll.webUrl`, as a site path (`/browse/music/new-music`).
fn section_view_all_path(section: &Value) -> Option<String> {
    site_path(section.get("viewAll")?.get("webUrl")?.as_str()?)
}

fn parse_item(it: &Value, anns: Option<&serde_json::Map<String, Value>>) -> Option<BrowseItem> {
    let pandora_id = it.get("pandoraId").and_then(Value::as_str).unwrap_or("").to_owned();
    let station_factory = it.get("stationFactory").and_then(Value::as_str).unwrap_or("").to_owned();
    let mut name = label_text(Some(it), "title").unwrap_or_default();
    let mut detail = label_text(Some(it), "detail").unwrap_or_default();
    if name.is_empty()
        && let Some(ann) = anns.and_then(|a| a.get(&pandora_id))
    {
        name = ann.get("name").and_then(Value::as_str).unwrap_or("").trim().to_owned();
        if detail.is_empty() {
            detail = ann
                .get("artistName")
                .or_else(|| ann.get("description"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_owned();
        }
    }
    if name.is_empty() && pandora_id.is_empty() {
        return None;
    }
    if name.is_empty() {
        name = pandora_id.clone();
    }
    let link = ["webUrl", "urlPath", "shareableUrlPath"]
        .into_iter()
        .find_map(|key| it.get(key).and_then(Value::as_str).and_then(site_path));
    Some(BrowseItem { name, detail, pandora_id, station_factory, link })
}

fn label_text(node: Option<&Value>, style: &str) -> Option<String> {
    let labels = node?.get("labels")?.as_array()?;
    labels
        .iter()
        .find(|l| l.get("style").and_then(Value::as_str) == Some(style))
        .or_else(|| labels.first())
        .and_then(|l| l.get("text").and_then(Value::as_str))
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests;
