//! "Standard library"

use crate::eval::{Eval, Error};
use crate::memory::{Memory, MemHandle, Value, Primitive};

use std::fmt;

// Note: At present, it is assumed that all exec lists include the builtin itself as 0th argument.

fn insert(memory: &mut Memory, name:&str, primitive:Primitive) {
	memory.dict_set_value(memory.globals.clone(), Primitive::String(name.to_string()), Value::Primitive(primitive));
}

pub fn populate(memory: &mut Memory) {
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
		for arg in args {
			print!("{}", eval.memory.value(arg.clone())); // TODO: "Consume" input
		}
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
			Value::Primitive(Primitive::String(s)) => { // FIXME: Quote, not string.
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
			Value::Primitive(Primitive::Int(i)) => { // FIXME: Quote, not string.
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

	// --- Constants ---

	insert(memory, "sp", Primitive::String(" ".to_string()));
	insert(memory, "ln", Primitive::String("\n".to_string()));

	// FIXME: Spelling/capitalization
	insert(memory, "INT_MIN", Primitive::Int(std::i64::MIN));
	insert(memory, "INT_MAX", Primitive::Int(std::i64::MAX));
}
