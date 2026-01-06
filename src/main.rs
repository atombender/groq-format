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
use groq_format::{DEFAULT_WIDTH, format_query};
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
}

fn main() {
    if let Err(e) = run() {
        eprintln!("groq-format: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.inputs.is_empty() {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;

        let formatted = format_query(&input, cli.width)?;
        println!("{}", formatted);
    } else {
        for input in &cli.inputs {
            process_file(Path::new(input), cli.write, cli.width)?;
        }
    }

    Ok(())
}

fn process_file(path: &Path, write: bool, width: usize) -> Result<(), Box<dyn std::error::Error>> {
    let input = fs::read_to_string(path)?;
    let formatted = format_query(&input, width)?;

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
