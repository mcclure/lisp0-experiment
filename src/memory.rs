//! Memory management / allocation / garbage collection
// TODO: GC
// TODO: Drop handler on handles

use crate::reader::AstContent;
use crate::eval;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::collections::{HashMap, VecDeque};
use std::fmt;

const STARTING_SIZE:usize = 1024*1024;

type MemAddr = usize;

type MemHandleTableCell<T> = RefCell<T>; // TODO: Unsafe form

#[derive(Clone, Debug, PartialEq, Eq, Hash)] // TODO: Add display
pub enum Primitive {
	Nil,
	True,
	String(String),
	Int(i64),
	Builtin(eval::Builtin), // Cannot be constructed (FIXME: include name?)
	//Float(f64)
}

impl fmt::Display for Primitive {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Write strictly the first element into the supplied output
        // stream: `f`. Returns `fmt::Result` which indicates whether the
        // operation succeeded or failed. Note that `write!` uses syntax which
        // is very similar to `println!`.
        match self {
            Primitive::Nil =>  write!(f, "[nil]"),
            Primitive::True => write!(f, "[true]"),
            Primitive::String(s) => write!(f, "{s}"),
            Primitive::Int(i) => write!(f, "{i}"),
            Primitive::Builtin(_) => write!(f, "[builtin]")
        }
    }
}

#[derive(Debug)] // TODO: Add display.
pub enum Value {
	Primitive(Primitive),
	Quote,
	Array,
	Dict
}

impl fmt::Display for Value {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Write strictly the first element into the supplied output
        // stream: `f`. Returns `fmt::Result` which indicates whether the
        // operation succeeded or failed. Note that `write!` uses syntax which
        // is very similar to `println!`.
        match self {
            Value::Primitive(p) =>  write!(f, "{p}"),
            Value::Quote => write!(f, "[quote]"),
            Value::Array => write!(f, "[array]"), // TODO: size would be nice.
            Value::Dict => write!(f, "[dict]"),
        }
    }
}
// TODO: Non-fixed size representation (might require unsafe?)
// TODO: Store vectors within MemSpaces
// TODO: Segregate spaces by type / type in pointer
#[derive(Debug)]
enum MemCell {
	Primitive(Primitive),
	Quote(MemAddr),
	Array(Vec<MemAddr>),
	Dict(HashMap<Primitive, MemAddr>),
	Forward(MemAddr) // Used during GC only
}

type MemSpace = Vec<MemCell>;

#[derive(Default)]
struct MemHandleTable {
	handles: Vec<Option<MemAddr>>, // All live handles
	free: VecDeque<usize>          // Indices of all None entries in handles
}

#[derive(Clone)]
pub struct MemHandleImpl {
	idx: usize,            // Index in table.handles

	// Every handle implementation keeps a ref to the handle table
	// it indexes into, so that the Drop implementation can set the
	// table entry to None and push the index into the free queue.
	// The reference is weak; the handle is no good without the Memory.
	parent: Weak<MemHandleTableCell<MemHandleTable>>
}

pub type MemHandle = Rc<MemHandleImpl>;

pub struct Memory {
	spaces: [MemSpace;2], // Current space, space to collect into
	space_parity:bool,    // Use as index to "spaces" for current space
	handle_table:Rc<MemHandleTableCell<MemHandleTable>>, // Dispense handles from here
	pub globals: MemHandle    // "Root"
}

impl Memory {
	pub fn new_sized(starting_size:usize) -> Self {
		let mut space0:MemSpace = Vec::with_capacity(starting_size);;
		space0.push(MemCell::Dict(Default::default()));
		let mut handle_table = MemHandleTable::default();
		handle_table.handles.push(Some(0));
		let handle_table = Rc::new(MemHandleTableCell::new(handle_table));
		let globals = Rc::new(MemHandleImpl {idx:0, parent:Rc::downgrade(&handle_table)});

		Memory {
			spaces: [
				space0,
				Vec::with_capacity(0)
			],
			space_parity:false,
			handle_table,
			globals
		}
	}

