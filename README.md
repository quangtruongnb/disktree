# disk-tree

A macOS terminal UI (TUI) disk space scanner built with Rust and [Ratatui](https://ratatui.rs).

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/quangtruongnb/disktree/main/install.sh | sh
```

## Features

- Recursively scans directories and displays disk usage as an interactive tree
- Fast parallel scanning with Rayon
- Move files/folders to Trash
- Keyboard-driven navigation

## Build from Source

Requires macOS and Rust 1.70+.

```sh
cargo install --path .
```

## Usage

```sh
# Scan home directory (default)
disk-tree

# Scan a specific directory
disk-tree /path/to/dir
```

## Keybindings

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate |
| `Enter` | Expand/collapse |
| `d` | Move to Trash |
| `q` | Quit |

## Dependencies

- [ratatui](https://github.com/ratatui-org/ratatui) — TUI framework
- [crossterm](https://github.com/crossterm-rs/crossterm) — terminal backend
- [walkdir](https://github.com/BurntSushi/walkdir) — directory traversal
- [rayon](https://github.com/rayon-rs/rayon) — parallel scanning
- [clap](https://github.com/clap-rs/clap) — CLI argument parsing
- [bytesize](https://github.com/hyunsik/bytesize) — human-readable sizes

## License

MIT
