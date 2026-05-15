//! groq-format - Format GROQ queries with adaptive line wrapping.
//!
//! Usage:
//!     groq-format query.groq                    # Format file to stdout
//!     groq-format -w query.groq                 # Format file in-place
//!     echo '*[_type == "article"]' | groq-format  # Format from stdin

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use clap::Parser;
use groq_format::{DEFAULT_WIDTH, FormatOptions, format_query_with_options};
use tempfile::NamedTempFile;

#[derive(Parser)]
#[command(name = "groq-format")]
#[command(about = "Format GROQ queries with adaptive line wrapping")]
#[command(version)]
struct Cli {
    /// Files to format. If empty, reads from stdin.
    #[arg(value_name = "FILE")]
    inputs: Vec<String>,

    /// Write result to source file instead of stdout (only for file inputs)
    #[arg(short = 'w', long = "write")]
    write: bool,

    /// Maximum line width
    #[arg(short = 'W', long = "width", default_value_t = DEFAULT_WIDTH)]
    width: usize,

    /// Wrap more aggressively: introduce break points at binary operators,
    /// filter brackets, parentheses and single-argument function calls so
    /// long expressions are broken to honor the width limit.
    #[arg(long = "force-wrap")]
    force_wrap: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("groq-format: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let options = FormatOptions::new(cli.width).with_force_wrap(cli.force_wrap);

    if cli.inputs.is_empty() {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;

        let formatted = format_query_with_options(&input, &options)?;
        println!("{}", formatted);
    } else {
        for input in &cli.inputs {
            process_file(Path::new(input), cli.write, &options)?;
        }
    }

    Ok(())
}

fn process_file(
    path: &Path,
    write: bool,
    options: &FormatOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = fs::read_to_string(path)?;
    let formatted = format_query_with_options(&input, options)?;

    if write {
        // Write atomically: write to temp file in same dir, then rename
        let dir = path.parent().unwrap_or(Path::new("."));
        let mut temp = NamedTempFile::new_in(dir)?;
        writeln!(temp, "{}", formatted)?;
        temp.persist(path)?;
    } else {
        println!("{}", formatted);
    }

    Ok(())
}