	pub fn new() -> Self {
		Self::new_sized(STARTING_SIZE)
	}

	fn space_top(&self) -> usize {
		self.spaces[self.space_parity as usize].len()
	}

	fn alloc_internal(&mut self, data: MemCell) -> MemAddr {
		if self.space_top() >= self.spaces[self.space_parity as usize].capacity() {
			panic!("Code can't garbage collect yet");
		}
		let top = self.space_top();
		self.spaces[self.space_parity as usize].push(data);
		top
	}

	fn construct_internal(&mut self, src: AstContent) -> MemAddr {
		let data = match src {
			AstContent::Nil => MemCell::Primitive(Primitive::Nil),
			AstContent::True => MemCell::Primitive(Primitive::True),
			AstContent::String(s) => MemCell::Primitive(Primitive::String(s)),
			AstContent::Int(s) => MemCell::Primitive(Primitive::Int(s)),
			AstContent::Quote(c) => {
				// Terrible memory locality properties
				MemCell::Quote(self.construct_internal(c.content))
			},
			AstContent::Group(v) => {
				let v2 = v.into_iter().map(|c| self.construct_internal(c.content)).collect();
				MemCell::Array(v2)
			}
		};
		self.alloc_internal(data)
	}

	pub fn construct(&mut self, src: AstContent) -> MemHandle {
		let addr = self.construct_internal(src);
		self.handle_new(addr)
	}

	fn handle_new(&mut self, addr:MemAddr) -> MemHandle {
		let handle_value = Some(addr);
		let mut handle_table = self.handle_table.borrow_mut();
		let idx = if handle_table.free.len() > 0 {
			let idx = handle_table.free.pop_front().unwrap();
			handle_table.handles[idx as usize] = handle_value;
			idx
		} else {
			let idx = handle_table.handles.len();
			handle_table.handles.push(handle_value);
			idx
		};
		let r = Rc::new(MemHandleImpl { idx, parent:Rc::downgrade(&self.handle_table) });
		r
	}

	fn space(&self) -> &MemSpace { &self.spaces[self.space_parity as usize] }
	fn space_mut(&mut self) -> &mut MemSpace { &mut self.spaces[self.space_parity as usize] }
	fn cell(&self, handle: MemHandle) -> &MemCell {
		let addr = self.handle_table.borrow().handles[handle.idx].unwrap(); // FIXME: Use unchecked?
		&self.space()[addr]
	}
	fn cell_mut(&mut self, handle: MemHandle) -> &mut MemCell {
		let addr = self.handle_table.borrow().handles[handle.idx].unwrap();
		&mut self.space_mut()[addr]
	}

	// Why would this be useful?
	// pub fn set_copy(&mut self, dst:MemHandle, set:MemHandle) {
	// 	let cell2 = (*self.cell(set)).clone();
	// 	let cell = self.cell_mut(dst);
	// 	*cell = cell2; // Does this work right with vec/hashmap?
	// }

	pub fn nil(&mut self) -> MemHandle {
		self.value_new(Value::Primitive(Primitive::Nil))
	}

	pub fn value(&self, handle: MemHandle) -> Value {
		match self.cell(handle) {
			MemCell::Primitive(p) => Value::Primitive(p.clone()), // Is string clone a problem?
			MemCell::Quote(_) => Value::Quote,
			MemCell::Array(_) => Value::Array,
			MemCell::Dict(_) => Value::Dict,
			MemCell::Forward(_) => panic!("Memory corruption detected")
		}
	}

	pub fn value_to_cell_internal(&mut self, value:Value) -> MemCell {
		match value {
			Value::Primitive(p) => MemCell::Primitive(p),

			// These aren't recommended, but we have to put something, so make an "empty"
			Value::Quote => MemCell::Quote(self.alloc_internal(MemCell::Primitive(Primitive::Nil))),
			Value::Array => MemCell::Array(Default::default()),
			Value::Dict => MemCell::Array(Default::default()),
		}
	}

	// Note: Several of the below hold on to pointers across collections, which isn't valid.
	pub fn set_value(&mut self, handle: MemHandle, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let cell = self.cell_mut(handle);
		*cell = cell2
	}

