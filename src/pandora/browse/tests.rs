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
    assert_eq!(sections[0].view_all_path, None);
    assert_eq!(sections[1].view_all_path, None);
    let shelf = BrowseItem {
        name: "Country".into(),
        detail: String::new(),
        pandora_id: "BR:50:7".into(),
        station_factory: String::new(),
        link: None,
    };
    assert!(shelf.create_id().is_none(), "BR:50 rows do not start a station");
    let linked = parse_directory(&json!({
        "sections": [{
            "id": "BS:50",
            "title": {"labels": [{"text": "Browse Genres", "style": "header"}]},
            "subsections": [{"items": [{
                "pandoraId": "BR:50:7",
                "webUrl": "https://www.pandora.com/browse/music/genres/country?ref=1",
                "labels": [{"text": "Country", "style": "title"}]
            }]}]
        }]
    }));
    assert_eq!(linked[0].items[0].link.as_deref(), Some("/browse/music/genres/country"));
    assert!(linked[0].items[0].create_id().is_none());
    let path_item = parse_directory(&json!({
        "sections": [{
            "id": "BS:12",
            "title": {"labels": [{"text": "Playlists", "style": "header"}]},
            "subsections": [{"items": [{
                "pandoraId": "PL:1",
                "urlPath": "/playlist/the-drop",
                "labels": [{"text": "The Drop", "style": "title"}]
            }]}]
        }]
    }));
    assert_eq!(path_item[0].items[0].link.as_deref(), Some("/playlist/the-drop"));
}

#[test]
fn view_all_path_is_kept_and_every_item_is_kept() {
    let items: Vec<_> = (0..40)
        .map(|n| {
            json!({
                "pandoraId": format!("AL:{n}"),
                "labels": [{"text": format!("Album {n}"), "style": "title"}]
            })
        })
        .collect();
    let v = json!({
        "sections": [{
            "id": "BS:1:3:1",
            "title": {"labels": [{"text": "Artists Picked For You", "style": "header"}]},
            "subsections": [{"items": [{
                "pandoraId": "AR:1",
                "labels": [{"text": "M83", "style": "title"}]
            }]}]
        }, {
            "id": "BS:8:61",
            "title": {"labels": [{"text": "All New Albums", "style": "header"}]},
            "viewAll": {
                "webUrl": "https://www.pandora.com/browse/music/new-music?ref=1",
                "labels": [{"text": "View all new music", "style": "title"}]
            },
            "subsections": [{"items": items}]
        }, {
            "id": "BS:blank",
            "title": {"labels": [{"text": "No Link", "style": "header"}]},
            "viewAll": {"webUrl": "  "},
            "subsections": []
        }]
    });
    let sections = parse_directory(&v);
    assert_eq!(sections[0].view_all_path, None, "Artists Picked For You has no viewAll");
    assert_eq!(sections[1].view_all_path.as_deref(), Some("/browse/music/new-music"));
    assert_eq!(sections[1].items.len(), 40, "no silent cap");
    assert_eq!(sections[1].items[39].name, "Album 39");
    assert_eq!(sections[2].view_all_path, None);
}
