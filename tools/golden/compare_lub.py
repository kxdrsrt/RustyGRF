#!/usr/bin/env python3
"""Vergleicht die Dekompilierung von GRF Editors GrfCL (-lub, Referenz) mit `rgrf lua decompile`.

usage: compare_lub.py <GrfCL-Ausgabe> <unsere Ausgabe> <extrahierte .lub>
  1. tools/golden/run-lub-in-vm.sh '0 OTHER CLIENTS\\client-kro\\data.grf' kro   (GrfCL in der VM)
  2. rgrf extract <data.grf> SRC '*.lub'; je Datei: rgrf lua decompile SRC/x.lub -o OURS/x.lua
Verglichen wird der Inhalt als Tokenfolge, ohne Kommentare, Klammern, Kommas, `else`/`end`, mit
gleichgesetzten Vergleichsseiten (`9 < x` = `x > 9`, `nil == x` = `x == nil`), `["a"]` = `a` und mit
CP949 statt dem Mojibake von GRF Editor. Doppelte Schluessel in Tabellen fasst GRF Editor zusammen,
wir geben alle Zuweisungen aus: solche Dateien zaehlen als verschieden, sind aber gleichwertig.
"""
import os, re, sys, difflib, collections
ref, ours, src = sys.argv[1:4]
def walk(root):
    d={}
    for dp,_,fs in os.walk(root):
        for f in fs:
            if f.endswith(".lua"):
                p=os.path.join(dp,f); d[os.path.relpath(p,root).lower()]=p
    return d
R=walk(ref); O=walk(ours)
def read(p): return open(p,encoding="utf-8",errors="replace").read()
def demoji(t):
    out=bytearray()
    for ch in t:
        o=ord(ch)
        if o<256: out.append(o)
        else:
            try: out+=ch.encode("cp1252")
            except Exception: out+=ch.encode("utf-8")
    try: return out.decode("cp949")
    except Exception: return t
def flipcmp(t):
    f={"<":">","<=":">=",">":"<",">=":"<="}
    return re.sub(r'(\b\d+\.?\d*|"(?:[^"\\]|\\.)*")\s*(<=|>=|<|>)\s*([A-Za-z_][\w.]*)',lambda m:f"{m.group(3)} {f[m.group(2)]} {m.group(1)}",t)
def norm(t):
    t=flipcmp(t)
    t=re.sub(r'\b(nil|true|false)\s*(==|~=)\s*([A-Za-z_][\w.]*)',r'\3 \2 \1',t)
    t=re.sub(r'\[\s*"([A-Za-z_]\w*)"\s*\]',r'\1',t)
    lines=[l for l in t.splitlines() if not l.strip().startswith("--")]
    t="\n".join(lines)
    t=re.sub(r"--\[\[.*?\]\]","",t,flags=re.S)
    toks=re.findall(r'"(?:[^"\\]|\\.)*"|[A-Za-z_][A-Za-z_0-9]*|\d+\.?\d*|~=|==|<=|>=|\.\.\.?|\S',t)
    return [x for x in toks if x not in ("else","end","(",")",",",";")]
def canon(toks):
    # a < b with a literal: flip to b > a
    out=list(toks); i=0
    flip={"<":">","<=":">=",">":"<",">=":"<="}
    for i in range(1,len(out)-1):
        pass
    return out
def is5051(path):
    d=open(path,"rb").read(5)
    return d[4] if len(d)>4 and d[:4]==b"\x1bLua" else 0
srcmap={os.path.relpath(os.path.join(dp,f),src).lower()[:-4]:os.path.join(dp,f) for dp,_,fs in os.walk(src) for f in fs if f.endswith(".lub")}
cats=collections.Counter(); ratios=[]; worst=[]
for k,rp in R.items():
    key=k[:-4]
    if key not in srcmap: continue
    ver=is5051(srcmap[key])
    tag="5.0" if ver==0x50 else "5.1"
    if ver==0: cats["plain text (copied)"]+=1; continue
    if k not in O: cats["ours missing"]+=1; continue
    a=norm(demoji(read(rp))); b=norm(read(O[k]))
    if "-- not decompiled" in read(O[k]): kind="partial"
    else: kind="full"
    if a==b: cats[f"identical tokens ({kind}, Lua {tag})"]+=1; continue
    ca,cb=collections.Counter(a),collections.Counter(b); r=sum((ca&cb).values())*2/max(1,len(a)+len(b))
    cats[f"different ({kind}, Lua {tag})"]+=1; ratios.append(r); worst.append((r,k,len(a),len(b)))
for k,v in sorted(cats.items()): print(v,k)
worst.sort()
print("ratio of differing: min %.3f median %.3f"%(worst[0][0],sorted(ratios)[len(ratios)//2]) if worst else "")
for w in worst[:8]: print("%.3f %s ref=%d ours=%d"%w)
print("--- local declarations: files where counts differ (full files only)")
bad=0
for k,rp in R.items():
    key=k[:-4]
    if key not in srcmap or is5051(srcmap[key]) not in (0x50,0x51) or k not in O: continue
    ot=read(O[k])
    if "-- not decompiled" in ot: continue
    rl=len(re.findall(r"\blocal\b",demoji(read(rp)))); ol=len(re.findall(r"\blocal\b",ot))
    if rl!=ol:
        bad+=1; print(k.split("/")[-1],"ref",rl,"ours",ol)
print("files with different local counts:",bad)
