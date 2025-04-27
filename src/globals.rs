//! "Standard library"

use clap::builder::OsStringValueParser;

use crate::eval::{Error, Eval, BuiltinReturn};
use crate::memory::{MemHandle, MemHandleImpl, Memory, Primitive, Value};
use crate::reader::ReaderPosition;

use std::fmt;
use std::io::stdin;
use std::io::{Write, Read, BufRead};

// Note: At present, it is assumed that all exec lists include the builtin itself as 0th argument.

fn insert(memory: &mut Memory, name:&str, primitive:Primitive) {
	memory.dict_set_value(memory.globals.clone(), Primitive::String(name.to_string()), Value::Primitive(primitive));
}

fn option_to_value(o:Option<MemHandle>) -> BuiltinReturn {
	if let Some(o) = o {
		BuiltinReturn::Value(o)
	} else {
		BuiltinReturn::None
	}
}

pub fn populate(memory: &mut Memory, args:&[String]) {
	fn fmt_fs_error(e:std::io::Error, name:&str) -> Error {
		Error {message:format!("Filesystem failure running `{name}`: {}", e)}
	}

	// --- Core ---

	insert(memory, "set", Primitive::Builtin(|eval, args| {
		if args.len() < 2 {
			return Err(Error {message:"Too few args to `set`".to_string()});
		}

		match eval.memory.value(args[0].clone()) {
			// Set variable on scope.
			Value::Primitive(v @ Primitive::String(_)) => {
				if args.len() == 2 {
					eval.memory.dict_set(eval.memory.globals.clone(), v, args[1].clone());
					Ok(BuiltinReturn::None)
				} else {
					Err(Error {message:"Too many args to `set`".to_string()})
				}
			}
			Value::Quote => {
				if args.len() == 2 {
					eval.memory.quote_set(args[0].clone(), args[1].clone());
					Ok(BuiltinReturn::None)
				} else {
					Err(Error {message:"Too many args to `set`".to_string()})
				}
			}
			Value::Array => {
				if args.len() == 3 {
					match eval.memory.value(args[1].clone()) {
						Value::Primitive(Primitive::Int(i)) => {
							eval.memory.array_set(args[0].clone(), i as usize, args[2].clone());
							Ok(BuiltinReturn::None)
						}
						_ => Err(Error {message:"Index in array `set` not an integer".to_string()})
					}
				} else {
					Err(Error {message:"`set` on array expects exactly 3 arguments".to_string()})
				}
			}
			Value::Dict => {
				if args.len() == 3 {
					match eval.memory.value(args[1].clone()) {
						Value::Primitive(p) => {
							eval.memory.dict_set(args[0].clone(), p, args[2].clone());
							Ok(BuiltinReturn::None)
						}
						_ => Err(Error {message:"Index in dict `set` not a primitive".to_string()})
					}
				} else {
					Err(Error {message:"`set` on dict expects exactly 3 arguments".to_string()})
				}
			}

			// TODO: Value::Dict, Value::Array, local
			v @ _ => Err(Error {message:format!("First argument to `set` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	insert(memory, "print", Primitive::Builtin(|eval, args| {
		if let Some(file) = &mut eval.file_out {
			for arg in args { // TODO: In both loops "consume" input
				write!(file, "{}", eval.memory.value(arg.clone())).map_err(|e|fmt_fs_error(e, "print"))
					?;
			}
		} else {
			for arg in args {
				print!("{}", eval.memory.value(arg.clone()));
			}
		}
		Ok(BuiltinReturn::None)
	}));

	insert(memory, "flush", Primitive::Builtin(|eval, args| {
		if args.len() > 0 {
			return Err(Error {message:"`flush` expects no arguments".to_string()});
		}
		if let Some(file) = &mut eval.file_out {
			file.flush()
		} else {
			std::io::stdout().flush()
		}.map_err(|e|fmt_fs_error(e, "print"))?;
		Ok(BuiltinReturn::None)
	}));

	// --- Math ---

	// + rules: Strings can be added to strings and numbers, but nothing else
	//          Ints can only be added to other ints
	//          TODO: Arrays can be added to arrays, dicts can be added to dicts...?
	insert(memory, "+", Primitive::Builtin(|eval, args| {
		if args.len() < 2 {
			return Err(Error {message:"Too few args to `+`".to_string()});
		}
		match eval.memory.value(args[0].clone()) {
			// Get variable from scope.
			Value::Primitive(Primitive::String(s)) => {
				let mut s = s;
				for idx in 1..args.len() {
					match eval.memory.value(args[idx].clone()) {
						Value::Primitive(Primitive::String(s2)) =>
							s = s + &s2,
						Value::Primitive(Primitive::Int(i2)) =>
							s = format!("{s}{i2}"),
						v @ _ => return Err(Error {message:format!("Argument {idx} to `+` is not a string or number: {:?}", v)})
					}
				}
				Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::String(s)))))
			}
			Value::Primitive(Primitive::Int(i)) => {
				let mut i = i;
				for idx in 1..args.len() {
					match eval.memory.value(args[idx].clone()) {
						Value::Primitive(Primitive::Int(i2)) =>
							i = i + i2,
						v @ _ => return Err(Error {message:format!("Argument {idx} to `+` is not a number: {:?}", v)})
					}
				}
				Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::Int(i)))))
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => Err(Error {message:format!("First argument to `+` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	// -, *, and / are remarkably similar. Construct with a macro instead of closure operations
	// so that we can get an fn() and not a Closure.
	macro_rules! insert_binary_math {
		($name:expr, $op:expr) => {
			insert(memory, $name, Primitive::Builtin(|eval, args| {
				if args.len() < 2 {
					return Err(Error {message:format!("Too few args to `{}`", $name)});
				}
				let op = $op;
				match eval.memory.value(args[0].clone()) {
					Value::Primitive(Primitive::Int(i)) => {
						let mut i = i;
						for idx in 1..args.len() {
							match eval.memory.value(args[idx].clone()) {
								Value::Primitive(Primitive::Int(i2)) =>
									i = op(i, i2),
								v @ _ => return Err(Error {message:format!("Argument {idx} to {} is not a number: {:?}", $name, v)})
							}
						}
						Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::Int(i)))))
					}
					// TODO: Value::Dict, Value::Array, local
					v @ _ => Err(Error {message:format!("First argument to `{}` unrecognized: {:?}", $name, v)}) // TODO: Display not Debug
				}
			}));
		}
	}

	insert_binary_math!("-", |x,y| x-y);
	insert_binary_math!("*", |x,y| x*y);
	insert_binary_math!("/", |x,y| x/y);
	insert_binary_math!("&", |x,y| x&y);
	insert_binary_math!("|", |x,y| x|y);
	insert_binary_math!("^", |x,y| x^y);

	// FIXME: Is it weird to allow (/ 1 2 3) but not (% 1 2 3) ?
	insert(memory, "%", Primitive::Builtin(|eval, args| {
		if args.len() != 2 {
			return Err(Error {message:"`%` expects exactly 2 arguments".to_string()});
		}
		match (eval.memory.value(args[0].clone()), eval.memory.value(args[1].clone())) {
			(Value::Primitive(Primitive::Int(i)), Value::Primitive(Primitive::Int(i2))) => {
				Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::Int(i%i2)))))
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => Err(Error {message:format!("First argument to `%` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	insert(memory, "~", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:"`neg` expects exactly 1 argument".to_string()});
		}
		match eval.memory.value(args[0].clone()) {
			Value::Primitive(Primitive::Int(i)) => {
				Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::Int(!i)))))
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => Err(Error {message:format!("Argument to integer `~` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	// Don't collapse with ~ because this will diverge once floats exist.
	insert(memory, "neg", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:"`neg` expects exactly 1 argument".to_string()});
		}
		match eval.memory.value(args[0].clone()) {
			Value::Primitive(Primitive::Int(i)) => {
				Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::Int(-i)))))
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => Err(Error {message:format!("Argument to numeric `neg` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	fn value_to_bool(v:Value) -> bool {
		match v {
			Value::Primitive(Primitive::Nil) => false,
			_ => true
		}
	}
	fn bool_to_handle(memory:&mut Memory, b:bool) -> Result<BuiltinReturn, Error> {
		Ok(BuiltinReturn::Value(memory.value_new(if b {
			Value::Primitive(Primitive::True)
		} else {
			Value::Primitive(Primitive::Nil)
		})))
	}

	// &&, || and ^^ are remarkably similar.
	macro_rules! insert_binary_logic {
		($name:expr, $op:expr, $short:expr) => { // Third is a fn that returns true if it's time to short circuit
			insert(memory, $name, Primitive::Builtin(|eval, args| {
				if args.len() < 2 {
					return Err(Error {message:format!("Too few args to `{}`", $name)});
				}
				let op = $op;
				let short_circuit = $short; // Is this optimization meaningless?

				let mut b = value_to_bool(eval.memory.value(args[0].clone()));
				'iter: for idx in 1..args.len() {
					let b2 = value_to_bool(eval.memory.value(args[idx].clone()));
					b = op(b, b2);
					if short_circuit(b) { break 'iter; }
				}
				bool_to_handle(&mut eval.memory, b)
			}));
		}
	}

	insert_binary_logic!("||", |x,y| x || y, |x:bool| x);  // Short circuit if true
	insert_binary_logic!("&&", |x,y| x && y, |x:bool| !x); // Short circuit if false
	insert_binary_logic!("^^", |x,y| x != y, |_| false); // Don't short circuit

	insert(memory, "!", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:"`!` expects exactly 1 argument".to_string()});
		}
		let b = !value_to_bool(eval.memory.value(args[0].clone()));
		bool_to_handle(&mut eval.memory, b)
	}));

	macro_rules! insert_equality {
		($name:expr, $invert:expr) => { // Third is a fn that returns true if it's time to short circuit
			insert(memory, $name, Primitive::Builtin(|eval, args| {
				let invert = $invert;
				if invert && args.len() > 2 {
					return Err(Error {message:format!("Too many args to `{}`", $name)});
				}
				if args.len() < 2 {
					return Err(Error {message:format!("Too few args to `{}`", $name)});
				}

				match eval.memory.value(args[0].clone()) {
					// TODO: Support, at *least*, comparing quotes
					Value::Primitive(p) => {
						for idx in 1..args.len() {
							let mut result = match eval.memory.value(args[idx].clone()) {
								Value::Primitive(p2) => p == p2,
								_ => false
							};
							result = result ^ invert;
							if !result {
								return Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::Nil)))) // Functionally I could return nil here but this is more "idiomatic"
							}
						}
						Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::True))))
					}
					// TODO: Value::Dict, Value::Array, local
					v @ _ => Err(Error {message:format!("First argument to `{}` unrecognized: {:?}", $name, v)}) // TODO: Display not Debug
				}
			}));
		}
	}

	insert_equality!("=", false);
	insert_equality!("!=", true); // FIXME: This is confusing because they all compare against #1.

	macro_rules! insert_comparison {
		($name:expr, $op:tt) => { // Third is a fn that returns true if it's time to short circuit
			insert(memory, $name, Primitive::Builtin(|eval, args| {
				if args.len() != 2 { // TODO: Allow >2?
					return Err(Error {message:format!("Expected exactly 2 args to `{}`", $name)});
				}

				match (eval.memory.value(args[0].clone()), eval.memory.value(args[1].clone())) {
					// TODO: Support, at *least*, comparing quotes
					(Value::Primitive(Primitive::Int(a)), Value::Primitive(Primitive::Int(b))) => {
						bool_to_handle(&mut eval.memory, a $op b)
					}
					(Value::Primitive(Primitive::String(a)), Value::Primitive(Primitive::String(b))) => {
						bool_to_handle(&mut eval.memory, a $op b)
					}
					// TODO: Value::Dict, Value::Array, local
					(a @ _, b @ _) => Err(Error {message:format!("Unrecognized arguments to `{}`: {:?} vs {:?}", $name, a, b)}) // TODO: Display not Debug
				}
			}))
		}
	}

	insert_comparison!("<", <);
	insert_comparison!(">", >);
	insert_comparison!("<=", <=);
	insert_comparison!(">=", >=);

	// --- Conversion/types ---

	macro_rules! insert_is {
		($name:expr, $pattern:pat) => {
			insert(memory, $name, Primitive::Builtin(|eval, args| {
				if args.len() != 1 {
					return Err(Error {message:format!("Expected exactly one argument to `{}`", $name)});
				}
				let value = match eval.memory.value(args[0].clone()) {
					// v @ Value::Primitive(Primitive::Nil) => v // Bring back if cells can ever change
					$pattern => true,
					_ => false
				};
				return bool_to_handle(&mut eval.memory, value);
			}));
		}
	}

	insert_is!("is-int", Value::Primitive(Primitive::Int(_)));
	insert_is!("is-string", Value::Primitive(Primitive::String(_)));
	insert_is!("is-builtin", Value::Primitive(Primitive::Builtin(_)));
	insert_is!("is-quote", Value::Quote);
	insert_is!("is-array", Value::Array);
	insert_is!("is-dict", Value::Dict);
	insert_is!("is-fn", Value::Fun);
	insert_is!("is-bool", Value::Primitive(Primitive::Nil) | Value::Primitive(Primitive::True));
	insert_is!("is-callable", Value::Array | Value::Fun | Value::Primitive(Primitive::Builtin(_)));

	insert(memory, "is-int", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("Expected exactly one argument to `is-int`")});
		}
		let value = match eval.memory.value(args[0].clone()) {
			// v @ Value::Primitive(Primitive::Nil) => v // Bring back if cells can ever change
			Value::Primitive(Primitive::Int(_)) => true,
			_ => false
		};
		return bool_to_handle(&mut eval.memory, value);
	}));

	insert(memory, "to-bool", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("Expected exactly one argument to `to-bool`")});
		}
		let value = match eval.memory.value(args[0].clone()) {
			// v @ Value::Primitive(Primitive::Nil) => v // Bring back if cells can ever change
			Value::Primitive(Primitive::Nil) | Value::Primitive(Primitive::True) =>
				return Ok(BuiltinReturn::Value(args[0].clone())), // Passthrough
			_ => Value::Primitive(Primitive::True)
		};
		Ok(BuiltinReturn::Value(eval.memory.value_new(value)))
	}));

	insert(memory, "to-int", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("Expected exactly one argument to `to-int`")});
		}
		let value = match eval.memory.value(args[0].clone()) {
		    Value::Primitive(Primitive::Nil) => Some(0),
		    Value::Primitive(Primitive::True) => Some(1),
		    //Value::Primitive(Primitive::Int(i)) => BuiltinReturn::Value(i),
		    Value::Primitive(Primitive::Int(_)) => return Ok(BuiltinReturn::Value(args[0].clone())), // Passthrough
		    Value::Primitive(Primitive::String(s)) => s.parse().ok(),
			v @ _ => return Err(Error {message:format!("First argument to `to-int` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		let value = value.map(|v|eval.memory.value_new(Value::Primitive(Primitive::Int(v))));
		Ok(option_to_value(value))
	}));

	insert(memory, "to-string", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("Expected exactly one argument to `to-int`")});
		}
		let value = match eval.memory.value(args[0].clone()) {
		    Value::Primitive(Primitive::String(_)) => return Ok(BuiltinReturn::Value(args[0].clone())),
		    v @ _ => format!("{}", v)
		};
		Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::String(value)))))
	}));

	// --- Data ---

	insert(memory, "make-array", Primitive::Builtin(|eval, args| {
		Ok(BuiltinReturn::Value(eval.memory.array_from_handles(args)))
	}));

	// TODO: I don't like this and I'd rather some kinda '(pair) form
	// FIXME: Dupes are allowed. Is that bad?
	insert(memory, "make-dict", Primitive::Builtin(|eval, args| {
		let dict = eval.memory.dict_new();
		if args.len() % 2 != 0 {
			return Err(Error {message:format!("Non-even number of args to `make-dict`")});
		}
		// TODO itertools chunks
		for idx in (0..args.len()).step_by(2) {
			let idx2 = idx + 1;
			match eval.memory.value(args[idx].clone()) {
				Value::Primitive(p) => {
					eval.memory.dict_set(dict.clone(), p, args[idx2].clone());
				}
				v @ _ => return Err(Error {message:format!("Argument {idx} (key #{}) to `make-dict` is not a primitive: {:?}", idx/2, v)}) // TODO: Display not Debug
			}
		}
		Ok(BuiltinReturn::Value(dict))
	}));

	insert(memory, "make-quote", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:"Expected exactly one argument to `make-quote`".to_string()});
		}
		Ok(BuiltinReturn::Value(eval.memory.quote_new(args[0].clone())))
	}));

	insert(memory, "unquote", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:"Expected exactly one argument to `quote`".to_string()});
		}
		match eval.memory.value(args[0].clone()) {
			// Get variable from scope.
			Value::Quote => {
				Ok(BuiltinReturn::Value(eval.memory.quote_get(args[0].clone())))
			}
			v @ _ => Err(Error {message:format!("First argument to `unquote` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	insert(memory, "len", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("Expected exactly one argument to `len`")});
		}
		let len = match eval.memory.value(args[0].clone()) {
			// TODO: Support, at *least*, comparing quotes
			Value::Primitive(Primitive::String(s)) => {
				s.len()
			}
			Value::Array => {
				eval.memory.array_len(args[0].clone())
			}
			Value::Dict => {
				eval.memory.dict_len(args[0].clone())
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => return Err(Error {message:format!("First argument to `len` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::Int(len as i64)))))
	}));

	// FIXME merge with "get" above
	insert(memory, "get", Primitive::Builtin(|eval, args| {
		if args.len() < 2 {
			return Err(Error {message:format!("Too few args to `get`")});
		}
		let handle_option = match eval.memory.value(args[0].clone()) {
			// TODO: Support, at *least*, comparing quotes
			Value::Primitive(Primitive::String(s)) => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(Primitive::Int(i)) => {
						let idx = i as usize;
						if i < 0 { // FIXME: Reduce code duplication here
							return Err(Error {message:format!("Negative index to `get`: {}", i)})
						}
						// This is O(N) :(
						s.chars().nth(idx).map(|ch|eval.memory.value_new(Value::Primitive(Primitive::String(ch.to_string()))))
					}
					v @ _ => return Err(Error {message:format!("Running `get` on string, expected integer index, got: {:?}", v)})
				}
			}
			Value::Array => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(Primitive::Int(i)) => {
						let idx = i as usize;
						if i < 0 {
							return Err(Error {message:format!("Negative index to `get`: {}", i)})
						}
						eval.memory.array_get(args[0].clone(), idx)
					}
					v @ _ => return Err(Error {message:format!("Running `get` on array, expected integer index, got: {:?}", v)})
				}
			}
			Value::Dict => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(p) => {
						eval.memory.dict_get(args[0].clone(), p)
					}
					v @ _ => return Err(Error {message:format!("Running `get` on dict, expected primitive index, got: {:?}", v)})
				}
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => return Err(Error {message:format!("First argument to `get` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		Ok(option_to_value(handle_option))
	}));

	insert(memory, "push", Primitive::Builtin(|eval, args| {
		if args.len() < 2 {
			return Err(Error {message:format!("Too few args to `push`")});
		}
		match eval.memory.value(args[0].clone()) {
			Value::Array => {
				for idx in 1..args.len() {
					eval.memory.array_push(args[0].clone(), args[idx].clone());
				}
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => return Err(Error {message:format!("First argument to `push` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		Ok(BuiltinReturn::None)
	}));

	insert(memory, "has", Primitive::Builtin(|eval, args| {
		if args.len() != 2 {
			return Err(Error {message:format!("`has` expects exactly 2 arguments")});
		}
		let has = match eval.memory.value(args[0].clone()) {
			// TODO: Support, at *least*, comparing quotes
			Value::Primitive(Primitive::String(s)) => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(Primitive::Int(i)) => {
						let idx = i as usize;
						if i < 0 {
							return Err(Error {message:format!("Negative index to `has`: {}", i)})
						}
						idx < s.len()
					}
					v @ _ => return Err(Error {message:format!("Running `get` on string, expected integer index, got: {:?}", v)})
				}
			}
			Value::Array => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(Primitive::Int(i)) => {
						let idx = i as usize;
						if i < 0 {
							return Err(Error {message:format!("Negative index to `has`: {}", i)})
						}
						idx < eval.memory.array_len(args[0].clone())
					}
					v @ _ => return Err(Error {message:format!("Running `get` on array, expected integer index, got: {:?}", v)})
				}
			}
			Value::Dict => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(p) => {
						eval.memory.dict_has(args[0].clone(), p)
					}
					v @ _ => return Err(Error {message:format!("Running `get` on dict, expected primitive index, got: {:?}", v)})
				}
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => return Err(Error {message:format!("First argument to `get` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		bool_to_handle(&mut eval.memory, has)
	}));

	insert(memory, "del", Primitive::Builtin(|eval, args| {
		if args.len() != 2 {
			return Err(Error {message:format!("`trunc` expects exactly one argument")});
		}
		match eval.memory.value(args[0].clone()) {
			// TODO: Support lots of other things
			Value::Dict => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(p) => {
						eval.memory.dict_del(args[0].clone(), p)
					}
					v @ _ => return Err(Error {message:format!("Running `del` on dict, expected primitive index, got: {:?}", v)})
				}
			}
			// TODO: local, maybe array?
			v @ _ => return Err(Error {message:format!("First argument to `get` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		Ok(BuiltinReturn::None)
	}));

	insert(memory, "truncate", Primitive::Builtin(|eval, args| {
		if args.len() != 2 {
			return Err(Error {message:format!("`del` expects exactly 2 arguments")});
		}
		match eval.memory.value(args[0].clone()) {
			Value::Array => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(Primitive::Int(i)) => {
						if i < 0 {
							return Err(Error {message:format!("Negative index to `truncate`: {}", i)})
						}
						eval.memory.array_truncate(args[0].clone(), i as usize)
					}
					v @ _ => return Err(Error {message:format!("Running `del` on dict, expected primitive index, got: {:?}", v)})
				}
			}
			v @ _ => return Err(Error {message:format!("First argument to `get` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		Ok(BuiltinReturn::None)
	}));

	insert(memory, "keys", Primitive::Builtin(|eval, args| {
		if args.len() > 1 {
			return Err(Error {message:format!("Too many args to `del`")});
		}
		let dict = if args.len() == 1 {
			match eval.memory.value(args[0].clone()) {
				// TODO: Support lots of other things
				Value::Dict => {
					args[0].clone()
				}
				// TODO: Value::Dict, Value::Array, local
				v @ _ => return Err(Error {message:format!("First argument to `keys` unrecognized: {:?}", v)}) // TODO: Display not Debug
			}
		} else {
			eval.memory.globals.clone()
		};
		let keys:Vec<MemHandle> = eval.memory.dict_keys(dict).into_iter().map(|x|eval.memory.value_new(Value::Primitive(x))).collect();
		let handle = eval.memory.array_from_handles(&keys);
		Ok(BuiltinReturn::Value(handle))
	}));

	insert(memory, "clone", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("`clone` expects exactly 1 argument")});
		}
		Ok(BuiltinReturn::Value(match eval.memory.value(args[0].clone()) {
			// FIXME: If it ever becomes possible to modify a Fun this will be inadequate
			Value::Primitive(_) | Value::Fun => args[0].clone(),
			Value::Quote => eval.memory.quote_clone(args[0].clone()),
			Value::Array => eval.memory.array_clone(args[0].clone()),
			Value::Dict => eval.memory.dict_clone(args[0].clone()),
		}))
	}));

	// --- inset/outset ---

	insert(memory, "inset", Primitive::Builtin(|eval, args| {
		let v: Vec<Value> = args.into_iter().map(|x|eval.memory.value(x.clone())).collect();
		match *v {
			[Value::Array, Value::Array] => {
				let len = eval.memory.array_len(args[1].clone());
				for idx in 0..len {
					let handle = eval.memory.array_get(args[1].clone(), idx).unwrap();
					match eval.memory.value(handle) {
						Value::Primitive(p @ Primitive::String(_)) => {
							let handle2 = eval.memory.array_get(args[0].clone(), idx).unwrap_or_else(||eval.memory.nil());
							eval.memory.dict_set(eval.memory.globals.clone(), p, handle2);
						}
						_ =>
							return Err(Error {message:format!("Argument 1 element {} to `inset` not recognized (wanted string)", idx)})
					}
				}
			}
/*
			[Value::Dict, Value::Array] => {

			}
			[Value::Dict, Value::Array, Value::Array] => {

			}
			[Value::Dict, Value::Dict, Value::Array] => {

			}
*/
			_ => return Err(Error {message:format!("Unrecognized arguments to `inset`")}) // TODO: Better errors
		}
		Ok(BuiltinReturn::None)
	}));

	insert(memory, "outset", Primitive::Builtin(|eval, args| {
		let v: Vec<Value> = args.into_iter().map(|x|eval.memory.value(x.clone())).collect();
		match *v {
			[Value::Array, Value::Array] => {
				let (len0, len1) = (eval.memory.array_len(args[0].clone()), eval.memory.array_len(args[1].clone()));
				for idx in 0..len1 {
					let handle = eval.memory.array_get(args[1].clone(), idx).unwrap();
					match eval.memory.value(handle) {
						Value::Primitive(p @ Primitive::String(_)) => {
							let handle2 = eval.memory.dict_get(eval.memory.globals.clone(), p).unwrap_or_else(||eval.memory.nil());
							if idx < len0 {
								eval.memory.array_set(args[0].clone(), idx, handle2);
							} else {
								eval.memory.array_push(args[0].clone(), handle2);
							}
						}
						_ =>
							return Err(Error {message:format!("Argument 1 element {} to `inset` not recognized (wanted string)", idx)})
					}
				}
			}
/*
			[Value::Dict, Value::Array] => {

			}
			[Value::Dict, Value::Array, Value::Array] => {

			}
			[Value::Dict, Value::Dict, Value::Array] => {

			}
*/
			_ => return Err(Error {message:format!("Unrecognized arguments to `inset`")}) // TODO: Better errors
		}
		Ok(BuiltinReturn::None)
	}));

	// --- File ---

	insert(memory, "file-allowed", Primitive::Builtin(|eval, _| {
		bool_to_handle(&mut eval.memory, eval.file_allow)
	}));

	// TODO: Support bytes for paths, input, output
	fn handle_to_path(eval: &Eval, name:&str, handle:MemHandle, nil_ok:bool) -> Result<Option<std::path::PathBuf>, Error> {
		if !eval.file_allow {
			return Err(Error {message:format!("Called `{name}` but fs is disabled")});
		}
		match eval.memory.value(handle) {
			Value::Primitive(Primitive::Nil) => {
				if nil_ok {
					Ok(None)
				} else {
					Err(Error {message:format!("Argument nil to `{name}` unrecognized")})
				}
			},
			Value::Primitive(Primitive::String(s)) => {
				Ok(Some(std::path::PathBuf::from(std::ffi::OsString::from(s))))
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => Err(Error {message:format!("First argument to `{name}` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}

	insert(memory, "file-exists", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("`file-exists` expects exactly 1 argument")});
		}
		let path = handle_to_path(&eval, "file-exists", args[0].clone(), false)?.unwrap(); // Nil impossible
		bool_to_handle(&mut eval.memory, std::fs::exists(path).map_err(|e|fmt_fs_error(e, "file-exists"))?)
	}));

	macro_rules! insert_fileopen {
		($name:expr, $target:ident, $limit:expr, $create:expr) => { // Third is just executed
			insert(memory, $name, Primitive::Builtin(|eval, args| {
				if args.len() < 1 {
					return Err(Error {message:format!("Too few args to `{}`", $name)});
				}
				if args.len() > $limit {
					return Err(Error {message:format!("Too many args to `{}`", $name)});
				}
				let path = handle_to_path(&eval, $name, args[0].clone(), true)?;
				if eval.$target.is_some() {
					eval.$target = None;
				}
				let open = $create;
				eval.$target = if let Some(path) = path {
					let file = open(&eval.memory, args, path);
					let file = file.map_err(|e|fmt_fs_error(e, $name))?;
					Some(file)
				} else {
					None
				};
				Ok(BuiltinReturn::None)
			}));
		}
	}

	insert_fileopen!("file-in", file_in, 1, |_, _, path| {
		std::fs::File::open(path).map(|f| std::io::BufReader::new(f))
	});

	// Arguments: path, create, truncate (!truncate implies append)
	insert_fileopen!("file-out", file_out, 3, |memory:&Memory, args:&[MemHandle], path| {
		let create = args.len() > 1 && value_to_bool(memory.value(args[1].clone()));
		let truncate = args.len() > 2 && value_to_bool(memory.value(args[2].clone()));
		let append = !truncate;

		std::fs::OpenOptions::new().write(true).create(create).truncate(truncate).append(append).open(path)
	});

	insert(memory, "read-line", Primitive::Builtin(|eval, args| {
		if args.len() > 0 {
			return Err(Error {message:format!("`file-line` expects no arguments")});
		}

		let s = if let Some(file) = &mut eval.file_in {
			file.by_ref().lines().next()
		} else {
			std::io::stdin().lock().lines().next()
		};

		s.map(|x|x.map(|s|eval.memory.value_new(Value::Primitive(Primitive::String(s)))).map_err(|e|fmt_fs_error(e, "read-line"))).transpose().map(option_to_value)
	}));

	// CONSIDER: Is it correct that at EOF this returns "" instead of nil?
	insert(memory, "read-all", Primitive::Builtin(|eval, args| {
		if args.len() > 0 {
			return Err(Error {message:format!("`read-all` expects no arguments")});
		}
		let mut s = String::new();
		let result = if let Some(file) = &mut eval.file_in {
			file.by_ref().read_to_string(&mut s)
		} else {
			std::io::stdin().lock().read_to_string(&mut s)
		};
		result.map_err(|e|fmt_fs_error(e, "read-all"))?;

		Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::String(s)))))
	}));

	// --- Functions ---

	fn fun_impl(name: &str, eval: &mut Eval, spec_body:MemHandle, spec_locals:Option<MemHandle>, spec_args:Option<MemHandle>, fun_name:Option<String>) -> Result<BuiltinReturn, Error> {
		fn filter_initial(memory:&mut Memory, handle:MemHandle) -> Result<MemHandle, Error> {
			match memory.value(handle.clone()) {
				Value::Quote => Ok(memory.quote_get(handle)),
				Value::Array => Err(Error {message:"Nested calls currently not supported for initial values".to_string()}),
				Value::Primitive(key @ Primitive::String(_)) => {
					let value = memory.dict_get(memory.globals.clone(), key.clone());
					if let Some(value) = value {
						Ok(value)
					} else {
						Err(Error {message:format!("Unrecognized variable in initials list: {}", key)})
					}
				}
				_ => Ok(handle)
			}
		}

		// Convert None spec_args to nil, throw on nonsense spec_args
		let fun_args = if let Some(args) = &spec_args {
			match eval.memory.value(args.clone()) {
				Value::Primitive(Primitive::Nil) => args.clone(),
				Value::Array =>	{
					let filtered = eval.memory.array_new();
					let argsn = eval.memory.array_len(args.clone());
					for idx in 0..argsn {
						let is_final = ||idx+1>=argsn;
						let item = eval.memory.array_get(args.clone(), idx).unwrap();
						let bad = |eval:&Eval, item| Err(Error {message:format!("`{name}` \"args\" list, item {idx} unrecognized: {:?}", eval.memory.value(item))});
						match eval.memory.value(item.clone()) {
							Value::Primitive(Primitive::String(s)) => {
								let nil = eval.memory.nil();
								let sub_item = if s.starts_with('&') {
									if is_final() {
										let s = if s.len() == 1 { "args" } else { &s[1..] };
										eval.memory.value_new(Value::Primitive(Primitive::String(s.to_string())))
									} else {
										return Err(Error {message:format!("`{name}` \"args\" list, item {idx}, & argument cannot have an inital value")})
									}
								} else {
									eval.memory.array_from_handles(&[item.clone(), nil])
								};
								eval.memory.array_push(filtered.clone(), sub_item);
							}
							Value::Array => {
								let itemn = eval.memory.array_len(item.clone());
								if 2 != itemn { return Err(Error {message:format!("`{name}` \"args\" list, item {idx} is not a pair (len {itemn})")}) }
								let sub_item = eval.memory.array_get(item.clone(), 0).unwrap();
								match eval.memory.value(sub_item.clone()) {
									Value::Primitive(Primitive::String(s)) => {
										if s.starts_with('&') {
											return Err(Error {message:format!("`{name}` \"args\" list, item {idx}, & argument cannot have an inital value")})
										}
									}
									_ => return bad(&eval, item.clone())
								}
								let pair = eval.memory.array_new();
								eval.memory.array_push(pair.clone(), sub_item);
								let sub_item_2 = eval.memory.array_get(item, 1).unwrap();
								let sub_item_2 = filter_initial(&mut eval.memory, sub_item_2)?;
								eval.memory.array_push(pair.clone(), sub_item_2);
								eval.memory.array_push(filtered.clone(), pair);
							}
							_ =>
								return bad(&eval, item.clone())
						}
					}
					filtered
				}
				v @ _ => {
					return Err(Error {message:format!("`{name}` expects array for arguments, got: {:?}", v)});
				}
			}
		} else {
			eval.memory.nil()
		};

		let fun_locals = if let Some(locals) = &spec_locals {
			match eval.memory.value(locals.clone()) {
				Value::Primitive(Primitive::Nil) |
				Value::Dict => locals.clone(), // FIXME: sanity check?
				Value::Array =>	{
					let filtered = eval.memory.dict_new();
					for idx in 0..eval.memory.array_len(locals.clone()) {
						let item = eval.memory.array_get(locals.clone(), idx).unwrap();
						let bad = |eval:&Eval, item| Err(Error {message:format!("`{name}` locals item {idx} unrecognized: {:?}", eval.memory.value(item))});
						match eval.memory.value(item.clone()) {
							Value::Primitive(k @ Primitive::String(_)) => {
								eval.memory.dict_set(filtered.clone(), k, item.clone());
							}
							Value::Array => {
								let [key, value] = &eval.memory.array_as_handles(item.clone())[..] else {
									return bad(&eval, item.clone())
								};
								let key = match eval.memory.value(key.clone()) {
									Value::Primitive(p @ Primitive::String(_)) => p,
									v @ _ =>
										return Err(Error {message:format!("`{name}` locals item {idx}, key is not a string: {:?}", v)})
								};
								let value = filter_initial(&mut eval.memory, value.clone())?;
								eval.memory.dict_set(filtered.clone(), key, value);
							}
							_ =>
								return bad(&eval, item.clone())

						}
					}
					filtered
				}
				_ => return Err(Error {message:format!("`{name}` expects array or dict for locals")})
			}
		} else {
			eval.memory.nil()
		};

		// Build spec_args
		let (fun_args, fun_locals) = match eval.memory.value(spec_body.clone()) {
			Value::Array => {
				(fun_args, fun_locals)
			}
			Value::Fun => {
				let spec_fun = eval.memory.fun_unpack(spec_body.clone());
				let fun_args =
					match (eval.memory.value(spec_fun.args.clone()), eval.memory.value(fun_args.clone())) {
						(Value::Array, Value::Primitive(Primitive::Nil)) => spec_fun.args.clone(),
						(Value::Primitive(Primitive::Nil), Value::Array) |
						(Value::Primitive(Primitive::Nil), Value::Primitive(Primitive::Nil)) =>
							fun_args.clone(),
						_ => return Err(Error {message:format!("`fn` given non-nil array, but body is a fn that already has an array")})
					};
				let fun_locals =
					match (eval.memory.value(spec_fun.locals.clone()), eval.memory.value(fun_locals.clone())) {
						(Value::Dict, Value::Primitive(Primitive::Nil)) =>
							eval.memory.dict_clone(spec_fun.locals.clone()),
						(Value::Primitive(Primitive::Nil), Value::Dict) |
						(Value::Primitive(Primitive::Nil), Value::Primitive(Primitive::Nil)) =>
							fun_locals.clone(),
						(Value::Dict, Value::Dict) => {
							let filtered = eval.memory.dict_clone(spec_fun.locals.clone());
							eval.memory.dict_pull(filtered.clone(), fun_locals);
							filtered
						}
						_ => panic!("Interpreter internal error") // Unreachable?
					};

				(fun_args, fun_locals)
			}
			_ => return Err(Error {message:format!("`fn` expects array for body")})
		};

		Ok(BuiltinReturn::Value(eval.memory.fun_new(
			crate::memory::Fun {name:fun_name, args: fun_args, locals: fun_locals, body: spec_body}
		)))
	}

	fn fun_impl_default_arguments(name:&str, eval: &mut Eval, args:&[MemHandle])  -> Result<BuiltinReturn, Error> {
		let argsn = args.len();
		if argsn < 1 {
			return Err(Error {message:format!("`{name}` expects at least 1 argument")});
		}
		if argsn > 4 {
			return Err(Error {message:format!("`{name}` expects at most 4 arguments")});
		}

		// Notice unusual "folding" left-side optional arguments scheme. This code is too complicated :(
		let spec_body = args[argsn-1].clone();
		let (spec_locals, spec_args, fun_name) = if argsn == 1 {
			(None, None, None)
		} else {
			let n_2 = args[argsn-2].clone();
			if argsn == 2 {
				if let Value::Primitive(Primitive::String(s)) = eval.memory.value(n_2.clone()) {
					(None, None, Some(s))
				} else {
					(Some(n_2), None, None)
				}
			} else {
				let n_3 = args[argsn-3].clone();
				if argsn == 3 {
					if let Value::Primitive(Primitive::String(s)) = eval.memory.value(n_3.clone()) {
						(Some(n_2), None, Some(s))
					} else {
						(Some(n_2), Some(n_3), None)
					}
				} else {
					match eval.memory.value(args[0].clone()) {
						Value::Primitive(Primitive::String(s)) => (Some(n_2), Some(n_3), Some(s)),
						v @ _ => return Err(Error {message:format!("Unrecognized name for `{name}`: {}", v)})
					}
				}
			}
		};

		fun_impl(name, eval, spec_body, spec_locals, spec_args, fun_name)
	}

	// TODO: In case of local defaults, re-execute to get values
	insert(memory, "fn", Primitive::Builtin(|eval, args| {
		fun_impl_default_arguments("fn", eval, args)
	}));

	insert(memory, "set-fn", Primitive::Builtin(|eval, args| {
		let argsn = args.len();
		if argsn < 2 {
			return Err(Error {message:format!("`fn` expects at least 2 arguments")});
		}
		if argsn > 4 {
			return Err(Error {message:format!("`fn` expects at most 4 arguments")});
		}

		let fun_name = match eval.memory.value(args[0].clone()) {
			Value::Primitive(Primitive::String(s)) => Some(s),
			v @ _ => return Err(Error {message:format!("Unrecognized name for `set-fn`: {}", v)})
		};

		// Notice unusual "left-side" optional arguments scheme
		let spec_body = args[argsn-1].clone();
		let spec_locals = if argsn>2 { Some(args[argsn-2].clone()) } else { None };
		let spec_args = if argsn>3 { Some(args[argsn-3].clone()) } else { None };

		let fun = fun_impl("set-fn", eval, spec_body, spec_locals, spec_args, fun_name)?;
		let BuiltinReturn::Value(fun) = fun else { panic!("Interpreter internal error"); };

		// This is a bad way to do it because if set shadows, it will break.
		// TODO: associate some builtins with number indices.
		let set = eval.memory.dict_get(eval.memory.globals.clone(), Primitive::String("set".to_string())).unwrap();
		let value = args[0].clone();

		Ok(BuiltinReturn::Push(vec![set, value, fun]))
	}));

	insert(memory, "do", Primitive::Builtin(|eval, args| {
		// Simple case: It's just a block. It's just a block!
		if args.len() == 1 {
			return Ok(BuiltinReturn::Push(vec![args[0].clone()]))
		}

		// Otherwise, simulate calling fn and invoking the result immediately.
		let fun = fun_impl_default_arguments("fn", eval, args)?;
		let BuiltinReturn::Value(fun) = fun else { panic!("Interpreter internal error"); };

		Ok(BuiltinReturn::Push(vec![fun]))
	}));

	// --- Specials ---

	insert(memory, "apply", Primitive::Builtin(|eval, args| {
		if args.len() != 2 {
			return Err(Error {message:"`apply` expects exactly 2 arguments".to_string()});
		}
		match eval.memory.value(args[1].clone()) {
			Value::Array => {
				let mut array = vec![args[0].clone()];
				array.append(&mut eval.memory.array_as_handles(args[1].clone()));

				Ok(BuiltinReturn::Push(array))
			},
			// TODO: Value::Dict, Value::Array, local
			v @ _ => Err(Error {message:format!("Second argument to `apply` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	insert(memory, "if", Primitive::Builtin(|eval, args| {
		let argsn = args.len();
		if argsn < 2 {
			return Err(Error {message:"Not enough arguments to `if`".to_string()});
		}
		if argsn > 4 {
			return Err(Error {message:"Too many arguments to `if`".to_string()});
		}
		let cond = value_to_bool(eval.memory.value(args[0].clone()));
		if cond {
			Ok(BuiltinReturn::Push(vec![args[1].clone()]))
		} else if argsn > 3 {
			match eval.memory.value(args[2].clone()) {
    			Value::Primitive(Primitive::Nil) => {
    				Ok(BuiltinReturn::Push(vec![args[3].clone()]))
    			}
    			v @ _ => Err(Error {message:format!("Expected `else` as third argument to `if`, got: {:?}", v)})
			}
		} else if argsn > 2 {
			Ok(BuiltinReturn::Push(vec![args[2].clone()]))
		} else {
			Ok(BuiltinReturn::None)
		}
	}));

	insert(memory, "else", Primitive::Nil);

	// --- Exposed internals ---

	insert(memory, "get-position", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("`get-position` expects exactly one argument")});
		}
		match eval.memory.value(args[0].clone()) {
			Value::Array => {
				let position = eval.memory.array_get_position(args[0].clone());
				let x = eval.memory.value_new(Value::Primitive(Primitive::Int(position.source as i64)));
				let y = eval.memory.value_new(Value::Primitive(Primitive::Int(position.line as i64)));
				let z = eval.memory.value_new(Value::Primitive(Primitive::Int(position.column as i64)));
				Ok(BuiltinReturn::Value(eval.memory.array_from_handles(&[x, y, z])))
			}
			v @ _ => Err(Error {message:format!("`get-position` expects array, got: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	insert(memory, "get-position-source", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("`get-position-source` expects exactly one argument")});
		}
		match eval.memory.value(args[0].clone()) {
			Value::Primitive(Primitive::Int(i)) => {
				if i >= 0 && (i as usize) < eval.source_tag.len() {
					Ok(BuiltinReturn::Value(eval.memory.value_new(Value::Primitive(Primitive::String(
						eval.source_tag[i as usize].clone()
					)))))
				} else {
					Ok(BuiltinReturn::Value(eval.memory.nil()))
				}
			}
			v @ _ => Err(Error {message:format!("`get-position-source` expects integer, got: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	insert(memory, "make-array-at", Primitive::Builtin(|eval, args| {
		if args.len() < 1 {
			return Err(Error {message:format!("`make-array-at` expects at least one argument")});
		}
		fn int_at(memory: &mut Memory, handle:&MemHandle, i:usize) -> Result<u32, Error> {
			let handle = memory.array_get(handle.clone(), i).unwrap();
			match memory.value(handle) {
				Value::Primitive(Primitive::Int(i)) => Ok(i as u32),
				v @ _ => Err(Error {message:format!("`make-array-at` argument 0: Array contains unexpected item {}", v)})
			}
		}
		match eval.memory.value(args[0].clone()) {
			Value::Array => {
				let position = match eval.memory.array_len(args[0].clone()) {
					0 => {
						eval.memory.array_get_position(args[0].clone())
					}
					2 => {
						ReaderPosition { source: int_at(&mut eval.memory, &args[0], 0)?, line: int_at(&mut eval.memory, &args[0], 1)?, column: 0 }
					}
					3 => {
						ReaderPosition { source: int_at(&mut eval.memory, &args[0], 0)?, line: int_at(&mut eval.memory, &args[0], 1)?, column: int_at(&mut eval.memory, &args[0], 2)? }
					}
					i @ _ => {
						return Err(Error {message:format!("`make-array-at` argument 0: Array has unexpected size {}", i)})
					}
				};
				Ok(BuiltinReturn::Value(
					eval.memory.array_from_handles_at(position, &args[1..])
				))
			}
			v @ _ => Err(Error {message:format!("`make-array-at` expects array for first argument, got: {:?}", v)})
		}
	}));

	insert(memory, "fn-unpack", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("`get-position` expects exactly one argument")});
		}
		match eval.memory.value(args[0].clone()) {
			Value::Fun => {
				let spec_fun = eval.memory.fun_unpack(args[0].clone());
				let name = if let Some(s) = spec_fun.name {
					eval.memory.value_new(Value::Primitive(Primitive::String(s)))
				} else {
					eval.memory.nil()
				};
				Ok(BuiltinReturn::Value(eval.memory.array_from_handles(&[name, spec_fun.args, spec_fun.locals, spec_fun.body])))
			}
			v @ _ => Err(Error {message:format!("`get-position` expects fn, got: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	// --- Oddballs ---

	// Takes any number of arguments, returns nil.
	insert(memory, "discard", Primitive::Builtin(|_, _| {
		Ok(BuiltinReturn::None)
	}));

	// Identity combinator: Takes one argument, returns it.
	insert(memory, "return", Primitive::Builtin(|_, args| {
		if args.len() != 1 {
			return Err(Error {message:"`return` expects exactly 1 argument".to_string()});
		}
		Ok(BuiltinReturn::Value(args[0].clone()))
	}));

	insert(memory, "fail", Primitive::Builtin(|eval, args| {
		let mut out = "".to_string();
		for arg in args {
			out.push_str(&format!("{}", eval.memory.value(arg.clone())));
		}
		Err(Error {message: out})
	}));

	// --- Constants ---

	insert(memory, "nil", Primitive::Nil);
	insert(memory, "true", Primitive::True);

	insert(memory, "sp", Primitive::String(" ".to_string()));
	insert(memory, "ln", Primitive::String("\n".to_string()));

	// FIXME: Spelling/capitalization
	insert(memory, "INT_MIN", Primitive::Int(std::i64::MIN));
	insert(memory, "INT_MAX", Primitive::Int(std::i64::MAX));

	// TODO: Either don't do this, or make a specific decision to overwrite it with real args
	{
		let args: Vec<MemHandle> = args.iter().map(|s|memory.value_new(Value::Primitive(Primitive::String(s.clone())))).collect();
		let args = memory.array_from_handles(&args);
		memory.dict_set(memory.globals.clone(), Primitive::String("args".to_string()), args)
	}
}
