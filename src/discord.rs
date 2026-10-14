use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::header::{
    HeaderMap, HeaderValue, AUTHORIZATION, RETRY_AFTER, USER_AGENT,
};
use reqwest::{RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tokio::time::sleep;

use crate::cache::CacheEntry;

const API_BASE: &str = "https://discord.com/api/v10";
const MAX_RATE_LIMIT_RETRIES: usize = 5;

pub struct DiscordClient {
    http: reqwest::Client,
}

impl DiscordClient {
    pub fn new(token: &str, auth_prefix: Option<&str>) -> Result<Self> {
        let authorization = authorization_value(token, auth_prefix);

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization).context("invalid Discord authorization header")?,
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("discord-search-cli/0.1.0"),
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build Discord HTTP client")?;

        Ok(Self { http })
    }

    pub async fn fetch_entries(
        &self,
        include_channels: bool,
        include_dms: bool,
    ) -> Result<Vec<CacheEntry>> {
        let mut entries = Vec::new();

        if include_channels {
            entries.extend(self.fetch_guild_channel_entries().await?);
        }

        if include_dms {
            match self.fetch_dm_entries().await {
                Ok(dm_entries) => entries.extend(dm_entries),
                Err(error) => eprintln!("Skipping DMs: {error:#}"),
            }
        }

        Ok(entries)
    }

    async fn fetch_guild_channel_entries(&self) -> Result<Vec<CacheEntry>> {
        let guilds = self.fetch_current_user_guilds().await?;
        let mut entries = Vec::new();

        for guild in guilds {
            let channels = match self.fetch_guild_channels(&guild.id).await {
                Ok(channels) => channels,
                Err(error) => {
                    eprintln!(
                        "Skipping guild {} ({}): {error:#}",
                        guild.name, guild.id
                    );
                    continue;
                }
            };

            for channel in channels.into_iter().filter(Channel::is_openable_guild_channel) {
                let name = channel.name.unwrap_or_else(|| channel.id.clone());
                let title = format!("#{name}");
                let subtitle = guild.name.clone();
                entries.push(CacheEntry {
                    id: channel.id.clone(),
                    kind: "channel".to_string(),
                    title,
                    subtitle: subtitle.clone(),
                    url: format!("https://discord.com/channels/{}/{}", guild.id, channel.id),
                    match_text: format!("{subtitle} {name} #{name}"),
                });
            }
        }

        Ok(entries)
    }

    async fn fetch_current_user_guilds(&self) -> Result<Vec<Guild>> {
        let mut guilds = Vec::new();
        let mut after: Option<String> = None;

        loop {
            let mut request = self
                .http
                .get(format!("{API_BASE}/users/@me/guilds"))
                .query(&[("limit", "200")]);
            if let Some(after_id) = &after {
                request = request.query(&[("after", after_id)]);
            }

            let page: Vec<Guild> = request
                .send_json_or_discord_error("current user guilds")
                .await?;

            if page.is_empty() {
                break;
            }

            after = page.last().map(|guild| guild.id.clone());
            let page_len = page.len();
            guilds.extend(page);

            if page_len < 200 {
                break;
            }
        }

        Ok(guilds)
    }

    async fn fetch_guild_channels(&self, guild_id: &str) -> Result<Vec<Channel>> {
        self.http
            .get(format!("{API_BASE}/guilds/{guild_id}/channels"))
            .send_json_or_discord_error("guild channels")
            .await
    }

    async fn fetch_dm_entries(&self) -> Result<Vec<CacheEntry>> {
        let channels: Vec<Channel> = self
            .http
            .get(format!("{API_BASE}/users/@me/channels"))
            .send_json_or_discord_error("DM channels")
            .await?;

        Ok(channels
            .into_iter()
            .filter(Channel::is_dm_channel)
            .map(|channel| {
                let title = channel.dm_title();
                CacheEntry {
                    id: channel.id.clone(),
                    kind: "dm".to_string(),
                    title: title.clone(),
                    subtitle: "Direct Message".to_string(),
                    url: format!("https://discord.com/channels/@me/{}", channel.id),
                    match_text: format!("dm direct message {title}"),
                }
            })
            .collect())
    }
}

