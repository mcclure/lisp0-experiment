//! Lisp evaluator

use crate::memory::{Memory, MemHandle, Primitive, Value};
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
	stack: Vec<(MemHandle, usize, Vec<MemHandle>)>
}

enum NextStep {
	Push(MemHandle),
	Execute(MemHandle)
}

impl Eval {
	pub fn new(memory: Memory, root: MemHandle) -> Self {
		Self {
			memory,
			stack: vec![(root, 0, Default::default())]
		}
	}

	pub fn eval(&mut self) -> Result<(), Error> {
		// stack will grow and shrink freely as program runs; when the stack's empty we're done.
		while let Some((fun, line_num, prepare)) = self.stack.last() {
			// First work out what function we're running
			let fun_len = self.memory.array_len(fun.clone());
			if fun_len == 0 { panic!("Executing empty function"); } // Isn't doing this every time slow :/

			// Now unpack the current line within that function
			let line = self.memory.array_get(fun.clone(), *line_num).expect("Interpreter internal error");
			let line_len = self.memory.array_len(line.clone());

			// We need to turn the line of "code" into a line of runtime values.
			'prepare: while prepare.len() < line_len {
				let item = self.memory.array_get(line.clone(), prepare.len()).expect("Interpreter internal error");
				let next = match self.memory.value(item.clone()) {
					// Integers are produced by the reader and just get passed through.
					// FIXME: Nil, True, Builtin are inappropriate?
			        Value::Primitive(primitive) => match primitive {
			            Primitive::Nil | Primitive::True
			            | Primitive::Int(_) | Primitive::Builtin(_)
			            	 => NextStep::Push(item),

			            // Strings are treated as names, and read from the dynamic scope.
			            name @ Primitive::String(_) => {
			            	let readback = self.memory.dict_get(self.memory.globals.clone(), name);
			            	if let Some(sub_item) = readback {
				            	NextStep::Push(sub_item)
				            } else { // Not found
				            	// Slightly awkward, premature optimization: Rather than clone name above in the expected case,
				            	// when the exceptional case occurs make an entirely new call into memory to pull the string again.
				            	let Value::Primitive(Primitive::String(name_str)) = self.memory.value(item) else { panic!("Interpreter internal error"); };
				            	return Err(Error {message:format!("Unrecognized variable: {name_str}")})
				            }
			            },
        			},
        			// Quoted values are unwrapped.
			        Value::Quote => {
			        	NextStep::Push(self.memory.quote_get(item))
			        }, // Unwrap
			        // Arrays are treated as function calls, and executed.
			        Value::Array => {
			        	// But we'll have to defer that to the next loop iteration…
			        	NextStep::Execute(item)
			        }
			        // FIXME: Inappropriate?
			        Value::Dict => {
			        	NextStep::Push(item)
			        },
			    };

			    match next {
			    	NextStep::Execute(item) => {
			    		self.stack.push((item, 0, Default::default()));
			    	}
        			NextStep::Push(rc) => {
        			},
			    };
			}
		}
		Ok(())
	}
}
