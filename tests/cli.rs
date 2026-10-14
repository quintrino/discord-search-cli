//! End-to-end tests that run the built binary. They never touch the network:
//! `refresh` is only exercised on its no-token failure path.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const CACHE: &str = r##"{
  "fetched_at": "2026-01-02T03:04:05Z",
  "entries": [
    {"id": "1", "kind": "channel", "title": "#general", "subtitle": "Alpha",
     "url": "https://discord.com/channels/10/1", "match_text": "Alpha general #general"},
    {"id": "2", "kind": "channel", "title": "#random", "subtitle": "Alpha",
     "url": "https://discord.com/channels/10/2", "match_text": "Alpha random #random"},
    {"id": "3", "kind": "dm", "title": "Sam", "subtitle": "Direct Message",
     "url": "https://discord.com/channels/@me/3", "match_text": "dm direct message Sam"}
  ]
}"##;

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_discord-search-cli"))
        .args(["--config-path"])
        .arg(dir.join("config.json"))
        .args(["--cache-path"])
        .arg(dir.join("cache.json"))
        .args(args)
        .env_remove("DISCORD_TOKEN")
        .output()
        .unwrap()
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

fn titles(value: &Value) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["title"].as_str().unwrap())
        .collect()
}

#[test]
fn search_filters_the_cache_and_prints_json() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("cache.json"), CACHE).unwrap();

    let output = run(dir.path(), &["search", "alpha gen"]);

    assert!(output.status.success());
    let value = stdout_json(&output);
    assert_eq!(titles(&value), ["#general"]);
    assert_eq!(value["items"][0]["arg"], "https://discord.com/channels/10/1");
    assert_eq!(value["items"][0]["valid"], true);
}

#[test]
fn search_without_a_query_lists_everything() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("cache.json"), CACHE).unwrap();

    let output = run(dir.path(), &["search"]);

    assert_eq!(titles(&stdout_json(&output)), ["#general", "#random", "Sam"]);
}

#[test]
fn search_with_no_match_prints_an_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("cache.json"), CACHE).unwrap();

    let output = run(dir.path(), &["search", "zzz"]);

    assert!(output.status.success());
    assert!(titles(&stdout_json(&output)).is_empty());
}

#[test]
fn search_without_a_cache_prints_an_error_item_and_succeeds() {
    let dir = tempfile::tempdir().unwrap();

    let output = run(dir.path(), &["search", "general"]);

    assert!(output.status.success());
    let value = stdout_json(&output);
    assert_eq!(titles(&value), ["Discord cache unavailable"]);
    assert_eq!(value["items"][0]["valid"], false);
    assert!(value["items"][0]["subtitle"]
        .as_str()
        .unwrap()
        .contains("Run refresh first"));
}

#[test]
fn search_with_a_corrupt_cache_prints_an_error_item() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("cache.json"), "not json").unwrap();

    let output = run(dir.path(), &["search"]);

    assert!(output.status.success());
    assert_eq!(titles(&stdout_json(&output)), ["Discord cache unavailable"]);
}

#[test]
fn cache_path_prints_the_override() {
    let dir = tempfile::tempdir().unwrap();

    let output = run(dir.path(), &["cache-path"]);

    let printed = String::from_utf8(output.stdout).unwrap();
    assert_eq!(printed.trim(), dir.path().join("cache.json").to_str().unwrap());
}

#[test]
fn refresh_without_a_token_fails_with_a_helpful_message() {
    let dir = tempfile::tempdir().unwrap();

    let output = run(dir.path(), &["refresh"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Missing Discord token"), "{stderr}");
    assert!(!dir.path().join("cache.json").exists());
}

#[test]
fn version_flag_reports_the_package_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_discord-search-cli"))
        .arg("--version")
        .output()
        .unwrap();

    let printed = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        printed.trim(),
        format!("discord-search-cli {}", env!("CARGO_PKG_VERSION"))
    );
}
