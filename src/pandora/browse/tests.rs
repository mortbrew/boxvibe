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
fn podcast_categories_use_the_path_and_nested_label_text() {
    let v = json!({
        "sections": [{
            "id": "BS:38",
            "title": {"labels": [{"text": "Podcasts by Category", "style": "header"}]},
            "subsections": [{"items": [
                {
                    "pandoraId": "BR:38:48",
                    "webUrl": "/browse/podcasts/categories/true-crime",
                    "labels": [{"style": "title", "text": {"value": ""}}]
                },
                {
                    "id": "PC:9",
                    "shareableUrlPath": "/podcast/crime-junkie/PC:9",
                    "labels": [{"style": "title", "text": {"value": "Crime Junkie"}}],
                    "text": {"subtitle": "Audiochuck"}
                },
                {
                    "type": "container",
                    "items": [{
                        "pandoraId": "PE:4",
                        "labels": [{"style": "title", "text": "Latest episode"}]
                    }]
                }
            ]}]
        }],
        "annotations": {
            "BR:38:12": {"categoryName": "Comedy", "shareableUrlPath": "/browse/podcasts/categories/comedy"}
        }
    });
    // The annotation id is the row's id when the row itself has no path.
    let mut with_ann = v.clone();
    with_ann["sections"][0]["subsections"][0]["items"][0]["pandoraId"] = json!("BR:38:12");
    with_ann["sections"][0]["subsections"][0]["items"][0].as_object_mut().unwrap().remove("webUrl");
    let sections = parse_directory(&v);
    assert_eq!(sections[0].items[0].name, "True Crime");
    assert_eq!(sections[0].items[0].link.as_deref(), Some("/browse/podcasts/categories/true-crime"));
    assert_eq!(sections[0].items[1].name, "Crime Junkie");
    assert_eq!(sections[0].items[1].detail, "Audiochuck");
    assert_eq!(sections[0].items[1].pandora_id, "PC:9");
    assert_eq!(sections[0].items[2].name, "Latest episode");
    assert_eq!(sections[0].items[2].pandora_id, "PE:4");
    let named = parse_directory(&with_ann);
    assert_eq!(named[0].items[0].name, "Comedy");
    assert_eq!(named[0].items[0].link.as_deref(), Some("/browse/podcasts/categories/comedy"));
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

#[test]
fn podcast_details_list_episode_ids_and_objects() {
    let v = json!({
        "details": {
            "annotations": {
                "PE:1": {"name": "Latest episode", "pandoraId": "PE:1", "duration": 3600, "shareableUrlPath": "/podcast/x/PE:1"},
                "PC:9": {"name": "Crime Junkie", "pandoraId": "PC:9", "type": "PC"}
            },
            "podcastProgramDetails": {
                "pandoraId": "PC:9",
                "recentEpisodes": ["PE:1", {"pandoraId": "PE:2", "episodeName": "Older", "duration": 90}, null]
            }
        }
    });
    let show = json!({
        "continueListeningEpisode": "PE:2",
        "details": { "annotations": {
            "PC:9": { "name": "Crime Junkie", "publisherName": "audiochuck", "episodeCount": 400,
                      "icon": { "artUrl": "https://example.test/cj.jpg" } },
            "PE:1": { "name": "Older", "ordering": 1, "type": "PE" },
            "PE:2": { "name": "Newer", "ordering": 9, "type": "PE" }
        }}
    });
    let show = parse_show_page(&show, "PC:9", "Crime Junkie");
    assert!(show[0].title.contains("Crime Junkie"), "{}", show[0].title);
    assert!(show[0].title.contains("audiochuck") && show[0].title.contains("400"), "{}", show[0].title);
    assert_eq!(show[0].items[0].pandora_id, "PE:2");
    assert!(show[0].items[0].name.starts_with("Continue"));
    assert_eq!(show[0].items[1].pandora_id, "PE:1");
    let sections = parse_podcast_details(&v, "PC:9", "Crime Junkie");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title, "Crime Junkie");
    assert_eq!(sections[0].items.len(), 2);
    assert_eq!(sections[0].items[0].pandora_id, "PE:1");
    assert_eq!(sections[0].items[0].name, "Latest episode");
    assert_eq!(sections[0].items[0].detail, "1h 0m");
    assert!(sections[0].items[0].link.is_none(), "a website path must not steal Enter from playback");
    assert_eq!(sections[0].items[1].name, "Older");
    assert_eq!(sections[0].items[1].detail, "2 min");
    let only_notes = json!({
        "annotations": { "PE:4": {"episodeName": "From the notes", "pandoraId": "PE:4"} }
    });
    let sections = parse_podcast_details(&only_notes, "PC:9", "");
    assert_eq!(sections[0].title, "PC:9");
    assert_eq!(sections[0].items[0].name, "From the notes");
    let catalog = json!({
        "annotations": {
            "PE:7": {"name": "Latest from the catalog", "pandoraId": "PE:7", "duration": 64, "programName": "Dateline"}
        },
        "podcastProgramDetails": { "pandoraId": "PC:9", "recentEpisodes": ["PE:7"] }
    });
    let sections = parse_podcast_details(&catalog, "PC:9", "Dateline");
    assert_eq!(sections[0].items[0].name, "Latest from the catalog");
    assert_eq!(sections[0].items[0].pandora_id, "PE:7");
    assert_eq!(sections[0].items[0].detail, "2 min");
}
