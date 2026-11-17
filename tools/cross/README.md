# Tests auf anderen Systemen (vom Mac aus)

Gemessen am 2026-11-17 (siehe `docs/03-roadmap.md`, M5). Nichts davon ist für den normalen Bau nötig.

**Linux (ARM64, Docker):** Quellen ohne `.git`/`.cargo`/`target` nach `~/build/linux-src` kopieren, dann

```sh
docker run --rm -v ~/build/linux-src:/src:ro -v "<RO/1 CLIENT-SIDE>":/data:ro \
  -v rgrf-target:/target -v rgrf-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target \
  -e RGRF_TESTDATA=/data -e RGRF_REQUIRE_TESTDATA=1 -e RGRF_GOLDEN_FULL=1 -w /src rust:1 \
  cargo test --release --workspace --no-fail-fast
```

**Windows (ARM64 und x64, Parallels-VM):** Kreuz-Bauen mit `cargo-xwin` auf dem Mac (lädt die Windows-SDK von Microsoft; Zielordner unter `~/build/win`, damit die VM ihn als `\\Mac\Home\build\win` sieht):

```sh
export CARGO_TARGET_DIR=$HOME/build/win
cargo xwin build --release --target aarch64-pc-windows-msvc      # oder x86_64-pc-windows-msvc
cargo xwin test  --release --no-run --target aarch64-pc-windows-msvc
cp tools/cross/run-tests-in-vm.ps1 ~/build/win/run-tests.ps1
prlctl exec "<VM>" 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' -NoProfile \
  -ExecutionPolicy Bypass -File '\\Mac\Home\build\win\run-tests.ps1' -triple aarch64-pc-windows-msvc
```

Das Skript spiegelt das Repo und die Binärdateien unter denselben Pfaden nach `C:\Users\k\...` (die Tests tragen `CARGO_MANIFEST_DIR` und `CARGO_BIN_EXE_*` als feste Mac-Pfade), führt jede Test-Datei aus und schreibt `~/build/win/test-log-<triple>.txt`. Alte Test-Programme vorher löschen (`find ~/build/win -path "*release/build/*/out/*.exe" -mmin +3 -delete`). Pfade und Benutzername (`k`) im Skript anpassen.
