use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Cache {
    pub fetched_at: DateTime<Utc>,
    pub entries: Vec<CacheEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheEntry {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub url: String,
    pub match_text: String,
}

impl Cache {
    pub fn new(mut entries: Vec<CacheEntry>) -> Self {
        entries.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then(left.subtitle.cmp(&right.subtitle))
                .then(left.title.cmp(&right.title))
        });

        Self {
            fetched_at: Utc::now(),
            entries,
            warning: None,
        }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read cache at {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("failed to parse cache at {}", path.display()))
    }

    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create cache directory {}", parent.display()))?;
        }

        let tmp_path = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&tmp_path, text)
            .with_context(|| format!("failed to write temp cache {}", tmp_path.display()))?;
        fs::rename(&tmp_path, path)
            .with_context(|| format!("failed to replace cache {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_round_trips_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let cache = Cache::new(vec![CacheEntry {
            id: "1".to_string(),
            kind: "channel".to_string(),
            title: "#general".to_string(),
            subtitle: "Server".to_string(),
            url: "https://discord.com/channels/1/2".to_string(),
            match_text: "server general".to_string(),
        }]);

        cache.write_atomic(&path).unwrap();
        let loaded = Cache::read(&path).unwrap();

        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].title, "#general");
    }

    fn entry(kind: &str, subtitle: &str, title: &str) -> CacheEntry {
        CacheEntry {
            id: title.to_string(),
            kind: kind.to_string(),
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            url: String::new(),
            match_text: String::new(),
        }
    }

    #[test]
    fn new_sorts_by_kind_then_subtitle_then_title() {
        let cache = Cache::new(vec![
            entry("dm", "Direct Message", "zed"),
            entry("channel", "B Server", "#a"),
            entry("channel", "A Server", "#z"),
            entry("channel", "A Server", "#b"),
        ]);

        let order: Vec<_> = cache.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(order, ["#b", "#z", "#a", "zed"]);
    }

    #[test]
    fn warning_is_omitted_from_json_when_none() {
        let cache = Cache::new(vec![]);
        let text = serde_json::to_string(&cache).unwrap();

        assert!(!text.contains("warning"));
    }

    #[test]
    fn cache_without_warning_field_can_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        fs::write(
            &path,
            r#"{"fetched_at":"2026-01-02T03:04:05Z","entries":[]}"#,
        )
        .unwrap();

        let loaded = Cache::read(&path).unwrap();

        assert!(loaded.entries.is_empty());
        assert!(loaded.warning.is_none());
    }

    #[test]
    fn read_reports_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let error = Cache::read(&dir.path().join("nope.json")).unwrap_err();

        assert!(format!("{error:#}").contains("failed to read cache"));
    }

    #[test]
    fn read_reports_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        fs::write(&path, "not json").unwrap();

        let error = Cache::read(&path).unwrap_err();

        assert!(format!("{error:#}").contains("failed to parse cache"));
    }

    #[test]
    fn write_creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("cache.json");

        Cache::new(vec![]).write_atomic(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn write_replaces_old_cache_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        Cache::new(vec![entry("dm", "Direct Message", "old")])
            .write_atomic(&path)
            .unwrap();

        Cache::new(vec![entry("dm", "Direct Message", "new")])
            .write_atomic(&path)
            .unwrap();

        let loaded = Cache::read(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].title, "new");
        assert!(!path.with_extension("json.tmp").exists());
    }
}
