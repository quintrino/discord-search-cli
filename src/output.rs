use serde::Serialize;

use crate::cache::CacheEntry;

#[derive(Debug, Serialize)]
pub struct SearchOutput {
    items: Vec<SearchItem>,
}

#[derive(Debug, Serialize)]
struct SearchItem {
    uid: String,
    title: String,
    subtitle: String,
    arg: String,
    #[serde(rename = "match")]
    match_text: String,
    valid: bool,
}

pub fn to_search_output(entries: Vec<CacheEntry>) -> SearchOutput {
    SearchOutput {
        items: entries
            .into_iter()
            .map(|entry| SearchItem {
                uid: format!("discord-{}-{}", entry.kind, entry.id),
                title: entry.title,
                subtitle: entry.subtitle,
                arg: entry.url,
                match_text: entry.match_text,
                valid: true,
            })
            .collect(),
    }
}

pub fn to_error_output(message: impl Into<String>) -> SearchOutput {
    SearchOutput {
        items: vec![SearchItem {
            uid: "discord-error".to_string(),
            title: "Discord cache unavailable".to_string(),
            subtitle: message.into(),
            arg: String::new(),
            match_text: "discord cache unavailable error".to_string(),
            valid: false,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, id: &str) -> CacheEntry {
        CacheEntry {
            id: id.to_string(),
            kind: kind.to_string(),
            title: "#general".to_string(),
            subtitle: "Server".to_string(),
            url: format!("https://discord.com/channels/1/{id}"),
            match_text: "Server general #general".to_string(),
        }
    }

    fn json(output: SearchOutput) -> serde_json::Value {
        serde_json::to_value(output).unwrap()
    }

    #[test]
    fn maps_entries_to_script_filter_items() {
        let value = json(to_search_output(vec![entry("channel", "123")]));

        assert_eq!(
            value,
            serde_json::json!({
                "items": [{
                    "uid": "discord-channel-123",
                    "title": "#general",
                    "subtitle": "Server",
                    "arg": "https://discord.com/channels/1/123",
                    "match": "Server general #general",
                    "valid": true
                }]
            })
        );
    }

    #[test]
    fn uid_includes_kind_so_ids_cannot_collide() {
        let value = json(to_search_output(vec![entry("channel", "5"), entry("dm", "5")]));

        assert_eq!(value["items"][0]["uid"], "discord-channel-5");
        assert_eq!(value["items"][1]["uid"], "discord-dm-5");
    }

    #[test]
    fn no_entries_gives_an_empty_item_list() {
        assert_eq!(
            json(to_search_output(vec![])),
            serde_json::json!({ "items": [] })
        );
    }

    #[test]
    fn error_output_is_a_single_invalid_item() {
        let value = json(to_error_output("run refresh"));
        let items = value["items"].as_array().unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["title"], "Discord cache unavailable");
        assert_eq!(items[0]["subtitle"], "run refresh");
        assert_eq!(items[0]["arg"], "");
        assert_eq!(items[0]["valid"], false);
    }
}
