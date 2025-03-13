//! Lisp evaluator

use crate::memory::{Memory, MemHandle};
use std::fmt;

pub type Builtin = fn(&mut Eval, MemHandle) -> Result<Option<MemHandle>, Error>;

#[derive(Debug, Clone)]
pub struct Error {
	pub message: String
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.message)
	}
}

impl std::error::Error for Error {}

pub struct Eval {
	pub memory: Memory,
	stack: Vec<(MemHandle, usize)>
}

impl Eval {
	pub fn new(memory: Memory, root: MemHandle) -> Self {
		Self {
			memory,
			stack: vec![(root, 0)]
		}
	}

	pub fn eval(&mut self) -> Result<(), Error> {
		Ok(())
	}
}
