# RustyGRF benchmarks

Measured on 2026-11-17. Short, honest numbers for later reference (showcase, README, release notes).
The project documentation is otherwise German; this file is English on purpose.

**Method.** `rgrf` (RustyGRF, Rust) against `GrfCL.exe` from GRF Editor (Tokeiburu, .NET), both in the **same Windows 11 ARM64 VM**
(Parallels, 6 vCPU, 6 GB RAM) on a **local disk**, with the **same inputs**. Wall time of the whole process (start-up
included) and peak working set. The order of the tools alternates from run to run so neither always runs first or on a
warm cache. Small jobs: 3 runs, median shown. Big jobs: 1–2 runs. Every output of both tools was read back with
`rgrf verify` (0 errors) and compared by content where comparable.

**Inputs** (kRO client files, not in the repo): `cdata.grf` 17 MB / 3,973 files · `rdata.grf` 290 MB / 24,756 files ·
`data.grf` 4.2 GB / 202,357 files (16 GB unpacked) · extracted trees of `cdata` and `rdata` for the build jobs.

## Results (VM, Windows ARM64)

| Job | `rgrf` | `GrfCL` | Output size | Peak RAM `rgrf` / `GrfCL` |
|---|---|---|---|---|
| Open + info, `data.grf` (4.2 GB) | **0.08–0.13 s** | 1.2–1.4 s | – | 42 / 260 MB |
| Extract `cdata` (103 MB) | **2.3 s** | 4.1 s | identical | 20 / 145 MB |
| Extract `rdata` (1.2 GB) | 25.5 s | 26.3 s | identical | 59 / 370 MB |
| Extract `data.grf` (16 GB) | 243 s or 385 s¹ | 314 s / 316 s | identical | 202 / 1,138 MB |
| Build `cdata`, zlib 9 | **1.0 s** | 2.1 s | 17,009,178 vs 17,012,174 B | 77 / 146 MB |
| Build `rdata`, zlib 9 | **11.7 s** | 16.8 s | 287,083,129 vs 287,126,228 B | 196 / 541 MB |
| Build `cdata`, LZMA, `rgrf` default (preset 9) | 10.5 s | **8.5 s** | **13.11 vs 13.55 MB (−3.3 %)** | 147 / 151 MB |
| Build `cdata`, LZMA, `rgrf --level 6` | **6.6 s** | 8.5 s | **13.28 vs 13.55 MB (−2.1 %)** | 128 / 151 MB |
| Build `rdata`, LZMA, `rgrf` default | 127 s | **74 s** | **233.6 vs 237.4 MB (−1.6 %)** | 249 / 449 MB |
| Build `rdata`, LZMA, `rgrf --level 6` | **67 s** | 74 s | **235.4 vs 237.4 MB (−0.8 %)** | 273 / 449 MB |
| Merge `rdata` over `data` (streams copied) | 19.2 s (13.0 s without check) | 17.0 s (10.6–19.8) | 4,042,050,709 vs 4,042,057,505 B | 242 / 377 MB |

¹ Depends on the order: 385 s when it ran first, 243 s when it ran second (file cache and write load of the VM). `GrfCL` was
stable at 314–316 s. No winner can be derived for the big extraction.

## Reading the numbers

- **Faster:** opening, small extractions and zlib builds. Start-up of a .NET process is part of the `GrfCL` time.
- **Smaller memory:** 2–6× less than `GrfCL` in every job.
- **Smaller files with LZMA:** −0.8 % to −3.3 %. At the default preset 9 `rgrf` is slower than `GrfCL` (up to 1.7×); at
  `--level 6` it is faster and still smaller. The preset is a time-versus-size dial, not a defect.
- **Not faster everywhere:** big extractions are on par, merges are on par.
- `rgrf` has far fewer features than GRF Editor (no editors, no preview yet, no Thor, no encryption).

## Native macOS (Apple M5 Pro, 15 cores), `rgrf` only

For scale, no comparison tool exists there. Release build, all outputs decoded and compared with the source.

| Job | Time |
|---|---|
| Open `data.grf` (4.2 GB, 202k entries) | 32 ms |
| `verify` of `data.grf` (decode every entry) | 3.0 s |
| Extract `data.grf` (15.1 GiB, 202,357 files) | 11 s |
| `merge` rdata over data (streams copied, 4.04 GB out, with check) | 6.0 s |
| `diff` of two 4–5 GB archives | 13 s |
| Merge of two 0x300 archives (Chris + kRO, 406,675 files) | 78 s (with zlib 9 recompression) |
| Merged GRF (5.7 GB) to 0x200 + LZMA + content dedupe (3.40 GiB, −36.4 %) | 11 min |

## Findings the comparison produced

- `merge` first took 153–186 s against 19 s because it compressed every entry again. It now copies streams unchanged
  unless `--method` is given (6 s on the Mac, 13–19 s in the VM; output 1 % larger than with recompression).
- Output of `GrfCL`'s merge differs from `rgrf`'s in two entries: the empty marker (`rgrf` does not copy markers) and the
  one encrypted entry (`msgstring_kr_s.lub` in `rdata`), which `rgrf` copies byte for byte.

## Limits of these numbers

One VM on one machine; `GrfCL` on Windows ARM64 may run partly emulated (not checked); the GRF Editor GUI was not measured;
big jobs have 1–2 runs only; the VM varies a lot (the same build with check took 10 s to 44 s in a side experiment, cause
not established). Zopfli was not compared (`GrfCL` has none).

## Reproduce

`tools/cross/bench.ps1` (runs inside the VM; needs `rgrf.exe`, `GrfCL.exe` and the archives), raw results as JSON lines in
`~/build/bench/`. Details of the setup: `tools/cross/README.md`, narrative and decisions: `docs/03-roadmap.md`.