	pub fn value_new(&mut self, value:Value) -> MemHandle {
		let cell = self.value_to_cell_internal(value);
		let addr = self.alloc_internal(cell);
		self.handle_new(addr)
	}

	pub fn quote_new(&mut self, value:Value) -> MemHandle {
		let cell = self.value_to_cell_internal(value);
		let inner_addr = self.alloc_internal(cell);
		let addr = self.alloc_internal(MemCell::Quote(inner_addr));
		self.handle_new(addr)
	}

	pub fn quote_get(&mut self, handle:MemHandle) -> MemHandle {
		let cell = self.cell(handle);
		let MemCell::Quote(addr) = cell else { panic!("Expected quote") };
		self.handle_new(*addr)
	}

	pub fn quote_set(&mut self, handle:MemHandle, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let addr2 = self.alloc_internal(cell2);
		let cell = self.cell_mut(handle);
		let MemCell::Quote(addr) = cell else { panic!("Expected quote") };
		*addr = addr2;
	}

	pub fn array_new(&mut self) -> MemHandle {
		let addr = self.alloc_internal(MemCell::Array(Default::default()));
		self.handle_new(addr)
	}

	// Clone array or crash
	pub fn array_as_handles(&mut self, handle: MemHandle) -> Vec<MemHandle> {
		let cell = self.cell(handle);
		let MemCell::Array(ary) = cell else { panic!("Expected array") };
		ary.clone().into_iter().map(|v| self.handle_new(v)).collect()
	}

	pub fn array_len(&mut self, handle:MemHandle) -> usize {
		let cell = self.cell_mut(handle);
		let MemCell::Array(ary) = cell else { panic!("Expected array") };
		ary.len()
	}

	pub fn array_get(&mut self, handle:MemHandle, idx:usize) -> Option<MemHandle> {
		let cell = self.cell_mut(handle);
		let MemCell::Array(ary) = cell else { panic!("Expected array") };

		if idx < ary.len() {
			let addr = ary[idx];
			Some(self.handle_new(addr))
		} else {
			None
		}
	}

	pub fn array_set(&mut self, handle:MemHandle, idx:usize, dst:MemHandle) {
		let cell = self.cell_mut(handle);
		let MemCell::Array(ary) = cell else { panic!("Expected array") };
		ary[idx] = dst.idx;
	}

	pub fn array_push(&mut self, handle:MemHandle, idx:usize, dst:MemHandle) {
		let cell = self.cell_mut(handle);
		let MemCell::Array(ary) = cell else { panic!("Expected array") };
		ary.push(dst.idx);
	}

	pub fn array_set_value(&mut self, handle:MemHandle, idx:usize, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let addr2 = self.alloc_internal(cell2);
		let cell = self.cell_mut(handle);
		let MemCell::Array(ary) = cell else { panic!("Expected array") };
		ary[idx] = addr2;
	}

	pub fn array_push_value(&mut self, handle:MemHandle, idx:usize, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let addr2 = self.alloc_internal(cell2);
		let cell = self.cell_mut(handle);
		let MemCell::Array(ary) = cell else { panic!("Expected array") };
		ary.push(addr2);
	}

	pub fn dict_new(&mut self) -> MemHandle {
		let cell = self.alloc_internal(MemCell::Dict(Default::default()));
		self.handle_new(cell)
	}

	// TODO: dict_size, dict_keys
	pub fn dict_get(&mut self, handle:MemHandle, key:Primitive) -> Option<MemHandle> {
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		if dict.contains_key(&key) {
			let addr = dict[&key];
			Some(self.handle_new(addr))
		} else {
			None
		}
	}

	pub fn dict_set(&mut self, handle:MemHandle, key:Primitive, dst:MemHandle) {
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.insert(key, dst.idx);
	}

	pub fn dict_set_value(&mut self, handle:MemHandle, key:Primitive, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let addr2 = self.alloc_internal(cell2);
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.insert(key, addr2);
	}
}
