# backpage

[![CI](https://github.com/oddurs/backpage/actions/workflows/ci.yml/badge.svg)](https://github.com/oddurs/backpage/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Turn the macOS desktop wallpaper into a read-only TUI dashboard.

The idea: the desktop behind your windows is the largest permanently visible
surface on the machine, and it shows a photograph. `backpage` renders a
terminal-style dashboard there instead — glanceable, never interactive, never
stealing focus.

## Status

Early. What works today:

- Composing a dashboard frame from titled panels, at any size, with truncation
  and padding that keep every line exactly the frame width.
- A CLI that renders one frame to stdout.

What does not exist yet:

- Writing the frame to the desktop picture.
- Any real data source — the panel contents are placeholders.
- A daemon, a refresh loop, or multi-display support.

The roadmap below is a statement of intent, not of current behaviour.

## Requirements

- macOS on Apple silicon
- Rust 1.98 or newer (the toolchain is pinned in `rust-toolchain.toml`)

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
backpage                              # render an 80x24 frame to stdout
backpage --width 120 --height 40      # render at a specific size
backpage --help
backpage --version
```

```
$ backpage --width 40 --height 4
┌ backpage ────────────────────────────┐
│ no data sources configured           │
│ run `backpage --help` for usage      │
└──────────────────────────────────────┘
```

Any height beyond what the panels need is padded with blank rows, so the frame
is always exactly the size you asked for.

## Roadmap

1. Render a frame to a PNG and set it as the desktop picture.
2. A refresh loop with a configurable interval.
3. Data sources: system load, battery, calendar, git status.
4. Multi-display and per-space wallpapers.
5. A config file describing which panels appear where.

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
