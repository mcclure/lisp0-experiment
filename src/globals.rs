//! "Standard library"

use crate::eval::{Eval, Error};
use crate::memory::{Memory, MemHandle, Value, Primitive};

use std::fmt;

// Note: At present, it is assumed that all exec lists include the builtin itself as 0th argument.

fn insert(memory: &mut Memory, name:&str, primitive:Primitive) {
	memory.dict_set_value(memory.globals.clone(), Primitive::String(name.to_string()), Value::Primitive(primitive));
}

pub fn populate(memory: &mut Memory) {
	insert(memory, "set", Primitive::Builtin(|eval, args| {
		if args.len() < 2 {
			return Err(Error {message:"Too few args to `set`".to_string()});
		}

		match eval.memory.value(args[0].clone()) {
			// Set variable on scope.
			Value::Primitive(v @ Primitive::String(_)) => { // FIXME: Quote, not string.
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
			Value::Primitive(v @ Primitive::String(_)) => { // FIXME: Quote, not string.
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

	insert(memory, "sp", Primitive::String(" ".to_string()));
	insert(memory, "ln", Primitive::String("\n".to_string()));

	// FIXME: Spelling/capitalization
	insert(memory, "INT_MIN", Primitive::Int(std::i64::MIN));
	insert(memory, "INT_MAX", Primitive::Int(std::i64::MAX));
}