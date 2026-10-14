mod output;
mod cache;
mod cli;
mod config;
mod discord;

use anyhow::{Context, Result};
use clap::Parser;

use crate::output::{to_search_output, to_error_output};
use crate::cache::{Cache, CacheEntry};
use crate::cli::{Cli, Command};
use crate::config::Settings;
use crate::discord::DiscordClient;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let settings = Settings::load(&cli)?;

    match &cli.command {
        Command::Search { query } => {
            let cache = match Cache::read(&settings.cache_path) {
                Ok(cache) => cache,
                Err(error) => {
                    println!(
                        "{}",
                        serde_json::to_string(&to_error_output(format!(
                            "No readable cache at {}. Run refresh first. {error:#}",
                            settings.cache_path.display()
                        )))?
                    );
                    return Ok(());
                }
            };
            let items = filter_entries(&cache.entries, query.as_deref());
            println!("{}", serde_json::to_string(&to_search_output(items))?);
        }
        Command::Refresh => {
            let cache = refresh_cache(&settings).await?;
            eprintln!(
                "Cached {} Discord entries at {}",
                cache.entries.len(),
                cache.fetched_at
            );
        }
        Command::Open { url } => open_url(url)?,
        Command::CachePath => println!("{}", settings.cache_path.display()),
    }

    Ok(())
}

async fn refresh_cache(settings: &Settings) -> Result<Cache> {
    let token = settings
        .token
        .as_deref()
        .context("Missing Discord token. Set DISCORD_TOKEN or pass --token.")?;
    let client = DiscordClient::new(token, settings.auth_prefix.as_deref())?;
    let entries = client
        .fetch_entries(settings.include_channels, settings.include_dms)
        .await?;
    let cache = Cache::new(entries);
    cache.write_atomic(&settings.cache_path)?;
    Ok(cache)
}

fn filter_entries(entries: &[CacheEntry], query: Option<&str>) -> Vec<CacheEntry> {
    let query = query.unwrap_or("").trim().to_lowercase();
    if query.is_empty() {
        return entries.to_vec();
    }

    let tokens: Vec<&str> = query.split_whitespace().collect();
    entries
        .iter()
        .filter(|entry| {
            let haystack = entry.match_text.to_lowercase();
            tokens.iter().all(|token| haystack.contains(token))
        })
        .cloned()
        .collect()
}

fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .status()
            .context("failed to run macOS open command")?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        println!("{url}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, match_text: &str) -> CacheEntry {
        CacheEntry {
            id: name.to_string(),
            kind: "channel".to_string(),
            title: name.to_string(),
            subtitle: "Server".to_string(),
            url: format!("https://discord.com/channels/1/{name}"),
            match_text: match_text.to_string(),
        }
    }

    #[test]
    fn filters_entries_by_all_query_tokens() {
        let entries = vec![
            entry("general", "server general #general"),
            entry("random", "server random #random"),
        ];

        let result = filter_entries(&entries, Some("server gen"));

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "general");
    }

    #[test]
    fn matching_ignores_case_and_token_order() {
        let entries = vec![entry("general", "Server General #general")];

        assert_eq!(filter_entries(&entries, Some("GENERAL server")).len(), 1);
    }

    #[test]
    fn query_is_trimmed_and_whitespace_only_matches_everything() {
        let entries = vec![entry("general", "server general")];

        assert_eq!(filter_entries(&entries, Some("  general  ")).len(), 1);
        assert_eq!(filter_entries(&entries, Some("   ")).len(), 1);
        assert_eq!(filter_entries(&entries, None).len(), 1);
    }

    #[test]
    fn query_with_no_match_returns_nothing() {
        let entries = vec![entry("general", "server general")];

        assert!(filter_entries(&entries, Some("general missing")).is_empty());
    }

    #[test]
    fn empty_query_returns_everything() {
        let entries = vec![entry("general", "server general")];

        assert_eq!(filter_entries(&entries, Some("")).len(), 1);
    }
}
