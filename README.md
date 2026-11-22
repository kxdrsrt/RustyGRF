# RustyGRF

One tool for **Ragnarok Online** GRF (Gravity Resource File) archives, sprites, animations, palettes and maps, on macOS, Windows and Linux.

RustyGRF is a desktop app, a command-line tool (`rgrf`) and an MCP server (`rgrf-mcp`) built on the same Rust libraries. It opens, checks, edits, builds, merges and encrypts GRF, GPF, Thor and RGZ archives, and shows and edits the files inside them.

> **Status: 0.1.0, beta.** Everything below is implemented and tested, mostly against real kRO client archives (`data.grf` with 202,358 entries). Not everything has been tried in the game yet; see [Status and limits](#status-and-limits).

## Features

**Archives**
- Read, list, search, verify, extract, build, repack, merge, diff and patch archives. GRF 0x102, 0x103, 0x200 and 0x300 (read and write), GPF, GSF, Thor and RGZ.
- Compression: zlib (works in every client), Zopfli (smaller, still zlib) and LZMA (needs an LZMA-capable `cps.dll`).
- Names are shown as Korean (CP949) by default, or as Latin-1 / UTF-8.
- Encrypt and decrypt single entries or the file table with GRF Editor's password scheme.
- Edit an archive in place with undo/redo, a crash-safe draft, and verified saves (the result is written next to the archive, checked, then swapped in).

**Files inside archives**
- **Sprites (SPR):** preview, edit pixels, replace or insert frames, convert indexed ⇄ RGBA, export and import PNG.
- **Animations (ACT):** preview with GIF export, an editor for layers, anchors, sounds, frames and actions, in-betweens, mirror/reverse, scripts, "Setup Headgear/Head/Garment", new ACT from templates.
- **Palettes (PAL):** view, edit, hue/saturation/brightness, preview a sprite with another palette.
- **Maps:** GAT, GND, RSW, RSM/RSM2 with a 3D preview, extract a map with every file it needs, downgrade maps for older clients, find missing files.
- **Lua:** syntax check, compile and decompile `.lub` (Lua 5.0 and 5.1), search inside compiled files.
- **Audio:** play and edit WAV, MP3 and OGG (trim, cut, fade, gain, resample, effects).
- **Rare formats:** BSON ⇄ JSON, EBM emblems, ICO, IMF, FNA, `Thumbs.db`, STR effects.
- **Client checks:** `DATA.INI` priority, hidden files, server address, missing resources (including names from Lua tables), translation update.

**App**
- Tree, details, folders and gallery views, search by name or pattern, filters, tabs and multiple windows.
- Dark and light theme, four accent colours, nine languages (English, German, Spanish, French, Indonesian, Japanese, Korean, Brazilian Portuguese, Filipino; all but English are machine drafts).
- A native menu bar, file associations for `.grf`, `.gpf`, `.gsf`, `.thor` and `.rgz`, and an update check against the latest GitHub release.

## Download

Installers and command-line archives are attached to each [release](../../releases/latest):

| System | App | Command line |
| --- | --- | --- |
| macOS (Apple Silicon) | `.dmg` | `rustygrf-cli-macos-arm64.tar.gz` |
| Windows x64 | `.msi` or `-setup.exe` | `rustygrf-cli-windows-x64.tar.gz` |
| Windows ARM64 | `-setup.exe` | `rustygrf-cli-windows-arm64.tar.gz` |
| Linux x64 | `.AppImage` or `.deb` | `rustygrf-cli-linux-x64.tar.gz` |
| Linux ARM64 | `.AppImage` or `.deb` | `rustygrf-cli-linux-arm64.tar.gz` |

**The installers are not code-signed** (no paid Apple or Microsoft developer account), so the system warns on first start:
- **macOS:** right-click the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/RustyGRF.app`.
- **Windows:** SmartScreen → *More info* → *Run anyway*.

Intel Macs are not built; build from source instead.

## Command line

```sh
rgrf info  data.grf                          # version, size, entry counts
rgrf ls    data.grf "*.lub" --long           # search and list
rgrf verify data.grf                         # check the table and decode every entry
rgrf extract data.grf ./out "data/sprite*"   # never overwrites without --overwrite
rgrf build ./out -o new.grf --method lzma --dedupe
rgrf merge custom.grf data.grf -o merged.grf # the FIRST archive wins, like DATA.INI
rgrf diff a.grf b.grf                        # differences by path and content
rgrf patch old.grf new.grf -o update.grf     # only what is new or different
rgrf --help                                  # act, spr, pal, map, lua, client, thor, rgz, ...
```

Every command can print machine-readable output with `--json`. See [docs/04-cli-and-mcp.md](docs/04-cli-and-mcp.md) (German) for all commands.

## MCP server

`rgrf-mcp` lets an AI agent inspect and build archives over stdio. It is read-only unless you pass `--allow-write <folder>`:

```json
{
  "servers": {
    "rustygrf": {
      "type": "stdio",
      "command": "/path/to/rgrf-mcp",
      "args": ["--allow-write", "/path/to/output-folder"]
    }
  }
}
```

Passwords for encrypted archives are read from `RGRF_PASSWORD` or `RGRF_KEY_FILE`, never from a tool argument.

## Build from source

You need a current stable Rust toolchain.

```sh
cargo build --release                 # rgrf and rgrf-mcp
cargo test --release                  # tests with real archives run when RGRF_TESTDATA points at them

cd apps/gui                           # the desktop app (Tauri 2)
npm ci
npx tauri build                       # installers for your system
```

On Linux the app needs `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev` and `libxdo-dev`. Details: [docs/11-release.md](docs/11-release.md).

## Status and limits

- Reading agrees with GRF Editor on every entry of the kRO archives (over 950,000 entries, identical hashes on macOS, Linux and Windows). Archives RustyGRF writes are read by GRF Editor without differences, and the LZMA variant runs in the dev client.
- Gravity's own protection (flag 0x80, the 0x300 key block) is not supported: it cannot be read without Gravity's keys. GRF Editor's password encryption is supported.
- Tested on macOS (Apple Silicon), Linux ARM64 and Windows ARM64 and x64. Linux x64, the AppImage and the Windows installers on real hardware are built by CI but not tried by hand.
- Several client tools (resource checker, translation update, the new sprite and animation edits) are measured on files but not yet checked in the running game.
- The installers are unsigned and the translations are machine drafts.

The documentation in `docs/` is in German, with the milestones, measurements and decisions: [architecture](docs/01-architecture.md), [formats](docs/02-format-notes.md) and [09-formats](docs/09-formats.md), [roadmap](docs/03-roadmap.md), [CLI and MCP](docs/04-cli-and-mcp.md), [feature comparison with GRF Editor](docs/10-feature-matrix.md), [release notes for packagers](docs/11-release.md), [localization](docs/12-localization.md). Measurements against GRF Editor's `GrfCL` are in [BENCHMARKS.md](BENCHMARKS.md).

## Thanks

- **[Tokeiburu](https://github.com/Tokeiburu)** for GRF Editor and ACT Editor: the source code to study, and the inspiration. RustyGRF is not a fork.
- **Curiosity** for LessGRF and the LZMA extension of `cps.dll` ([curio-r/rag-cps-dll](https://github.com/curio-r/rag-cps-dll)).
- **[Lua](https://www.lua.org/)** 5.0 and 5.1 (MIT), whose compiler is ported for checking and compiling Lua files.
- **[Tauri](https://github.com/tauri-apps/tauri)** for the window around the app.

More in [NOTICE](NOTICE) and [THIRD-PARTY.md](THIRD-PARTY.md).

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

RustyGRF is not affiliated with Gravity Co., Ltd. It contains no game data: no archives, sprites or textures.
