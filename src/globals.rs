//! "Standard library"

use clap::builder::OsStringValueParser;

use crate::eval::{Eval, Error};
use crate::memory::{MemHandle, MemHandleImpl, Memory, Primitive, Value};

use std::fmt;
use std::io::stdin;
use std::io::{Write, Read, BufRead};

// Note: At present, it is assumed that all exec lists include the builtin itself as 0th argument.

fn insert(memory: &mut Memory, name:&str, primitive:Primitive) {
	memory.dict_set_value(memory.globals.clone(), Primitive::String(name.to_string()), Value::Primitive(primitive));
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
					Ok(None)
				} else {
					Err(Error {message:"Too many args to `set`".to_string()})
				}
			}
			// TODO: Value::Dict, Value::Array, local
			v @ _ => Err(Error {message:format!("First argument to `set` unrecognized: {:?}", v)}) // TODO: Display not Debug
		}
	}));

	insert(memory, "get", Primitive::Builtin(|eval, args| {
		if args.len() < 1 {
			return Err(Error {message:"Too few args to `set`".to_string()});
		}
		match eval.memory.value(args[0].clone()) {
			// Get variable from scope.
			Value::Primitive(v @ Primitive::String(_)) => {
				if args.len() == 1 {
					eval.memory.dict_get(eval.memory.globals.clone(), v);
					Ok(None)
				} else {
					Err(Error {message:"Too many args to `set`".to_string()})
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
		Ok(None)
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
		Ok(None)
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
				Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::String(s)))))
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
				Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::Int(i)))))
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
						Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::Int(i)))))
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
				Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::Int(i%i2)))))
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
				Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::Int(!i)))))
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
				Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::Int(-i)))))
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
	fn bool_to_handle(memory:&mut Memory, b:bool) -> Result<Option<MemHandle>, Error> {
		Ok(Some(memory.value_new(if b {
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
								return Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::Nil)))) // Functionally I could return nil here but this is more "idiomatic"
							}
						}
						Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::True))))
					}
					// TODO: Value::Dict, Value::Array, local
					v @ _ => Err(Error {message:format!("First argument to `{}` unrecognized: {:?}", $name, v)}) // TODO: Display not Debug
				}
			}));
		}
	}

	insert_equality!("=", false);
	insert_equality!("!=", true); // FIXME: This is confusing because they all compare against #1.

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
	insert_is!("is-bool", Value::Primitive(Primitive::Nil) | Value::Primitive(Primitive::True));
	insert_is!("is-callable", Value::Array | Value::Primitive(Primitive::Builtin(_)));

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
				return Ok(Some(args[0].clone())), // Passthrough
			_ => Value::Primitive(Primitive::True)
		};
		Ok(Some(eval.memory.value_new(value)))
	}));

	insert(memory, "to-int", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("Expected exactly one argument to `to-int`")});
		}
		let value = match eval.memory.value(args[0].clone()) {
		    Value::Primitive(Primitive::Nil) => Some(0),
		    Value::Primitive(Primitive::True) => Some(1),
		    //Value::Primitive(Primitive::Int(i)) => Some(i),
		    Value::Primitive(Primitive::Int(_)) => return Ok(Some(args[0].clone())), // Passthrough
		    Value::Primitive(Primitive::String(s)) => s.parse().ok(),
			v @ _ => return Err(Error {message:format!("First argument to `to-int` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		Ok(value.map(|v|eval.memory.value_new(Value::Primitive(Primitive::Int(v)))))
	}));

	insert(memory, "to-string", Primitive::Builtin(|eval, args| {
		if args.len() != 1 {
			return Err(Error {message:format!("Expected exactly one argument to `to-int`")});
		}
		let value = match eval.memory.value(args[0].clone()) {
		    Value::Primitive(Primitive::String(_)) => return Ok(Some(args[0].clone())),
		    v @ _ => format!("{}", v)
		};
		Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::String(value)))))
	}));

	// --- Data ---

	insert(memory, "make-array", Primitive::Builtin(|eval, args| {
		Ok(Some(eval.memory.array_from_handles(args)))
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
		Ok(Some(dict))
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
		Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::Int(len as i64)))))
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
		Ok(handle_option)
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
		Ok(None)
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
						idx < s.len()
					}
					v @ _ => return Err(Error {message:format!("Running `get` on string, expected integer index, got: {:?}", v)})
				}
			}
			Value::Array => {
				match eval.memory.value(args[1].clone()) {
					Value::Primitive(Primitive::Int(i)) => {
						let idx = i as usize;
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
			return Err(Error {message:format!("`del` expects exactly one argument")});
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
			// TODO: Value::Dict, Value::Array, local
			v @ _ => return Err(Error {message:format!("First argument to `get` unrecognized: {:?}", v)}) // TODO: Display not Debug
		};
		Ok(None)
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
		Ok(Some(handle))
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
				Ok(None)
			}));
		}
	}

	insert_fileopen!("file-in", file_in, 1, |_, _, path| {
		std::fs::File::open(path).map(|f| std::io::BufReader::new(f))
	});

	// Arguments: path, create, truncate
	insert_fileopen!("file-out", file_out, 3, |memory:&Memory, args:&[MemHandle], path| {
		let create = args.len() > 1 && value_to_bool(memory.value(args[1].clone()));
		let truncate = args.len() > 2 && value_to_bool(memory.value(args[2].clone()));

		std::fs::OpenOptions::new().write(true).create(create).truncate(truncate).open(path)
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

		s.map(|x|x.map(|s|eval.memory.value_new(Value::Primitive(Primitive::String(s)))).map_err(|e|fmt_fs_error(e, "read-line"))).transpose()
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

		Ok(Some(eval.memory.value_new(Value::Primitive(Primitive::String(s)))))
	}));

	// --- Oddballs ---

	// Takes any number of arguments, returns nil.
	insert(memory, "discard", Primitive::Builtin(|_, _| {
		Ok(None)
	}));

	// Identity combinator: Takes one argument, returns it.
	insert(memory, "return", Primitive::Builtin(|_, args| {
		if args.len() != 1 {
			return Err(Error {message:"`return` expects exactly 1 argument".to_string()});
		}
		Ok(Some(args[0].clone()))
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