trait DiscordRequestExt {
    async fn send_json_or_discord_error<T: DeserializeOwned>(self, label: &str) -> Result<T>;
}

impl DiscordRequestExt for RequestBuilder {
    async fn send_json_or_discord_error<T: DeserializeOwned>(self, label: &str) -> Result<T> {
        for attempt in 0..=MAX_RATE_LIMIT_RETRIES {
            let request = self
                .try_clone()
                .context("failed to clone Discord request for retry")?;
            let response = request
                .send()
                .await
                .with_context(|| format!("failed to request Discord {label}"))?;
            let status = response.status();
            let headers = response.headers().clone();

            if status == StatusCode::TOO_MANY_REQUESTS {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|error| format!("<failed to read response body: {error}>"));

                if attempt < MAX_RATE_LIMIT_RETRIES {
                    let delay = rate_limit_delay(&headers, &body);
                    eprintln!(
                        "Rate limited on Discord {label}; retrying in {:.2}s ({}/{})",
                        delay.as_secs_f64(),
                        attempt + 1,
                        MAX_RATE_LIMIT_RETRIES
                    );
                    sleep(delay).await;
                    continue;
                }

                bail!("Discord rejected {label} request: HTTP {status}: {body}");
            }

            return response.json_or_discord_error(label).await;
        }

        unreachable!("rate limit retry loop must return or bail")
    }
}

trait DiscordResponseExt {
    async fn json_or_discord_error<T: DeserializeOwned>(self, label: &str) -> Result<T>;
}

impl DiscordResponseExt for reqwest::Response {
    async fn json_or_discord_error<T: DeserializeOwned>(self, label: &str) -> Result<T> {
        let status = self.status();
        if !status.is_success() {
            let body = self
                .text()
                .await
                .unwrap_or_else(|error| format!("<failed to read response body: {error}>"));
            bail!("Discord rejected {label} request: HTTP {status}: {body}");
        }

        self.json()
            .await
            .with_context(|| format!("failed to parse Discord {label} response"))
    }
}

fn rate_limit_delay(headers: &HeaderMap, body: &str) -> Duration {
    let seconds = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| {
            headers
                .get("x-ratelimit-reset-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<f64>().ok())
        })
        .or_else(|| {
            serde_json::from_str::<RateLimitBody>(body)
                .ok()
                .and_then(|body| body.retry_after)
        })
        .unwrap_or(2.0);

    Duration::from_secs_f64(seconds.max(0.25) + 0.25)
}

