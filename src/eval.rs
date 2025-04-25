//! Lisp evaluator
// TODO: Arguments

// If EVERYTHING'S broken
const TRACE_DEBUG:bool = false;

use crate::reader;
use crate::memory::{Memory, MemHandle, Primitive, Value};
use std::fmt;

pub enum BuiltinReturn {
	None,
	Value(MemHandle),
	Push(Vec<MemHandle>)
}

pub type Builtin = fn(&mut Eval, &[MemHandle]) -> Result<BuiltinReturn, Error>;

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

fn position_string(memory: &Memory, source_tag:&reader::SourceTag, handle:MemHandle) -> String {
	let pos = memory.array_get_position(handle);
	if pos.source == reader::POSITION_UNKNOWN.source
	&& pos.line   == reader::POSITION_UNKNOWN.line
	&& pos.column == reader::POSITION_UNKNOWN.column {
		"[position unknown]".to_string()
	} else {
		format!("{} line {} column {}", source_tag[pos.source as usize], pos.line, pos.column)
	}
}

// want-return?, restore-stack-on-return, function-or-line, linenum-at, line-in-progress
type StackFrame = (bool, Option<MemHandle>, Option<MemHandle>, Option<usize>, Vec<MemHandle>);

pub struct Eval {
	pub memory: Memory,
	pub source_tag: reader::SourceTag,
	stack: Vec<StackFrame>,

	// Scratch space for globals.rs
	pub file_allow: bool,
	pub file_in: Option<std::io::BufReader<std::fs::File>>,
	pub file_out: Option<std::fs::File>
}

enum PrepareNext {
	Push(MemHandle),
}

enum StackNext {
	Proceed(bool, bool), // Will execute and/or return. Args: Execute? Final line?
	Push(MemHandle) // Still preparing and need to invoke a function.
}

impl Eval {
	pub fn new(memory: Memory, root: MemHandle, source_tag:reader::SourceTag, file_allow: bool) -> Self {
		Self {
			memory,
			source_tag,
			stack: vec![(false, None, Some(root), Some(0), Default::default())],
			file_allow, file_in: None, file_out: None
		}
	}

	pub fn position_string(&self, handle: MemHandle) -> String {
		position_string(&self.memory, &self.source_tag, handle)
	}

