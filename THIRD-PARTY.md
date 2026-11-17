# Drittbibliotheken

Jede Abhängigkeit wird vor der Aufnahme auf Lizenz geprüft (`cargo deny check licenses`, Konfiguration in `deny.toml`) und hier eingetragen.

| Crate | Version | Lizenz | Zweck | Aufgenommen (Meilenstein) |
|---|---|---|---|---|
| `flate2` | 1.1.11 | MIT OR Apache-2.0 | zlib (Dateitabelle, Einträge), Rust-Backend `miniz_oxide` | M1 |
| `lzma-rust2` | 0.25.0 | Apache-2.0 | LZMA-Einträge lesen (M1) und kodieren (M2, Feature `encoder`) | M1/M2 |
| `zopfli` | 0.8.4 | Apache-2.0 | zlib-kompatible Kompression mit höherem Aufwand (`--method zopfli`) | M2 |
| `sha2` | 0.11.1 | MIT OR Apache-2.0 | SHA-256 für Inhalts-Dedupe und Rückprüfung beim Repack | M2 |
| `unicode-normalization` | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | Dateinamen von macOS (NFD) beim Bauen in NFC bringen | M2 |
| `crc32fast` | 1.5.5 | MIT OR Apache-2.0 | CRC32 | M1 |
| `md-5` | 0.11.0 | MIT OR Apache-2.0 | MD5 | M1 |
| `rayon` | 1.12.0 | MIT OR Apache-2.0 | parallele Verarbeitung | M1 |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | Fehlertypen | M1 |
| `encoding_rs` | 0.8.44 | (Apache-2.0 OR MIT) AND BSD-3-Clause | CP949-Namen (Korea) anzeigen/suchen | M1 |
| `clap` | 4.6.7 | MIT OR Apache-2.0 | CLI | M1 |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | JSON-Ausgabe | M1 |
| `anyhow` | 1.0.104 | MIT OR Apache-2.0 | Fehlerbehandlung in der CLI | M1 |

Stand 2026-11-16: `cargo deny check` (Lizenzen, Verbote, Quellen, Advisories) ohne Befund. Transitive Abhängigkeiten stehen in `Cargo.lock`; erlaubte Lizenzen in `deny.toml`.
