# LinkRouter

A tiny macOS default-browser shim that opens each link in the right Chrome profile.

Set it as your default browser. When you click a link, macOS starts LinkRouter.
It checks a small rules file, opens the link in the matching Chrome profile,
brings Chrome to the front, and quits. Nothing stays running in the background.

## Install

Requires macOS 12+, Rust and Google Chrome.

```sh
./install.sh
```

The script builds a release binary and puts `~/Applications/LinkRouter.app` in
place with an ad-hoc signature. It registers the app with Launch Services and
creates `~/.config/linkrouter/rules` from `rules.example` if you don't have one yet.

Then select **LinkRouter** under System Settings → Desktop & Dock → Default web browser.

## Rules

`~/.config/linkrouter/rules` is read on every click, so edits apply right away.

```ini
# [section] = Chrome profile directory (chrome://version → "Profile Path")
[Profile 3]
opener com.tinyspeck.slackmacgap   # any link clicked in Slack
domain acme.io                     # acme.io and every subdomain
prefix github.com/acme-corp        # this org and everything under it
```

- The first matching rule wins.
- Unmatched links open in Chrome's last active window, just as if Chrome were the default browser.
- `domain` matches the host itself or any subdomain. It never matches a lookalike such as `notacme.io` or `acme.io.evil.com`.
- `prefix` ignores the scheme, `www.` and case, and only matches at a path boundary: `acme-corp` does not match `acme-corpo`.
- To find an app's bundle id for `opener`, run `osascript -e 'id of app "Slack"'`.

Check a rule without opening anything:

```sh
~/Applications/LinkRouter.app/Contents/MacOS/LinkRouter route https://github.com/acme-corp/repo
~/Applications/LinkRouter.app/Contents/MacOS/LinkRouter route https://example.com com.tinyspeck.slackmacgap
```

To turn on the debug log, run `touch ~/Library/Logs/LinkRouter.log`. Every routed link is
then appended to it. Delete the file to turn it off. The log holds full URLs, which can
include tokens, so don't leave it on.

LinkRouter only passes on `http`, `https` and `file` URLs. Anything else is dropped, so a
crafted "URL" can never reach Chrome's command line as a switch.

## How it works

- **Profile hand-off:** LinkRouter runs Chrome's own binary with `--profile-directory=<dir> <url>`. Chrome's single-instance logic forwards the URL to the running browser.
- **Focus:** Chrome is activated before and after the hand-off, so the target profile's window ends up in front.
- **Opener detection:** the app that sent the link comes from the sender PID of the `GURL` Apple event.

## Performance

Measured on an Apple Silicon Mac running macOS 15.

| | LinkRouter | Finicky (for comparison) |
|---|---|---|
| App size | 420 KB binary | 56 MB app |
| Memory while idle | 0 (not running) | ~14 MB RSS, resident |
| Rules check (`route` CLI, cold) | ~10 ms, 3 MB RSS | — |
| Click → link in profile window | ~0.3 s, mostly Chrome's hand-off | ~0.9 s cold start when not resident |

Release builds use LTO, `panic=abort` and stripped symbols. The only dependencies
are the `objc2` AppKit/Foundation bindings, with default features turned off.

## Development

```sh
cargo test          # rule parsing and matching
./install.sh        # rebuild and reinstall the app
```

## AI disclosure

This project was written with [Claude Code](https://claude.com/claude-code)
(Anthropic's Claude) working with a human author, who directed, tested and
reviewed it. That includes the code, tests and this README.

## License

MIT, see [LICENSE](LICENSE).
