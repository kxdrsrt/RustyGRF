#!/bin/bash
# Laedt Test-Dateien Dritter (nur lokal, nicht einchecken): Thor und GRF 0x102/0x103/0x200 aus dem Repository
# L1nkZ/rpatchur, GPF aus fishdeity/iRO-Missing_Features_Additions. Braucht `gh`. Ziel: ~/build/rustygrf-fixtures
# (oder der erste Parameter). Danach: rgrf verify <datei> (Thor prueft auch data.integrity).
set -euo pipefail
OUT="${1:-$HOME/build/rustygrf-fixtures}"
mkdir -p "$OUT"
get() { gh api "repos/$1/contents/$2" --jq .content | base64 -d > "$OUT/$3"; }
for f in dir1 dir2 empty small tiny; do get L1nkZ/rpatchur "gruf/resources/tests/thor/$f.thor" "$f.thor"; done
for v in 102 103 200; do for k in empty small; do get L1nkZ/rpatchur "gruf/resources/tests/grf/$v-$k.grf" "$v-$k.grf"; done; done
for f in bgm_missing_added big_card big_other_items card_art_missing_added card_enchant_prefixes; do
  get fishdeity/iRO-Missing_Features_Additions "$f.gpf" "$f.gpf"
done
echo "geladen nach $OUT"
