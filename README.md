# discord-search-cli

Rust CLI that searches cached Discord channels and DMs, then opens the selected entry as a Discord URL.

The CLI keeps search and refresh separate: `search` only reads cached entries, and `refresh` is the only command that calls the Discord API.

## Commands

```sh
cargo run -- search "general"
cargo run -- refresh
cargo run -- cache-path
cargo run -- open "https://discord.com/channels/@me/123"
```

For a launcher or script that reads JSON results, run:

```sh
/path/to/discord-search-cli search "{query}"
```

Then open the selected item with `discord-search-cli open "<arg>"`, or any URL opener, using the item's `arg`.

## Token

The CLI reads `DISCORD_TOKEN` or accepts `--token`.

Tokens are only needed for `refresh`. `search` does not call Discord and can run without a token.

By default the token is sent as-is, which is what raw user tokens need:

```sh
DISCORD_TOKEN="..." discord-search-cli refresh
discord-search-cli --token "..." refresh
```

For bot tokens, add the `Bot` prefix:

```sh
discord-search-cli --auth-prefix Bot --token "..." refresh
```

For bearer tokens, use:

```sh
discord-search-cli --auth-prefix Bearer --token "..." refresh
```

If the token already starts with `Bot ` or `Bearer `, it is used as-is. Global flags are safest before the subcommand, as shown above.

## Config

Default config path on macOS:

```text
~/Library/Application Support/discord-search-cli/config.json
```

Example:

```json
{
  "include_channels": true,
  "include_dms": true
}
```

CLI flags override config values where present.

Useful flags:

```sh
--cache-path /tmp/discord-search-cli-cache.json
--config-path ./config.json
--auth-prefix Bot|Bearer
--no-dms
--no-channels
```

## Cache

Default cache path on macOS:

```text
~/Library/Application Support/discord-search-cli/cache.json
```

Cache writes are atomic: the refreshed data is written to a temp file first, then renamed over the old cache.

`search` never refreshes. If no cache exists, it prints an error item telling you to run `refresh`.

## Rate Limits

Discord may return `429 Too Many Requests` while refreshing many guilds. The CLI respects Discord's `retry_after`, `retry-after`, and `x-ratelimit-reset-after` values, waits, and retries up to five times before giving up on that request.

## Discord Coverage

The CLI fetches:

- Guilds from `GET /users/@me/guilds`
- Guild channels from `GET /guilds/{guild.id}/channels`
- DMs from `GET /users/@me/channels`

Discord’s officially documented app/bot APIs do not expose every user-client DM scenario equally. DM caching may require a raw user/OAuth-style token or may fail depending on token type and Discord access rules.

## JSON output

`search` prints JSON with an `items` array:

```json
{
  "items": [
    {
      "uid": "discord-channel-123",
      "title": "#general",
      "subtitle": "Server Name",
      "arg": "https://discord.com/channels/456/123",
      "match": "Server Name general #general",
      "valid": true
    }
  ]
}
```

## Install

```sh
brew tap quintrino/tap
brew install quintrino/tap/discord-search-cli
```

Prebuilt binaries for Apple silicon, Intel macOS and x86_64 Linux are attached to each GitHub release.

## Development

```sh
cargo test
```
