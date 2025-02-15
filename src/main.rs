use std::path::{Path, PathBuf};
use std::io;
use std::fs;

use char_reader::CharReader;

const VERSION:&str = "0.1";

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
        let file = fs::File::open(filepath);
        let mut chars = char_reader::CharReader::new(file?);
        while let Ok(Some(ch)) = chars.next_char() {
            // TODO
        }
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, format!("No filename given")))
    }
}
