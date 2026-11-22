//! `rgrf client translate ...`: updates a client's text tables from the RO Translation Project
//! (the Rust version of `tools/translation/*.py`; see [`rgrf_formats::translate`]).

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Subcommand;
use rgrf_formats::translate::{self, GrfTool, assemble, fill, install, mods, personal, update};
use serde_json::json;

#[derive(Subcommand)]
pub enum TranslateCmd {
    /// Puts the project's folders together in the order of its ClientGenerator.bat (`grf/` for the
    /// archive, `loose/` for files next to the client)
    Assemble {
        /// A checkout of https://github.com/llchrisll/ROenglishRE
        repo: PathBuf,
        /// pre (pre-renewal server) or re (renewal server)
        #[arg(long, value_parser = ["pre", "re"])]
        mode: String,
        /// Client date from the generator's menu, e.g. 2018-06-20
        #[arg(long)]
        client: String,
        /// Output folder (must not exist)
        #[arg(short, long)]
        out: PathBuf,
        /// Drop files of your own server (path inside the archive, `/`, case-insensitive, `*` `?`)
        #[arg(long)]
        exclude: Vec<String>,
    },
    /// Fills entries of the client's text tables that are still Korean with the English entry of
    /// the same key from the translation (stateiconinfo, skilldescript, map and card name tables,
    /// tipoftheday, hotkey names, item names and descriptions). The client is only read
    Fill {
        /// The client folder (with DATA.INI)
        #[arg(long)]
        full: PathBuf,
        /// A checkout of ROenglishRE
        #[arg(long)]
        roen: PathBuf,
        /// Where the fixed archive files go (with their path inside the archive)
        #[arg(long)]
        out: PathBuf,
        /// Where the fixed loose files go (System/itemInfo_*.lub); without it they are not fixed
        #[arg(long)]
        out_loose: Option<PathBuf>,
        /// The rgrf program that reads the archives (default: this one)
        #[arg(long)]
        rgrf: Option<PathBuf>,
    },
    /// Carries your own edits over to a newer translation entry by entry (msgstringtable.txt,
    /// skillinfolist.lub, skilldescript.lub)
    Personal {
        /// A FULL clone of ROenglishRE (git fetch --unshallow)
        repo: PathBuf,
        #[arg(long)]
        client_dev: PathBuf,
        /// The output of `assemble` of the new translation
        #[arg(long)]
        asm: PathBuf,
        /// Output folder (must not exist)
        #[arg(short, long)]
        out: PathBuf,
        /// The date your GRFs were last built (YYYY-MM-DD)
        #[arg(long, default_value = "9999-12-31")]
        built_before: String,
        /// Always keep your SkillName of this skill (e.g. NPC_EARTHQUAKE)
        #[arg(long)]
        keep_skill: Vec<String>,
        /// The longest text (before #) still counted as your abbreviation
        #[arg(long, default_value_t = 3)]
        max_short: usize,
    },
    /// Three-way merge of your unpacked GRF folders with the newest translation and add-ons
    Mods {
        /// A FULL clone of ROenglishRE
        repo: PathBuf,
        #[arg(long)]
        client_dev: PathBuf,
        /// The output of `assemble`
        #[arg(long)]
        asm: PathBuf,
        /// Output folder (must not exist)
        #[arg(short, long)]
        out: PathBuf,
        /// Archives below client-dev/grf to look at
        #[arg(long, num_args = 1.., default_values_t = ["core".to_string(), "cdata".to_string()])]
        archives: Vec<String>,
        #[arg(long, default_value = "9999-12-31")]
        built_before: String,
        /// Share of differing lines above which a file counts as unrelated to the project
        #[arg(long, default_value_t = 0.5)]
        max_ratio: f64,
        /// A data path that is never touched
        #[arg(long)]
        exclude: Vec<String>,
    },
    /// The whole update of core.grf and cdata.grf: mods + personal + the plain translation, merged
    /// into new archives (nothing in client-dev is changed)
    Update {
        /// A FULL clone of ROenglishRE
        repo: PathBuf,
        /// The output of `assemble`
        #[arg(long)]
        asm: PathBuf,
        #[arg(long)]
        client_dev: PathBuf,
        /// Output folder (must not exist)
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value = "9999-12-31")]
        built_before: String,
        /// A data path that is never touched (server files etc.)
        #[arg(long)]
        exclude: Vec<String>,
        /// Always keep your SkillName of this skill
        #[arg(long)]
        keep_skill: Vec<String>,
        /// A folder (archive root, with data/) of your own files for core.grf, highest priority
        #[arg(long)]
        extra_core: Option<PathBuf>,
        /// The rgrf program that builds and merges (default: this one)
        #[arg(long)]
        rgrf: Option<PathBuf>,
    },
    /// Puts the result of `update` into client-dev: backup first, a dry run unless --apply
    Install {
        /// The output folder of `update`
        #[arg(long)]
        update: PathBuf,
        /// The output of `assemble` (for loose/SystemEN)
        #[arg(long)]
        asm: PathBuf,
        #[arg(long)]
        client_dev: PathBuf,
        /// Backup folder (must not exist when applying)
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        rgrf: Option<PathBuf>,
        /// Really write
        #[arg(long)]
        apply: bool,
    },
}