	pub fn eval(&mut self) -> Result<(), Error> {
		macro_rules! args_str { // We need this string like three places below, I don't want to keep retyping it
			() => {
				Primitive::String("args".to_string())
			}
		}

		// TODO: Replace eformat with an after-the-fact stack walk; remove fun clone on third call
		macro_rules! eformat {
			($pos:expr, $str:expr $(,$args:expr)*) => {
				format!(concat!("{}: ", $str), position_string(&self.memory, &self.source_tag, $pos.clone()), $($args),*)
			}
		}

		// Oddball cases: Empty program, empty stack
		if let Some((_, _, fun, _, _)) = self.stack.last() {
			let Some(fun) = fun else { panic!("Internal error") };
			if 0 == self.memory.array_len(fun.clone()) {
				return Ok(())
			}
		} else { panic!("Can't call eval() twice on one Eval"); }

		// stack will grow and shrink freely as program runs; when the stack's empty we're done.
		'eval: loop {
			let stack = &mut self.stack;
			let next = if let Some((_, _, fun, line_num, prepare)) = stack.last_mut() {
				let Some(fun) = fun else { panic!("Internal error") };

				// First work out what function we're running
				let fun_len = self.memory.array_len(fun.clone()); // Used only in multiline functions

				// Now unpack the current line within that function
				let (line, line_len) = if let Some(line_num) = line_num { // We are executing a normal multiline function
					if fun_len == 0 { // Isn't doing this every time slow :/
						return Err(Error {message:eformat!(fun, "Executing empty function")}) 
					}
					let line = self.memory.array_get(fun.clone(), *line_num).expect("Interpreter internal error");

					// TODO: Replace eformat with an after-the-fact stack walk; remove fun clone on third call
					match self.memory.value(line.clone()) {
		                   // Think carefully: This doesn't refer to the *value* being an array but to the *code item* being an array.
		                   Value::Array => (line.clone(), Some(self.memory.array_len(line))),
		                   // Non-array items are simply "returned". TODO: Should quoted values be unwrapped?
		                   _ => (line.clone(), None)
					}
				} else { // We are executing a single line of code (probably a nested expression)
					(fun.clone(), Some(self.memory.array_len(fun.clone())))
				};

				// We need to turn the line of "code" into a line of runtime values.
				'prepare: loop {
					// Interpret each word in the line, adding it to "prepare" until prepare has enough values
					let mut is_final = || {
						if let Some(line_num) = line_num { *line_num >= fun_len-1 } else { true } // True if this is the last or only line of the fun
					};
					
					let item = if let Some(line_len) = line_len {
						if line_len <= prepare.len() {
							break 'prepare StackNext::Proceed(true, is_final()) // Prepare loop done
						}
						self.memory.array_get(line.clone(), prepare.len()).expect("Interpreter internal error") // We checked the length already
					} else {
						line.clone() // None len is an indication line was not an array.
					};

					// Inspect this word
					let next = match self.memory.value(item.clone()) {
						// Integers are produced by the reader and just get passed through.
						// FIXME: Nil, True, Builtin are inappropriate because the reader doesn't make these?
				        Value::Primitive(primitive) => match primitive {
				            Primitive::Nil | Primitive::True
				            | Primitive::Int(_) | Primitive::Builtin(_)
				            	 => PrepareNext::Push(item),

				            // Strings are treated as names, and read from the dynamic scope.
				            name @ Primitive::String(_) => {
				            	if TRACE_DEBUG { // Yes it's ugly, no there's not a better way to make the borrow checker happy
					            	let readback = self.memory.dict_get(self.memory.globals.clone(), name.clone());
				            		print!("[..Arg {} Decode: \"{}\" To: {}]", prepare.len(), name.clone(), if let Some(value) = readback { self.memory.value(value.clone())} else {Value::Primitive(Primitive::Int(404)) });
				            	}
				            	let readback = self.memory.dict_get(self.memory.globals.clone(), name);
				            	if let Some(sub_item) = readback { // Found
					            	PrepareNext::Push(sub_item)
					            } else { // Not found
					            	// Slightly awkward, premature optimization: Rather than clone name above in the expected case,
					            	// when the exceptional case occurs make an entirely new call into memory to pull the string again.
					            	let Value::Primitive(Primitive::String(name_str)) = self.memory.value(item) else { panic!("Interpreter internal error"); };
					            	return Err(Error {message:eformat!(fun, "Unrecognized variable: {}", name_str)})
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
				        Value::Fun | Value::Dict => { // FIXME: Is dict inappropriate?
				        	PrepareNext::Push(item)
				        },
				    };

				    // We survived to the end of the loop! Push the value we found and continue.
				    match next {
	        			PrepareNext::Push(handle) => {
	        				prepare.push(handle);

	        				if line_len.is_none() { // Line wasn't a list, so prepare loop wasn't much of a loop
	        					break 'prepare StackNext::Proceed(false, is_final()) // Use prepare vec to shuttle line to next bit..
	        				}
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
					if TRACE_DEBUG {
						println!("[EVAL CALL depth: {}]", self.stack.len());
					}
			    	self.stack.push((true, None, Some(handle), None, Default::default()));
			    }

			    // Prepare loop is done and now we should execute the line we've prepared.
			    StackNext::Proceed(execute, returning) => {
			    	let mut return_on_continue = returning;

			    	'execute: loop {
				    	let returning = return_on_continue;

				    	// Peel a layer off the stack (we might push the first three values back later but prepare we'll consume)
				    	let (want_return,scope_restore,fun,line_num,prepare) = self.stack.pop().unwrap(); // Consider making unwrap unsafe
				    	let mut returned:Option<MemHandle> = None; // Will only be populated if returning

				    	// The odd construction here is because one branch of this `if` "eats" scope_restore
				    	let mut scope_restore = if returning {
				    		scope_restore
				    	} else {
				    		// More lines to execute in this function! Should not have popped
				    		self.stack.push((want_return,scope_restore,fun.clone(),Some(line_num.unwrap()+1),Default::default())); // unwrap known safe, could be unchecked
				    		None
				    	};

				    	let returning = returning && want_return; // If the line wants to return but we're in the middle of a multiline function... don't return
				    	
				    	if !execute {
				    		if returning {
					    		returned = Some(prepare[0].clone());
					    	}
				    	} else if prepare.len() == 0 { // Allow empty lines?? I guess a convenience for builders
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
		    				if TRACE_DEBUG {
								print!("[EVAL EXEC @{} depth: {} car {} cdrl {}: ", eformat!(true_fun(&self.stack, fun.clone()), ""), self.stack.len(), self.memory.value(car.clone()), cdr.len());
								let mut first = false; for handle in cdr {
									if !first { first = true; } else { print!(", ") }
									print!("{}", self.memory.value(handle.clone()));
								}
								println!("]");
							}

							fn true_fun(stack:&Vec<StackFrame>, fun:Option<MemHandle>) -> MemHandle {
								if let Some(fun) = fun { fun } else {
									let (_,_,fun,_,_) = stack.last().unwrap(); // Consider making unwrap unsafe
									fun.clone().expect("Interpreter internal error")
								}
							}

				    		// Only a couple things are executable actually...
				    		match self.memory.value(car.clone()) {
				    			// Interpreter defined
					            Value::Primitive(Primitive::Builtin(builtin)) => {
					            	let result = builtin(self, cdr).map_err(|e| {
					            		let fun = true_fun(&self.stack, fun);
					            		Error {message:eformat!(fun, "{}", e.message)}
					            	})?;
					            	match result {
					                    BuiltinReturn::None => {
					                    	if returning {
					                    		returned = Some(self.memory.nil())
					                    	}
					                    }
					                    BuiltinReturn::Value(handle) => {
					                    	if returning {
					                    		returned = Some(handle);
					                    	}
					                    }
					                    BuiltinReturn::Push(handles) => {
					                    	if TRACE_DEBUG {
	                							println!("[EVAL SPECIAL depth: {} returning: {returning} SR? {}]", self.stack.len(), scope_restore.is_some());
					                    	}
					                    	self.stack.push((returning,scope_restore,None,None,handles));
					                    	return_on_continue = true;
					                    	continue 'execute;
					                    }
					                }
					            },
					            // User defined
								Value::Array => {
									// The only complicated part here is juggling the args variable
/*
									let old_args = if scope_restore.is_none() {
										// Normal case: Fetch the args variable out of memory
										let old_args = self.memory.dict_get(self.memory.globals.clone(), args_str!());
										old_args.unwrap_or_else(||self.memory.nil()) // Failing here should be impossible currently
									} else {
										// Tail recursion case: yoink the args var *this* stackframe was supposed to return
										std::mem::take(&mut args_restore).unwrap()
									};

									if TRACE_DEBUG {
										println!("[EVAL DESCEND depth: {} carl: {}]", self.stack.len(), self.memory.array_len(car.clone()));
									}

									let args = self.memory.array_from_handles(cdr);
									self.memory.dict_set(self.memory.globals.clone(), args_str!(), args);
*/
									if TRACE_DEBUG {
										print!("[EVAL DESCEND B depth: {} carl: {} SR? {}", self.stack.len(), self.memory.array_len(car.clone()), scope_restore.is_some());
									}

									self.stack.push((returning,scope_restore.clone(),Some(car.clone()),Some(0),Default::default()));
								},
								Value::Fun => {
									// Fiddle with old_args if it exists and we're replacing
									// Then invoke
									let call_fun = self.memory.fun_unpack(car.clone());

									if TRACE_DEBUG {
										print!("[EVAL DESCEND F depth: {} SR? {}", self.stack.len(), scope_restore.is_some());
									}

									let args_value = self.memory.value(call_fun.args.clone());
									let locals_value = self.memory.value(call_fun.locals.clone());
									let no_name = call_fun.name.is_none();
									let no_args = matches!(args_value, Value::Primitive(Primitive::Nil));
									let no_locals = matches!(locals_value, Value::Primitive(Primitive::Nil));
									if !(no_name && no_args && no_locals) {
										if TRACE_DEBUG { print!(" Swap: "); }
										fn swap(memory: &mut Memory, scope_restore: MemHandle, key:Primitive, handle: MemHandle) {
											let old_handle = memory.dict_get(memory.globals.clone(), key.clone());
											let has = memory.dict_has(scope_restore.clone(), key.clone());
											if TRACE_DEBUG {
												print!("{}:{}{}, ", key.clone(), memory.value(handle.clone()), if has {"(skip)"} else {""});
											}
											memory.dict_set(memory.globals.clone(), key.clone(), handle);

											if !has { // Older shadows newer for restore
												if let Some(old_handle) = old_handle {
													memory.dict_set(scope_restore, key, old_handle);
												} else { // MemCell has a special value type JUST for this case
													memory.dict_set_hole(scope_restore, key);
												}
											}
										}

										let scope_restore = scope_restore.clone().unwrap_or_else(|| self.memory.dict_new());

										if !no_locals {
											let locals_keys = self.memory.dict_keys(call_fun.locals.clone());
											for key in locals_keys {
												let value = self.memory.dict_get(call_fun.locals.clone(), key.clone()).unwrap();
												swap(&mut self.memory, scope_restore.clone(), key, value);
											}
										}

										if !no_args {
											for idx in 0..self.memory.array_len(call_fun.args.clone()) {
												let pair = self.memory.array_get(call_fun.args.clone(), idx).unwrap();
												let name = self.memory.array_get(pair.clone(), 0).unwrap();
												let Value::Primitive(key@Primitive::String(_)) = self.memory.value(name) else { panic!("Interpreter internal error"); };
												let value = if cdr.len() > idx {
													cdr[idx].clone()
												} else {
													self.memory.array_get(pair, 1).unwrap()
												};
												swap(&mut self.memory, scope_restore.clone(), key, value)
											}
										}

										if let Some(name_str) = call_fun.name {
											swap(&mut self.memory, scope_restore.clone(), Primitive::String(name_str), car.clone());
										}

										if TRACE_DEBUG {
											println!("]");
										}

										// Invoke
										self.stack.push((returning,Some(scope_restore),Some(call_fun.body),Some(0),Default::default()));
									}
								}
								// That's it!
								v @ _ => {
				            		let fun = true_fun(&self.stack, fun);
					            	return Err(Error {message:eformat!(fun, "Tried to execute non-function: {:?}", v)}) // TODO display
								}
					        }
				    	}

				    	// End-of-function stack cleanup follows

						if TRACE_DEBUG {
							print!(" [ARGS ASCEND");
						}

				        // If we're returning and we realized above we need to juggle args, do that
				        if let Some(scope_restore) = &scope_restore {
				        	if TRACE_DEBUG {
								print!(" RESTORE c {}", self.memory.dict_len(scope_restore.clone()));
							}
							let restore_keys = self.memory.dict_keys(scope_restore.clone());
							for key in restore_keys {
								let value = self.memory.dict_get(scope_restore.clone(), key.clone()).unwrap();

								if self.memory.is_hole(value.clone()) {
									self.memory.dict_del(scope_restore.clone(), key);
								} else {
									self.memory.dict_set(scope_restore.clone(), key, value);
								}
							}
					    }

					    if TRACE_DEBUG { println!("]"); }

				        // Peek stack one level, append to prepare and loop
				       	// (If appending to prepare ISN'T the right thing to do... something went VERY wrong above!)
				        if let Some(returned) = returned {
				        	let Some((_, _, _, _, prepare)) = self.stack.last_mut() else { panic!("Interpreter internal error"); };
				        	prepare.push(returned);
				        }

					    break; // Do not actually loop
				    }
				}
			}
		}
		Ok(())
	}
}
