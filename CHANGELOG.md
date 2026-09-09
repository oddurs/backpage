# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Frame composition: titled panels stacked into a fixed character grid, with
  truncation and padding that keep every line exactly the frame width.
- Samples read from `poptop --once`, with figures the platform does not
  publish dropped rather than shown as em dashes.
- Rasterising a frame to a PNG with a system monospace font, and setting it as
  the macOS desktop picture.
- Type sized to fill the display and the block centred, so the dashboard fits
  the screen it lands on.
- `--interval` to repaint continuously, and a launchd agent in `contrib/`.
- `--stdout`, `--source`, `--font`, `--size`, `--margin`, `--bg`, `--fg`,
  `--screen`, `--cols`, `--rows`.

- `--stream`, which runs the source's full-screen interface on a
  pseudo-terminal and draws its screen, with truecolor and the 256-colour
  palette resolved per cell.
- Font fallback, so characters the primary font lacks — notably the braille
  block poptop's graphs are drawn with — are taken from another font.
- Per-cell background colours, so highlighted and inverse-video cells read the
  way they do in a terminal.

### Changed

- `--width`/`--height` are now `--cols`/`--rows`, since the picture has pixel
  dimensions of its own.

[unreleased]: https://github.com/oddurs/backpage/commits/main
