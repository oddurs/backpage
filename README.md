# backpage

[![CI](https://github.com/oddurs/backpage/actions/workflows/ci.yml/badge.svg)](https://github.com/oddurs/backpage/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Turn the macOS desktop wallpaper into a read-only TUI dashboard.

The idea: the desktop behind your windows is the largest permanently visible
surface on the machine, and it shows a photograph. `backpage` renders a
terminal-style dashboard there instead — glanceable, never interactive, never
stealing focus.

## Status

Early, but it does the thing. What works today:

- Reading a sample from [poptop](https://github.com/oddurs/poptop) via its
  `--once` mode, dropping the figures the platform does not publish.
- Composing that into panels, rasterising them to a PNG with a system
  monospace font, and setting it as the desktop picture.
- Sizing the type to fill the display and centring the result, so the dashboard
  fits whatever screen it lands on.
- Repainting on an interval.

What does not exist yet:

- Any data source other than poptop.
- Multi-display and per-space pictures — every desktop gets the same picture.
- A config file. Everything is flags.

## Requirements

- macOS on Apple silicon
- Rust 1.98 or newer (the toolchain is pinned in `rust-toolchain.toml`)
- [poptop](https://github.com/oddurs/poptop) on `PATH`, or pointed at with
  `--source`

The first run asks for permission to control System Events, which is how the
desktop picture gets set. Granting it once is enough.

## Install

```sh
git clone https://github.com/oddurs/backpage.git
cd backpage
scripts/setup
cargo build --release
```

The binary lands at `target/release/backpage`. To put it on your `PATH`:

```sh
cargo install --path .
```

## Usage

```sh
backpage                      # paint the desktop once and exit
backpage --interval 10        # repaint every 10 seconds
backpage --stdout             # print the frame instead of painting
backpage --help
```

`--stdout` is the quickest way to see what will be drawn:

```
$ backpage --stdout --cols 92 --rows 9
┌ system ──────────────────────────────────────────────────────────────────────────────────┐
│ cpu     38.0%  (14 cores)                                                                │
│ fs      91.7%  / full, 38.3G of 460.4G available                                         │
│ mem     83.0%  19.9G / 24.0G used, 10.1G available                                       │
│ swap    65.4%  2.6G / 4.0G                                                               │
│ load    4.13 5.20 5.60                                                                   │
│ procs   601                                                                              │
│ io     173/601 processes unreadable — run as root to see them                            │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

By default the type is sized to fill the display and the block is centred.
`--size`, `--cols` and `--rows` override that; `--bg` and `--fg` take
`#rrggbb`.

### Keeping it up to date

Run it on an interval under launchd, so it survives logout and restarts:

```sh
cp contrib/com.oddurs.backpage.plist ~/Library/LaunchAgents/
launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.oddurs.backpage.plist
```

It must be bootstrapped into the `gui/` domain. Setting the desktop picture
goes through System Events, which refuses a background context with
`Connection is invalid (-609)`. While the display is locked or asleep the
`AppleEvent` cannot complete either — backpage reports the timeout and retries
on the next tick rather than exiting.

To stop it, and put your own picture back:

```sh
launchctl bootout "gui/$(id -u)/com.oddurs.backpage"
osascript -e 'tell application "System Events" to set picture of every desktop to "/path/to/your.png"'
```

## Roadmap

1. More data sources — battery, calendar, git status — alongside poptop.
2. Multi-display and per-space pictures.
3. A config file describing which panels appear where.
4. Redrawing only when the sample actually changed.

## Development

Every check runs through one entry point, so local runs and CI cannot drift:

```sh
scripts/task check     # fmt:check, lint, test, build — what CI runs
scripts/task fmt       # format in place
```

Work happens on a branch in its own worktree, never in the primary checkout:

```sh
scripts/agent start feat/wallpaper-writer   # creates the branch and worktree
cd ../.worktrees/backpage/feat/wallpaper-writer
scripts/agent commit "feat(wallpaper): write frames to the desktop picture"
scripts/agent pr
scripts/agent done                          # after the PR is merged
```

`scripts/agent doctor` reports anything that is not set up correctly. See
[CONTRIBUTING.md](CONTRIBUTING.md) for the full workflow.

## License

MIT — see [LICENSE](LICENSE).