#[derive(Debug, Deserialize)]
struct RateLimitBody {
    retry_after: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Guild {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Channel {
    id: String,
    #[serde(rename = "type")]
    channel_type: u8,
    name: Option<String>,
    recipients: Option<Vec<User>>,
}

impl Channel {
    fn is_openable_guild_channel(&self) -> bool {
        matches!(self.channel_type, 0 | 5 | 10 | 11 | 12 | 15)
    }

    fn is_dm_channel(&self) -> bool {
        matches!(self.channel_type, 1 | 3)
    }

    fn dm_title(&self) -> String {
        if let Some(name) = &self.name {
            if !name.trim().is_empty() {
                return name.clone();
            }
        }

        let recipients = self
            .recipients
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(User::display_name)
            .collect::<Vec<_>>();

        if recipients.is_empty() {
            format!("DM {}", self.id)
        } else {
            recipients.join(", ")
        }
    }
}

#[derive(Debug, Deserialize)]
struct User {
    username: String,
    global_name: Option<String>,
}

impl User {
    fn display_name(&self) -> String {
        self.global_name
            .as_ref()
            .filter(|name| !name.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| self.username.clone())
    }
}

fn authorization_value(token: &str, auth_prefix: Option<&str>) -> String {
    if token.starts_with("Bot ") || token.starts_with("Bearer ") {
        return token.to_string();
    }

    match auth_prefix {
        Some(prefix) if !prefix.trim().is_empty() => format!("{} {}", prefix.trim(), token),
        _ => token.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn raw_token_is_sent_unchanged_without_a_prefix() {
        assert_eq!(authorization_value("abc.def", None), "abc.def");
    }

    #[test]
    fn prefix_is_prepended_and_trimmed() {
        assert_eq!(authorization_value("abc", Some("Bot")), "Bot abc");
        assert_eq!(authorization_value("abc", Some("  Bearer ")), "Bearer abc");
    }

    #[test]
    fn blank_prefix_is_ignored() {
        assert_eq!(authorization_value("abc", Some("  ")), "abc");
    }

    #[test]
    fn token_that_already_has_a_scheme_is_not_prefixed_again() {
        assert_eq!(authorization_value("Bot abc", Some("Bearer")), "Bot abc");
        assert_eq!(authorization_value("Bearer abc", Some("Bot")), "Bearer abc");
    }

    #[test]
    fn rate_limit_delay_prefers_retry_after_header() {
        let map = headers(&[("retry-after", "1.5"), ("x-ratelimit-reset-after", "9")]);
        let delay = rate_limit_delay(&map, r#"{"retry_after": 7.0}"#);
        assert_eq!(delay, Duration::from_secs_f64(1.75));
    }

    #[test]
    fn rate_limit_delay_falls_back_to_reset_after_header() {
        let map = headers(&[("x-ratelimit-reset-after", "3")]);
        let delay = rate_limit_delay(&map, r#"{"retry_after": 7.0}"#);
        assert_eq!(delay, Duration::from_secs_f64(3.25));
    }

    #[test]
    fn rate_limit_delay_falls_back_to_body() {
        let delay = rate_limit_delay(&HeaderMap::new(), r#"{"retry_after": 4.0}"#);
        assert_eq!(delay, Duration::from_secs_f64(4.25));
    }

    #[test]
    fn rate_limit_delay_defaults_when_nothing_is_usable() {
        let map = headers(&[("retry-after", "soon")]);
        assert_eq!(
            rate_limit_delay(&map, "not json"),
            Duration::from_secs_f64(2.25)
        );
        assert_eq!(
            rate_limit_delay(&HeaderMap::new(), "{}"),
            Duration::from_secs_f64(2.25)
        );
    }

    #[test]
    fn rate_limit_delay_has_a_minimum() {
        let map = headers(&[("retry-after", "0")]);
        assert_eq!(rate_limit_delay(&map, ""), Duration::from_secs_f64(0.5));
    }

    fn channel(json: &str) -> Channel {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn guild_channel_types_that_can_be_opened() {
        for kind in [0, 5, 10, 11, 12, 15] {
            let c = channel(&format!(r#"{{"id":"1","type":{kind},"name":"x"}}"#));
            assert!(c.is_openable_guild_channel(), "type {kind}");
            assert!(!c.is_dm_channel(), "type {kind}");
        }
    }

    #[test]
    fn voice_stage_and_category_channels_are_skipped() {
        for kind in [2, 4, 13] {
            let c = channel(&format!(r#"{{"id":"1","type":{kind},"name":"x"}}"#));
            assert!(!c.is_openable_guild_channel(), "type {kind}");
        }
    }

    #[test]
    fn dm_and_group_dm_are_dm_channels() {
        for kind in [1, 3] {
            let c = channel(&format!(r#"{{"id":"1","type":{kind}}}"#));
            assert!(c.is_dm_channel(), "type {kind}");
            assert!(!c.is_openable_guild_channel(), "type {kind}");
        }
    }

    #[test]
    fn dm_title_uses_group_name_when_set() {
        let c = channel(r#"{"id":"9","type":3,"name":"Book club","recipients":[{"username":"a"}]}"#);
        assert_eq!(c.dm_title(), "Book club");
    }

    #[test]
    fn dm_title_prefers_global_name_and_falls_back_to_username() {
        let c = channel(
            r#"{"id":"9","type":3,"recipients":[
                {"username":"alice","global_name":"Alice A"},
                {"username":"bob","global_name":null},
                {"username":"carol","global_name":"  "}
            ]}"#,
        );
        assert_eq!(c.dm_title(), "Alice A, bob, carol");
    }

    #[test]
    fn dm_title_ignores_blank_name() {
        let c = channel(r#"{"id":"9","type":1,"name":"  ","recipients":[{"username":"dave"}]}"#);
        assert_eq!(c.dm_title(), "dave");
    }

    #[test]
    fn dm_title_without_recipients_uses_the_id() {
        let c = channel(r#"{"id":"9","type":1}"#);
        assert_eq!(c.dm_title(), "DM 9");
    }
}
