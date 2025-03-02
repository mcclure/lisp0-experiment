//! Command line app

use std::path::{Path, PathBuf};
use std::io;
use std::fs;

use char_reader::CharReader;

const VERSION:&str = "0.1";

mod memory;
mod reader;
mod util;

fn main() -> anyhow::Result<()> {
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[arg(short='v', long="version")]
        version: bool,
        #[arg(long="debug-ast", help="(Internal debug) Show reader output")]
        debug_ast: bool,
        filepath: Option<PathBuf>
    }
    let cli = Cli::parse();

    if cli.version {
        println!("Unnamed language interpreter, v{}", VERSION);
        return Ok(());
    }

    if let Some(filepath) = cli.filepath {
        let file = fs::File::open(filepath.clone());
        let mut chars = char_reader::CharReader::new(file?);
        let v = reader::ast(chars, filepath.to_string_lossy().into_owned())?;

        if cli.debug_ast {
            println!("{}", crate::util::ast_to_string(&v.source));
        }

        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, format!("No filename given")))?
    }
}
