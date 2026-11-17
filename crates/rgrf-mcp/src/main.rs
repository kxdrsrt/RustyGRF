//! `rgrf-mcp`: MCP server over stdio. Reads anywhere (or below `--allow-read`); writing tools exist
//! only with `--allow-write`. Logs go to stderr, stdout carries only protocol messages.

use std::io::{self, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use rgrf_mcp::policy::Policy;
use rgrf_mcp::server::{Server, serve};

#[derive(Parser)]
#[command(
    name = "rgrf-mcp",
    version,
    about = "MCP server for Ragnarok Online GRF archives (stdio)"
)]
struct Cli {
    /// Folder the server may write to (extract, build, repack, merge, patch); repeatable.
    /// Without it the server is read-only and the writing tools are not offered.
    #[arg(long = "allow-write", value_name = "DIR")]
    allow_write: Vec<PathBuf>,
    /// Restrict reading to these folders (plus the write folders); repeatable.
    /// Default: archives and folders anywhere can be read.
    #[arg(long = "allow-read", value_name = "DIR")]
    allow_read: Vec<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let policy = match Policy::new(&cli.allow_read, &cli.allow_write) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("rgrf-mcp: {e}");
            return ExitCode::from(1);
        }
    };
    eprintln!(
        "rgrf-mcp {}: ready ({}; reading {})",
        rgrf_core::VERSION,
        if policy.write_enabled() {
            format!(
                "writing allowed in {}",
                policy
                    .write_roots()
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            "read-only".to_string()
        },
        if policy.read_restricted() {
            "restricted"
        } else {
            "anywhere"
        }
    );
    match serve(
        &Server::new(policy),
        BufReader::new(io::stdin()),
        io::stdout(),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rgrf-mcp: {e}");
            ExitCode::from(1)
        }
    }
}
