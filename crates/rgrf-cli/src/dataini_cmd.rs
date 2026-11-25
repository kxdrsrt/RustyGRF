//! `rgrf client data-ini`: show and edit the client's `DATA.INI` (see `rgrf_core::dataini`).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use rgrf_core::dataini::{DataIni, Op, Place, locate, write_atomic};
use serde_json::json;

#[derive(Args, Clone)]
pub struct PlaceArgs {
    /// At the top: the highest priority, it wins over everything below (the usual place for a mod archive)
    #[arg(long, conflicts_with_all = ["last", "before", "after", "at"])]
    first: bool,
    /// At the bottom: the lowest priority (default for `add`)
    #[arg(long, conflicts_with_all = ["before", "after", "at"])]
    last: bool,
    /// Directly above this archive
    #[arg(long, value_name = "ARCHIVE", conflicts_with_all = ["after", "at"])]
    before: Option<String>,
    /// Directly below this archive
    #[arg(long, value_name = "ARCHIVE", conflicts_with = "at")]
    after: Option<String>,
    /// At this position of the order, counted from 0
    #[arg(long, value_name = "N")]
    at: Option<usize>,
}

impl PlaceArgs {
    fn place(&self) -> Option<Place> {
        if self.first {
            Some(Place::First)
        } else if self.last {
            Some(Place::Last)
        } else if let Some(n) = &self.before {
            Some(Place::Before(n.clone()))
        } else if let Some(n) = &self.after {
            Some(Place::After(n.clone()))
        } else {
            self.at.map(Place::At)
        }
    }
}

#[derive(Args, Clone)]
pub struct Common {
    /// A client folder (its DATA.INI is used) or the DATA.INI file
    path: PathBuf,
    /// Show the result without writing it
    #[arg(long)]
    dry_run: bool,
    /// Do not keep the file as it was before the first edit (DATA.INI.bak, written once and never replaced)
    #[arg(long)]
    no_backup: bool,
}

#[derive(Subcommand)]
pub enum DataIniCmd {
    /// List the archives in priority order (the first wins); exit 2 when indexes are used twice
    Show {
        /// A client folder or the DATA.INI file
        path: PathBuf,
    },
    /// Add archives (default: at the bottom). The numbering is rewritten one after the other
    Add {
        #[command(flatten)]
        common: Common,
        /// Archives as the client finds them next to its exe, e.g. odin.grf
        #[arg(required = true, value_name = "ARCHIVE")]
        archives: Vec<String>,
        #[command(flatten)]
        place: PlaceArgs,
    },
    /// Take archives out of the list (the files themselves stay)
    Remove {
        #[command(flatten)]
        common: Common,
        #[arg(required = true, value_name = "ARCHIVE")]
        archives: Vec<String>,
    },
    /// Move a listed archive
    Move {
        #[command(flatten)]
        common: Common,
        name: String,
        #[command(flatten)]
        place: PlaceArgs,
    },
    /// Put another archive in the place of a listed one (e.g. data.grf -> data2.grf)
    Replace {
        #[command(flatten)]
        common: Common,
        old: String,
        new: String,
    },
    /// Number the archives one after the other again (closes gaps and duplicate indexes)
    Normalize {
        #[command(flatten)]
        common: Common,
        /// The first index (default: the one the file starts with; the client reads 0 and 1 alike)
        #[arg(long)]
        start: Option<u32>,
    },
}

pub fn run(cmd: &DataIniCmd, json: bool) -> Result<i32> {
    match cmd {
        DataIniCmd::Show { path } => {
            let file = locate(path)?;
            let ini = read(&file)?;
            let dup = ini.duplicate_indexes();
            print_list(&file, &ini, json, false, false, false, &[]);
            if !dup.is_empty() && !json {
                eprintln!("warning: index {dup:?} is used twice; `normalize` closes that");
            }
            Ok(if dup.is_empty() { 0 } else { 2 })
        }
        DataIniCmd::Add {
            common,
            archives,
            place,
        } => edit(
            common,
            &Op::Add {
                names: archives.clone(),
                place: place.place().unwrap_or(Place::Last),
            },
            json,
            archives,
        ),
        DataIniCmd::Remove { common, archives } => edit(
            common,
            &Op::Remove {
                names: archives.clone(),
            },
            json,
            &[],
        ),
        DataIniCmd::Move {
            common,
            name,
            place,
        } => {
            let Some(place) = place.place() else {
                bail!("say where: --first, --last, --before NAME, --after NAME or --at N");
            };
            edit(
                common,
                &Op::Move {
                    name: name.clone(),
                    place,
                },
                json,
                &[],
            )
        }
        DataIniCmd::Replace { common, old, new } => edit(
            common,
            &Op::Replace {
                old: old.clone(),
                new: new.clone(),
            },
            json,
            std::slice::from_ref(new),
        ),
        DataIniCmd::Normalize { common, start } => {
            edit(common, &Op::Normalize { start: *start }, json, &[])
        }
    }
}

fn read(file: &Path) -> Result<DataIni> {
    let bytes = fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
    Ok(DataIni::parse(&bytes))
}

fn edit(c: &Common, op: &Op, json: bool, added: &[String]) -> Result<i32> {
    let file = locate(&c.path)?;
    let old_bytes = fs::read(&file).with_context(|| format!("cannot read {}", file.display()))?;
    let mut ini = DataIni::parse(&old_bytes);
    ini.apply(op)?;
    let new_bytes = ini.to_bytes();
    let changed = new_bytes != old_bytes;
    // archives that are listed but not next to the DATA.INI (the client would not find them)
    let dir = file.parent().unwrap_or(Path::new("."));
    let missing: Vec<String> = added
        .iter()
        .filter(|n| crate::client_cmd::find_ci(dir, n.trim()).is_none())
        .cloned()
        .collect();
    let mut backed_up = false;
    if changed && !c.dry_run {
        if !c.no_backup {
            let bak = bak_path(&file);
            if !bak.exists() {
                fs::write(&bak, &old_bytes)
                    .with_context(|| format!("cannot write {}", bak.display()))?;
                backed_up = true;
            }
        }
        write_atomic(&file, &new_bytes)
            .with_context(|| format!("cannot write {}", file.display()))?;
    }
    print_list(&file, &ini, json, changed, c.dry_run, backed_up, &missing);
    Ok(0)
}

fn bak_path(file: &Path) -> PathBuf {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    file.with_file_name(format!("{name}.bak"))
}

fn print_list(
    file: &Path,
    ini: &DataIni,
    json: bool,
    changed: bool,
    dry_run: bool,
    backed_up: bool,
    missing: &[String],
) {
    let items = ini.items();
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "path": file.display().to_string(),
                "changed": changed,
                "dry_run": dry_run,
                "backup_written": backed_up,
                "archives": items.iter().map(|i| json!({"index": i.index, "name": i.name})).collect::<Vec<_>>(),
                "duplicate_indexes": ini.duplicate_indexes(),
                "not_found_next_to_data_ini": missing,
            })
        );
        return;
    }
    for i in &items {
        println!("{}={}", i.index, i.name);
    }
    let what = match (changed, dry_run) {
        (false, _) => None,
        (true, true) => Some("not written (--dry-run)"),
        (true, false) => Some("written"),
    };
    if let Some(w) = what {
        let bak = if backed_up {
            format!(", the old file is kept as {}", bak_path(file).display())
        } else {
            String::new()
        };
        eprintln!("{}: {w}{bak}", file.display());
    }
    for m in missing {
        eprintln!(
            "warning: {m} is not next to {}, the client would not find it",
            file.display()
        );
    }
}
