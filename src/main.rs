//! Command line app

use std::path::{Path, PathBuf};
use std::io;
use std::fs;

use either::Either;
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
        #[arg(short='e', long="execute", help="Execute instead of file")]
        execute: Option<String>,
        #[arg(long="lisp", help="Force lisp syntax mode")]
        force_lisp: bool,
        #[arg(long="disable-fs", help="Suppress file I/O")]
        disable_fs: bool,
        #[arg(long="debug-ast", help="(Internal debug) Show reader output")]
        debug_ast: bool,
        #[arg(long="debug-mem-initial", help="(Internal debug) Set initial GC space size", default_value_t=0, hide_default_value=true)]
        debug_mem_initial: usize,
        #[arg(long="debug-mem-limit", help="(Internal debug) Set max GC space size", default_value_t=0, hide_default_value=true)]
        debug_mem_limit: usize,
        filepath: Option<PathBuf>,
        #[clap(last=true, num_args = 0..)]
        pub args: Vec<String>,
    }
    let cli = Cli::parse();

    if cli.version {
        println!("Unnamed language interpreter, v{}", VERSION);
        return Ok(());
    }

    let (chars, tag, lisp) = if let Some(execute) = &cli.execute {
        if cli.filepath.is_some() {
            Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Gave both filename and -e")))?;
        }
        (Either::Left(char_reader::CharReader::new(execute.as_bytes())), "<commandline>".to_string(), cli.force_lisp)
    } else {
        if let Some(filepath) = cli.filepath {
            let filepath_string = filepath.to_string_lossy().into_owned();
            let lisp = cli.force_lisp || util::is_lisp_filename(&filepath_string);
            let file = fs::File::open(filepath.clone());
            (Either::Right(char_reader::CharReader::new(file?)), filepath_string, lisp)
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, format!("No filename given")))?;
            unreachable!(); // ? will terminate previous line
        }
    };

    let v = either::for_both!(chars, chars => // Convert the Either<T> to a <T>
        crate::reader::ast(chars, tag, lisp)
    )?;

    if cli.debug_ast {
        println!("{}", crate::util::ast_to_string(&v.source));

        Ok(())
    } else {
        let mut memory = if cli.debug_mem_initial > 0 {
            crate::memory::Memory::new_sized(cli.debug_mem_initial)
        } else {
            crate::memory::Memory::new()
        };
        if cli.debug_mem_limit > 0 {
            memory.gc_size_limit = cli.debug_mem_limit;
        }

        crate::globals::populate(&mut memory, &cli.args);

        let root = memory.construct(v.source.content);
        let mut eval = crate::eval::Eval::new(memory, root, !cli.disable_fs);

        eval.eval().map_err( |e| e.into() )
    }
}
