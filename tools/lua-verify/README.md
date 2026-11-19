# lua-verify

Prüfwerkzeug für den Lua-Dekompilierer und den Lua-Compiler von `rgrf-formats` gegen **echtes** Lua
(5.1 über `mlua`, 5.0 aus `fetch-lua50.sh`). Eigener Cargo-Workspace (braucht `unsafe`), wird nicht
mit ausgeliefert. Ergebnisse: [docs/09-formats.md](../../docs/09-formats.md).

```
cd tools/lua-verify
CARGO_TARGET_DIR=~/build/rustygrf-luaverify cargo build --release
./fetch-lua50.sh                         # optional: Lua 5.0.3 nach ~/build/lua50 (für die 5.0-Dateien)
B=~/build/rustygrf-luaverify/release/rgrf-lua-verify
$B data.grf                              # Parser, Verhaltensvergleich (Original gegen dekompilierten Text)
$B data.grf --parse-only | --dump-only   # nur Parser / nur Rundlauf von luac::dump
$B data.grf --compile-equal              # unser Compiler gegen Lua 5.1.5, Byte für Byte
LV_OURS=1 $B data.grf --recompile        # Dekompilieren, mit unserem Compiler zurück, Code gleich dem Original?
$B data.grf --encodings                  # welche Zeichensätze die Strings der Dateien haben
$B --fuzz 100000 [startwert]             # Zufallsprogramme: unser Compiler gegen Lua 5.1.5
$B data.grf --compile-equal50            # Lua 5.0: luacomp50 gegen das echte Lua 5.0.3 (62 Dateien der kRO-data.grf)
$B --fuzz50 1000000 [startwert]          # Lua 5.0: Zufallsprogramme (1 von 8 mit Syntaxfehler: gleiche Bytes oder gleiche Meldung)
$B --garbage50 1000000 [startwert]       # Lua 5.0: stark beschädigte Programme und Zufallsbytes (auch: kein Absturz)
$B --limits50                            # Lua 5.0: Programme an den Grenzen (Ebenen, Variablen, Register, Konstanten, Sprünge)
```
Umgebungsvariablen: `LV_ONLY=<namensteil>`, `LV_TRACE`, `LV_SHOW` (Ausgaben zeigen), `LV_MUTATE`
(Selbsttest: verfälscht den Text absichtlich), `LV_NOHOOK`, `LV_STEP=load|main`, `LUA50=<pfad>`,
`RILUAC=<luac-Programm>` (für `--recompile` mit einem fremden Compiler), `LV_NOMUT` und `LV_SHOW=1|2` (nur `--fuzz50`: keine Syntaxfehler erzeugen, Fehlerarten zählen/Beispiele zeigen). Die 5.0-Modi starten das echte Lua 5.0.3 mit `-e` und reichen die Quelltexte über stdin/stdout durch (keine temporären Dateien); `LV_MUTATE=1` verfälscht dort unseren Compiler absichtlich (Selbsttest). Exit-Code 1 bei Abweichung.
