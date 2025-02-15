use std::path::{Path, PathBuf};
use std::io;
use std::fs;

use char_reader::CharReader;

const VERSION:&str = "0.1";

mod reader;

fn main() -> io::Result<()> {
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[arg(short='v')]
        version: bool,
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
        let v = reader::ast(chars, filepath.to_string_lossy().into_owned());

        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, format!("No filename given")))
    }
}