fn tool(rgrf: &Option<PathBuf>) -> Result<GrfTool> {
    match rgrf {
        Some(p) => Ok(GrfTool::new(p)),
        None => GrfTool::current().map_err(|e| anyhow!("{e}")),
    }
}

fn fail(e: translate::Error) -> anyhow::Error {
    anyhow!("{e}")
}

/// Runs one `client translate` command; the lines the scripts printed go to stdout (a list in
/// `--json`).
pub fn run(action: &TranslateCmd, json: bool) -> Result<i32> {
    let mut lines: Vec<String> = Vec::new();
    let mut say = |l: &str| {
        if !json {
            println!("{l}");
        }
        lines.push(l.to_string());
    };
    match action {
        TranslateCmd::Assemble {
            repo,
            mode,
            client,
            out,
            exclude,
        } => {
            let o = assemble::Options {
                pre_renewal: mode == "pre",
                client: client.clone(),
                out: out.clone(),
                exclude: exclude.clone(),
            };
            assemble::assemble(repo, &o, &mut say).map_err(fail)?;
        }
        TranslateCmd::Fill {
            full,
            roen,
            out,
            out_loose,
            rgrf,
        } => {
            let o = fill::Options {
                full: full.clone(),
                roen: roen.clone(),
                out: out.clone(),
                out_loose: out_loose.clone(),
            };
            fill::run(&tool(rgrf)?, &o, &mut say).map_err(fail)?;
        }
        TranslateCmd::Personal {
            repo,
            client_dev,
            asm,
            out,
            built_before,
            keep_skill,
            max_short,
        } => {
            let mut o =
                personal::Options::new(repo.clone(), client_dev.clone(), asm.clone(), out.clone());
            o.built_before.clone_from(built_before);
            o.keep_skill.clone_from(keep_skill);
            o.max_short = *max_short;
            say(&personal::run(&o).map_err(fail)?);
        }
        TranslateCmd::Mods {
            repo,
            client_dev,
            asm,
            out,
            archives,
            built_before,
            max_ratio,
            exclude,
        } => {
            let mut o =
                mods::Options::new(repo.clone(), client_dev.clone(), asm.clone(), out.clone());
            o.archives.clone_from(archives);
            o.built_before.clone_from(built_before);
            o.max_ratio = *max_ratio;
            o.exclude.clone_from(exclude);
            say(&mods::run(&o).map_err(fail)?.1);
        }
        TranslateCmd::Update {
            repo,
            asm,
            client_dev,
            out,
            built_before,
            exclude,
            keep_skill,
            extra_core,
            rgrf,
        } => {
            let o = update::Options {
                repo: repo.clone(),
                asm: asm.clone(),
                client_dev: client_dev.clone(),
                out: out.clone(),
                built_before: built_before.clone(),
                exclude: exclude.clone(),
                keep_skill: keep_skill.clone(),
                extra_core: extra_core.clone(),
            };
            say(&update::run(&tool(rgrf)?, &o).map_err(fail)?);
        }
        TranslateCmd::Install {
            update,
            asm,
            client_dev,
            backup,
            rgrf,
            apply,
        } => {
            let o = install::Options {
                update: update.clone(),
                asm: asm.clone(),
                client_dev: client_dev.clone(),
                backup: backup.clone(),
                apply: *apply,
            };
            install::run(&tool(rgrf)?, &o, &mut say).map_err(fail)?;
        }
    }
    if json {
        println!(
            "{}",
            json!({"schema_version": crate::SCHEMA_VERSION, "lines": lines})
        );
    }
    Ok(0)
}
