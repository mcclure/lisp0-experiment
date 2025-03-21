//! Lisp evaluator
// TODO: Arguments

use crate::memory::{Memory, MemHandle, Primitive, Value};
use std::fmt;

pub type Builtin = fn(&mut Eval, &[MemHandle]) -> Result<Option<MemHandle>, Error>;

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
	stack: Vec<(bool, MemHandle, Option<usize>, Vec<MemHandle>)> // want-return?, function, linenum-at, line-in-progress
}

enum PrepareNext {
	Push(MemHandle),
}

enum StackNext {
	Execute(bool), // Final line?
	Push(MemHandle)
}

impl Eval {
	pub fn new(memory: Memory, root: MemHandle) -> Self {
		Self {
			memory,
			stack: vec![(false, root, Some(0), Default::default())]
		}
	}

	pub fn eval(&mut self) -> Result<(), Error> {
		// stack will grow and shrink freely as program runs; when the stack's empty we're done.
		'eval: loop {
			let next = if let Some((_, fun, line_num, prepare)) = self.stack.last_mut() {
				// First work out what function we're running
				let fun_len = self.memory.array_len(fun.clone()); // Used only in multiline functions

				// Now unpack the current line within that function
				let (line, line_len) = if let Some(line_num) = line_num { // We are executing a normal multiline function
					if fun_len == 0 { return Err(Error {message:format!("Executing empty function")}) } // Isn't doing this every time slow :/

					let line = self.memory.array_get(fun.clone(), *line_num).expect("Interpreter internal error");
					(line.clone(), self.memory.array_len(line))
				} else { // We are executing a single line of code (probably a nested expression)
					(fun.clone(), self.memory.array_len(fun.clone()))
				};

				// We need to turn the line of "code" into a line of runtime values.
				'prepare: loop {
					// Interpret each word in the line, adding it to "prepare" until prepare has enough values
					if line_len <= prepare.len() {
						break 'prepare StackNext::Execute( // Prepare loop done
							if let Some(line_num) = line_num { *line_num >= fun_len-1 } else { true } // True if this is the last or only line of the fun
						)
					}

					// Inspect this word
					let item = self.memory.array_get(line.clone(), prepare.len()).expect("Interpreter internal error"); // We checked the length already
					let next = match self.memory.value(item.clone()) {
						// Integers are produced by the reader and just get passed through.
						// FIXME: Nil, True, Builtin are inappropriate because the reader doesn't make these?
				        Value::Primitive(primitive) => match primitive {
				            Primitive::Nil | Primitive::True
				            | Primitive::Int(_) | Primitive::Builtin(_)
				            	 => PrepareNext::Push(item),

				            // Strings are treated as names, and read from the dynamic scope.
				            name @ Primitive::String(_) => {
				            	let readback = self.memory.dict_get(self.memory.globals.clone(), name);
				            	if let Some(sub_item) = readback { // Found
					            	PrepareNext::Push(sub_item)
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
				        	// FIXME: match and convert numbers/nil to string?
				        	PrepareNext::Push(self.memory.quote_get(item))
				        }, // Unwrap
				        // Arrays are treated as function calls, and executed.
				        Value::Array => {
				        	// But we'll have to defer that to the next loop iteration…
				        	break 'prepare StackNext::Push(item)
				        }
				        // FIXME: Inappropriate?
				        Value::Dict => {
				        	PrepareNext::Push(item)
				        },
				    };

				    // We survived to the end of the loop! Push the value we found and continue.
				    match next {
	        			PrepareNext::Push(handle) => {
	        				prepare.push(handle)
	        			},
				    };
				}
			} else {
				break 'eval; // Stack is empty. Exit the function
			};

			// Got kicked out of the prepare loop. Why?
			match next {
				// Prepare loop isn't done and wants an individual (call) evaluated.
			    StackNext::Push(handle) => {
			    	self.stack.push((true, handle, None, Default::default()));
			    }

			    // Prepare loop is done and now we should execute the line we've prepared.
			    StackNext::Execute(returning) => {
			    	// Peel a layer off the stack (we might push the first three values back later but prepare we'll consume)
			    	let (want_return,fun,line_num,prepare) = self.stack.pop().unwrap(); // Consider making unwrap unsafe
			    	let mut returned:Option<MemHandle> = None; // Will only be populated if returning
			    	if !returning { // More lines to execute in this function! Should not have popped
			    		self.stack.push((want_return,fun,Some(line_num.unwrap()+1),Default::default())); // unwrap known safe, could be unchecked
			    	}
			    	let returning = returning && want_return; // If the line wants to return but we're in the middle of a multiline function... don't return
			    	if prepare.len() == 0 { // Allow empty lines?? I guess a convenience for builders
			    		if returning { // This means that () by itself is a shorthand for nil
			    			returned = Some(self.memory.nil());
			    		}
			    	} else {
			    		// Split into function and args
			    		let (car,cdr):(&MemHandle,&[MemHandle]) = if prepare.len() == 1 { // Juggle unsafe split_at case
			    			(&prepare[0], &[])
			    		} else {
			    			let (carl, cdrl) = prepare.split_at(1);
			    			(&carl[0], cdrl)
			    		};
			    		// Only a couple things are executable actually...
			    		match self.memory.value(car.clone()) {
			    			// Interpreter defined
				            Value::Primitive(Primitive::Builtin(fun)) => {
				            	let result = fun(self, cdr)?;
				            	if returning {
				            		returned = Some(result.unwrap_or_else(|| self.memory.nil()));
				            	}
				            },
				            // User defined
							Value::Array => {
								self.stack.push((returning,car.clone(),Some(0),Default::default()));
							},
							// That's it!
							v @ _ => {
				            	return Err(Error {message:format!("Tried to execute non-function: {:?}", v)}) // TODO display
							}
				        }
			    	}
			       	// Peek stack one level, append to prepare and loop
			       	// (If appending to prepare ISN'T the right thing to do... something went VERY wrong above!)
			        if let Some(returned) = returned {
			        	let Some((_, _, _, prepare)) = self.stack.last_mut() else { panic!("Interpreter internal error"); };
			        	prepare.push(returned);
			        }
			    }
			}
		}
		Ok(())
	}
}
