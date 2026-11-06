//! Pandora Browse: `POST /api/v1/feed/getDirectory`.
//!
//! One response feeds every Browse row (Artists Picked For You, New Music,
//! genres, playlists, …). Sections have a title and items; annotations fill
//! in names when a row is sparse.

use serde_json::{Value, json};

use super::{Client, Error};

/// One Browse section as the directory returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseSection {
    pub id: String,
    pub title: String,
    pub items: Vec<BrowseItem>,
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
    /// `v1/feed/getDirectory`: every Browse section and its rows.
    pub fn directory(&mut self) -> Result<Vec<BrowseSection>, Error> {
        let v = self.post("/api/v1/feed/getDirectory", &json!({}))?;
        Ok(parse_directory(&v))
    }
}

/// Sections and items from a getDirectory body (annotations fill sparse names).
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
    Some(BrowseSection { id, title, items })
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
    Some(BrowseItem { name, detail, pandora_id, station_factory })
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn directory_parses_sections_items_and_annotations() {
        let v = json!({
            "sections": [{
                "id": "BS:1:3:1",
                "title": {"labels": [{"text": "Artists Picked For You", "style": "header"}]},
                "subsections": [{"items": [{
                    "type": "grid",
                    "pandoraId": "AR:1",
                    "stationFactory": "SF:16722:1",
                    "labels": [{"text": "M83", "style": "title"}]
                }]}]
            }, {
                "id": "BS:1:8",
                "title": {"labels": [{"text": "New Music", "style": "header"}]},
                "subsections": [{"items": [{
                    "type": "grid",
                    "pandoraId": "AL:9",
                    "labels": []
                }]}]
            }],
            "annotations": {
                "AL:9": {"name": "Sparse Album", "artistName": "Someone", "type": "AL"}
            }
        });
        let sections = parse_directory(&v);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, "Artists Picked For You");
        assert_eq!(sections[0].items[0].name, "M83");
        assert_eq!(sections[0].items[0].create_id(), Some("AR:1"));
        assert_eq!(sections[1].items[0].name, "Sparse Album");
        assert_eq!(sections[1].items[0].detail, "Someone");
        assert!(sections[1].items[0].create_id().is_none());
    }
}
