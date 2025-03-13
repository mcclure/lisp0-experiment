//! Command line app

use std::path::{Path, PathBuf};
use std::io;
use std::fs;

use char_reader::CharReader;

const VERSION:&str = "0.1";

mod eval;
mod globals;
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
        #[arg(long="debug-mem-size", help="(Internal debug) Set initial GC space size")]
        debug_mem_size: usize,
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
        let v = crate::reader::ast(chars, filepath.to_string_lossy().into_owned())?;

        if cli.debug_ast {
            println!("{}", crate::util::ast_to_string(&v.source));

            Ok(())
        } else {
            let mut memory = if cli.debug_mem_size > 0 {
                crate::memory::Memory::new_sized(cli.debug_mem_size)
            } else {
                crate::memory::Memory::new()
            };

            crate::globals::populate(&mut memory);

            let root = memory.construct(v.source.content);
            let mut eval = crate::eval::Eval::new(memory, root);

            eval.eval().map_err( |e| e.into() )
        }
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, format!("No filename given")))?
    }
}
