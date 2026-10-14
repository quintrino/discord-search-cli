use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cli::Cli;

#[derive(Debug, Clone)]
pub struct Settings {
    pub token: Option<String>,
    pub auth_prefix: Option<String>,
    pub cache_path: PathBuf,
    pub include_channels: bool,
    pub include_dms: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FileConfig {
    token: Option<String>,
    auth_prefix: Option<String>,
    cache_path: Option<PathBuf>,
    include_channels: Option<bool>,
    include_dms: Option<bool>,
}

impl Settings {
    pub fn load(cli: &Cli) -> Result<Self> {
        let config_path = cli.config_path.clone().unwrap_or_else(default_config_path);
        let file = read_config(config_path)?;
        // Raw (user) tokens are the default; a prefix is only sent when asked for.
        let auth_prefix = if cli.raw_token {
            None
        } else {
            cli.auth_prefix.clone().or_else(|| file.auth_prefix.clone())
        };

        Ok(Self {
            token: cli.token.clone().or_else(|| file.token.clone()),
            auth_prefix,
            cache_path: cli
                .cache_path
                .clone()
                .or_else(|| file.cache_path.clone())
                .unwrap_or_else(default_cache_path),
            include_channels: file.include_channels.unwrap_or(!cli.no_channels),
            include_dms: file.include_dms.unwrap_or(!cli.no_dms),
        })
    }
}

fn read_config(path: PathBuf) -> Result<FileConfig> {
    if !path.exists() {
        return Ok(FileConfig {
            token: None,
            auth_prefix: None,
            cache_path: None,
            include_channels: None,
            include_dms: None,
        });
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config at {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse config at {}", path.display()))
}

fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("discord-search-cli")
}

fn default_config_path() -> PathBuf {
    app_data_dir().join("config.json")
}

fn default_cache_path() -> PathBuf {
    app_data_dir().join("cache.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().unwrap(),
            }
        }

        fn config_path(&self) -> PathBuf {
            self.dir.path().join("config.json")
        }

        fn write_config(&self, json: &str) {
            fs::write(self.config_path(), json).unwrap();
        }

        fn load(&self, extra: &[&str]) -> Result<Settings> {
            let config_path = self.config_path();
            let mut args = vec!["discord-search-cli", "--config-path", config_path.to_str().unwrap()];
            args.extend_from_slice(extra);
            args.push("cache-path");
            Settings::load(&Cli::try_parse_from(args).unwrap())
        }
    }

    #[test]
    fn defaults_send_the_token_as_is() {
        let settings = Fixture::new().load(&[]).unwrap();

        assert_eq!(settings.auth_prefix, None);
        assert!(settings.include_channels);
        assert!(settings.include_dms);
    }

    #[test]
    fn cli_token_is_used() {
        let settings = Fixture::new().load(&["--token", "abc"]).unwrap();

        assert_eq!(settings.token.as_deref(), Some("abc"));
    }

    #[test]
    fn cli_token_overrides_the_config_file_token() {
        let fixture = Fixture::new();
        fixture.write_config(r#"{"token": "from-file"}"#);

        let settings = fixture.load(&["--token", "from-cli"]).unwrap();
        assert_eq!(settings.token.as_deref(), Some("from-cli"));
    }

    #[test]
    fn auth_prefix_flag_is_used() {
        let settings = Fixture::new().load(&["--auth-prefix", "Bot"]).unwrap();

        assert_eq!(settings.auth_prefix.as_deref(), Some("Bot"));
    }

    #[test]
    fn auth_prefix_comes_from_the_config_file() {
        let fixture = Fixture::new();
        fixture.write_config(r#"{"auth_prefix": "Bearer"}"#);

        let settings = fixture.load(&[]).unwrap();

        assert_eq!(settings.auth_prefix.as_deref(), Some("Bearer"));
    }

    #[test]
    fn auth_prefix_flag_overrides_the_config_file() {
        let fixture = Fixture::new();
        fixture.write_config(r#"{"auth_prefix": "Bearer"}"#);

        let settings = fixture.load(&["--auth-prefix", "Bot"]).unwrap();

        assert_eq!(settings.auth_prefix.as_deref(), Some("Bot"));
    }

    #[test]
    fn raw_token_flag_drops_any_prefix() {
        let fixture = Fixture::new();
        fixture.write_config(r#"{"auth_prefix": "Bot"}"#);

        let settings = fixture.load(&["--raw-token", "--auth-prefix", "Bearer"]).unwrap();

        assert_eq!(settings.auth_prefix, None);
    }

    #[test]
    fn cache_path_flag_overrides_the_config_file() {
        let fixture = Fixture::new();
        fixture.write_config(r#"{"cache_path": "/from/file.json"}"#);

        let from_file = fixture.load(&[]).unwrap();
        let from_flag = fixture.load(&["--cache-path", "/from/flag.json"]).unwrap();

        assert_eq!(from_file.cache_path, PathBuf::from("/from/file.json"));
        assert_eq!(from_flag.cache_path, PathBuf::from("/from/flag.json"));
    }

    #[test]
    fn default_cache_path_is_in_the_app_data_dir() {
        let settings = Fixture::new().load(&[]).unwrap();

        assert!(settings.cache_path.ends_with("discord-search-cli/cache.json"));
    }

    #[test]
    fn no_channels_and_no_dms_flags_turn_them_off() {
        let settings = Fixture::new().load(&["--no-channels", "--no-dms"]).unwrap();

        assert!(!settings.include_channels);
        assert!(!settings.include_dms);
    }

    #[test]
    fn config_file_can_turn_off_channels_and_dms() {
        let fixture = Fixture::new();
        fixture.write_config(r#"{"include_channels": false, "include_dms": false}"#);

        let settings = fixture.load(&[]).unwrap();

        assert!(!settings.include_channels);
        assert!(!settings.include_dms);
    }

    #[test]
    fn invalid_config_file_is_an_error_naming_the_path() {
        let fixture = Fixture::new();
        fixture.write_config("{ nope");

        let error = fixture.load(&[]).unwrap_err();

        assert!(format!("{error:#}").contains("failed to parse config"));
    }
}
