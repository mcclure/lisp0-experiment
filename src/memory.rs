//! Memory management / allocation / garbage collection
// TODO: GC
// TODO: Drop handler on handles
// TODO: Special address 0

use crate::reader::{AstContent, AstNode, ReaderPosition};
use crate::eval;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::collections::{HashMap, VecDeque};
use std::fmt;

const MB:usize = 1024*1024;
const GB:usize = MB*1024;
const STARTING_SIZE:usize = MB;
const BUMP_OVER_OCCUPANCY:f32 = 0.5; // TODO: Make tunable?
const INFLECTION_SIZE:usize = GB;    // Size over which we start doubling and switch to incrementing. TODO: Make tunable?
const DEFAULT_LIMIT:usize = 4*GB;

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
	Dict,
	Fun
}

// Note: In the language and docs these are called "fn"s,
// But I call them "fun" pervasively in the Rust code because fn is a reserved word.
#[derive(Clone)]
pub struct Fun {
	pub name:Option<String>,
	pub args: MemHandle,
	pub locals: MemHandle,
	pub body: MemHandle
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
            Value::Fun => write!(f, "[fn]"),
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
	// Wacky: Every array knows what line it was made on? Fits in 56 byte rule but should this be a Feature?
	Array(ReaderPosition, Vec<MemAddr>),
	Dict(HashMap<Primitive, MemAddr>),
	Fun { name:Option<String>, args: MemAddr, locals: MemAddr, body: MemAddr },
	Hole,            // Used by scope only
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

impl Drop for MemHandleImpl {
	fn drop(&mut self) {
		let table_option = &mut self.parent.upgrade();
		if let Some(table) = table_option {
			let idx = self.idx;
			let mut table = table.borrow_mut();
			table.handles[idx] = None;
			table.free.push_back(idx)
		}
    }
}

pub type MemHandle = Rc<MemHandleImpl>;

pub struct Memory {
	spaces: [MemSpace;2], // Current space, space to collect into
	space_parity:bool,    // Use as index to "spaces" for current space
	handle_table:Rc<MemHandleTableCell<MemHandleTable>>, // Dispense handles from here
	pub globals: MemHandle,    // "Root"

	pub gc_size_limit:usize,
	gc_last_occupancy:f32
}

impl Memory {
	pub fn new_sized(starting_size:usize) -> Self {
		//println!("MemCell size: {}", std::mem::size_of::<MemCell>()); // Note: When making changes to MemCell, uncomment and verify this is still 56

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
			globals,

			gc_size_limit:DEFAULT_LIMIT,
			gc_last_occupancy:0.
		}
	}

	pub fn new() -> Self {
		Self::new_sized(STARTING_SIZE)
	}

	fn space_top(&self) -> usize {
		self.spaces[self.space_parity as usize].len()
	}

	fn alloc_internal(&mut self, data: MemCell) -> MemAddr {
		let capacity = self.spaces[self.space_parity as usize].capacity();
		if self.space_top() >= capacity {
			type Todo = VecDeque<MemCell>;

			fn desired_capacity(old_capacity:usize, last_occupancy:f32) -> usize {
				if last_occupancy > BUMP_OVER_OCCUPANCY { // TODO: literally cap somewhere?
					if old_capacity < INFLECTION_SIZE {
						old_capacity*2
					} else {
						old_capacity+GB
					}
				} else {
					old_capacity
				}
			}

			// For forward_ method, input is index in FROM, output is index in TO
			fn forward_one(space_from: &mut MemSpace, space_to: &mut MemSpace, root_from:MemAddr, todo:&mut Todo, offset_one:bool) -> MemAddr {
				if let MemCell::Forward(addr_to) = space_from[root_from] {
					addr_to
				} else {
					// This depends on math magic to work: We would like to push the cell to space_to, but
					// we need the cell to *not* be in space_to when we walk it in forward_all, or else
					// space_to will lock and we won't be able to push to it while walking. So we put the
					// cell itself in the todo list, and assume the place we'll land is the current len
					// plus the current queue size. This can be improved later by writing a custom Vec.
					// offset_one is because roots have an unusual relationship to the todo queue
					let addr_to = space_to.len() + todo.len() + offset_one as usize;
					let cell = std::mem::replace(&mut space_from[root_from], MemCell::Forward(addr_to));
					todo.push_back(cell);
					addr_to
				}
			}
			fn forward_all(space_from: &mut MemSpace, space_to: &mut MemSpace, root:MemAddr) -> MemAddr {
				let mut todo: Todo = Default::default();
				let root = forward_one(space_from, space_to, root, &mut todo, false);
				while let Some(mut cell) = todo.pop_front() {
					// TODO: "Pop out" the value instead of pulling it from todo
					match &mut cell { // Follow links
				        MemCell::Primitive(_) | MemCell::Hole => (),
				        MemCell::Quote(addr) =>
				        	*addr = forward_one(space_from, space_to, *addr, &mut todo, true),
				        MemCell::Array(_, vec) => {
				        	for addr in vec {
				        		*addr = forward_one(space_from, space_to, *addr, &mut todo, true);
				        	}
				        }
				        MemCell::Dict(hash_map) => {
				        	for addr in hash_map.values_mut() {
				        		*addr = forward_one(space_from, space_to, *addr, &mut todo, true);
				        	}
				        }
				        MemCell::Fun {..} => {
				        	todo!();
				        }
				        MemCell::Forward(_) => panic!("Interpreter internal error during GC"),
				    }
				    space_to.push(cell); // See "math magic" above
				}
				root
			}

			{
				let (space_from, space_to) = {
					let (zero, one) = self.spaces.split_at_mut(1);
					if self.space_parity {
						(&mut one[0], &mut zero[0])
					} else {
						(&mut zero[0], &mut one[0])
					}
				};

				let new_capacity = desired_capacity(capacity, self.gc_last_occupancy)
					.min(self.gc_size_limit);
				space_to.reserve_exact(new_capacity); // TODO: Assert space_to.len() is zero?

				let handles = &mut self.handle_table.borrow_mut().handles;

				#[cfg(feature = "debug-gc")]
				eprintln!("** DEBUG-GC: Beginning GC. Capacity: {capacity} New capacity: {new_capacity} Handles: {}", handles.len());

				// COLLECT
				for handle in handles {
					if let Some(addr) = handle { // FIXME: Truncate nones at end, that's silly
						*addr = forward_all(space_from, space_to, *addr);
					}
				}

				if space_to.len() >= new_capacity {
					panic!("Memory 100% full even after GC. Bailing"); // TODO return error?
				}

				// TODO: Assert space_to.len() is new_capacity?
				self.gc_last_occupancy = space_to.len() as f32 / new_capacity as f32;

				#[cfg(feature = "debug-gc")]
				eprintln!("** DEBUG-GC: Finished GC. Occupancy: {}", self.gc_last_occupancy); // TODO: Time?
			}

			self.space_mut().truncate(0); // Current space is now empty
			self.space_parity = !self.space_parity; // Flip spaces
		}
		let top = self.space_top();
		self.spaces[self.space_parity as usize].push(data);
		top
	}

	fn construct_internal(&mut self, src: AstNode) -> MemAddr {
		let data = match src.content {
			AstContent::Nil => MemCell::Primitive(Primitive::Nil),
			AstContent::True => MemCell::Primitive(Primitive::True),
			AstContent::String(s) => MemCell::Primitive(Primitive::String(s)),
			AstContent::Int(s) => MemCell::Primitive(Primitive::Int(s)),
			AstContent::Quote(c) => {
				// Terrible memory locality properties
				MemCell::Quote(self.construct_internal(*c))
			},
			AstContent::Group(v) => {
				let v2 = v.into_iter().map(|c| self.construct_internal(c)).collect();
				MemCell::Array(src.at, v2)
			}
		};
		self.alloc_internal(data)
	}

	pub fn construct(&mut self, src: AstNode) -> MemHandle {
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

	fn handle_to_addr(&self, handle:MemHandle) -> MemAddr {
		self.handle_table.borrow().handles[handle.idx].unwrap() // FIXME: Use unchecked?
	}

	fn space(&self) -> &MemSpace { &self.spaces[self.space_parity as usize] }
	fn space_mut(&mut self) -> &mut MemSpace { &mut self.spaces[self.space_parity as usize] }
	fn cell(&self, handle: MemHandle) -> &MemCell {
		let addr = self.handle_to_addr(handle);
		&self.space()[addr]
	}
	fn cell_mut(&mut self, handle: MemHandle) -> &mut MemCell {
		let addr = self.handle_to_addr(handle);
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
			MemCell::Array(_, _) => Value::Array,
			MemCell::Dict(_) => Value::Dict,
			MemCell::Fun{..} => Value::Fun,
			MemCell::Hole => panic!("Interpreter bug detected"),
			MemCell::Forward(_) => panic!("Memory corruption detected")
		}
	}

	pub fn value_to_cell_internal(&mut self, value:Value) -> MemCell {
		match value {
			Value::Primitive(p) => MemCell::Primitive(p),

			// These aren't recommended, but we have to put something, so make an "empty"
			Value::Quote => MemCell::Quote(self.alloc_internal(MemCell::Primitive(Primitive::Nil))),
			Value::Array => MemCell::Array(Default::default(), Default::default()),
			Value::Dict => MemCell::Array(Default::default(), Default::default()),
			Value::Fun => {
				let nil = self.nil();
				let addr = self.handle_to_addr(nil);
				MemCell::Fun { name:None, args:addr, locals:addr, body:addr }
			}
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

	pub fn quote_new(&mut self, handle:MemHandle) -> MemHandle {
		let inner_addr = self.handle_to_addr(handle);
		let addr = self.alloc_internal(MemCell::Quote(inner_addr));
		self.handle_new(addr)
	}

	pub fn quote_new_value(&mut self, value:Value) -> MemHandle {
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

	pub fn quote_set(&mut self, handle:MemHandle, handle2:MemHandle) {
		let addr2 = self.handle_to_addr(handle2);
		let cell = self.cell_mut(handle);
		let MemCell::Quote(addr) = cell else { panic!("Expected quote") };
		*addr = addr2;
	}

	pub fn quote_set_value(&mut self, handle:MemHandle, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let addr2 = self.alloc_internal(cell2);
		let cell = self.cell_mut(handle);
		let MemCell::Quote(addr) = cell else { panic!("Expected quote") };
		*addr = addr2;
	}

	pub fn quote_clone(&mut self, handle:MemHandle) -> MemHandle {
		let inner = self.quote_get(handle); // "Safe" but could be more efficient..
		self.quote_new(inner)
	}

	pub fn array_new(&mut self) -> MemHandle {
		let addr = self.alloc_internal(MemCell::Array(Default::default(), Default::default()));
		self.handle_new(addr)
	}

	pub fn array_new_with(&mut self, position:ReaderPosition) -> MemHandle {
		let addr = self.alloc_internal(MemCell::Array(position, Default::default()));
		self.handle_new(addr)
	}

	pub fn array_from_handles_with(&mut self, position:ReaderPosition, handles: &[MemHandle]) -> MemHandle {
		let addr = self.alloc_internal(MemCell::Primitive(Primitive::Nil)); // Is this suboptimal?
		let cell2 = MemCell::Array(position, handles.iter().map(|handle|self.handle_to_addr(handle.clone())).collect());
		let cell = &mut self.space_mut()[addr];
		*cell = cell2;
		self.handle_new(addr)
	}

	pub fn array_from_handles(&mut self, handles: &[MemHandle]) -> MemHandle {
		self.array_from_handles_with(Default::default(), handles)
	}

	// Clone array or crash
	pub fn array_as_handles(&mut self, handle: MemHandle) -> Vec<MemHandle> {
		let cell = self.cell(handle);
		let MemCell::Array(_,ary) = cell else { panic!("Expected array") };
		ary.clone().into_iter().map(|v| self.handle_new(v)).collect()
	}

	pub fn array_len(&mut self, handle:MemHandle) -> usize {
		let cell = self.cell_mut(handle);
		let MemCell::Array(_,ary) = cell else { panic!("Expected array") };
		ary.len()
	}

	pub fn array_get(&mut self, handle:MemHandle, idx:usize) -> Option<MemHandle> {
		let cell = self.cell_mut(handle);
		let MemCell::Array(_,ary) = cell else { panic!("Expected array") };

		if idx < ary.len() {
			let addr = ary[idx];
			Some(self.handle_new(addr))
		} else {
			None
		}
	}

	pub fn array_get_position(&self, handle:MemHandle) -> ReaderPosition {
		let cell = self.cell(handle);
		let MemCell::Array(pos,_) = cell else { panic!("Expected array") };
		*pos
	}

	pub fn array_set(&mut self, handle:MemHandle, idx:usize, dst:MemHandle) {
		let addr = self.handle_to_addr(dst);
		let cell = self.cell_mut(handle);
		let MemCell::Array(_,ary) = cell else { panic!("Expected array") };
		ary[idx] = addr;
	}

	pub fn array_push(&mut self, handle:MemHandle, dst:MemHandle) {
		let addr = self.handle_to_addr(dst);
		let cell = self.cell_mut(handle);
		let MemCell::Array(_,ary) = cell else { panic!("Expected array") };
		ary.push( addr );
	}

	pub fn array_set_value(&mut self, handle:MemHandle, idx:usize, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let addr2 = self.alloc_internal(cell2);
		let cell = self.cell_mut(handle);
		let MemCell::Array(_,ary) = cell else { panic!("Expected array") };
		ary[idx] = addr2;
	}

	pub fn array_push_value(&mut self, handle:MemHandle, idx:usize, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let addr2 = self.alloc_internal(cell2);
		let cell = self.cell_mut(handle);
		let MemCell::Array(_,ary) = cell else { panic!("Expected array") };
		ary.push(addr2);
	}

	pub fn array_truncate(&mut self, handle:MemHandle, len:usize) {
		let cell = self.cell_mut(handle);
		let MemCell::Array(_,ary) = cell else { panic!("Expected array") };
		ary.truncate(len);
	}

	pub fn array_clone(&mut self, handle:MemHandle) -> MemHandle {
		let inner = self.array_as_handles(handle); // "Safe" but could be more efficient..
		self.array_from_handles(&inner)
	}

	pub fn dict_new(&mut self) -> MemHandle {
		let cell = self.alloc_internal(MemCell::Dict(Default::default()));
		self.handle_new(cell)
	}

	pub fn dict_len(&mut self, handle: MemHandle) -> usize {
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.len()
	}

	pub fn dict_has(&mut self, handle:MemHandle, key:Primitive) -> bool {
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.contains_key(&key)
	}

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
		let addr = self.handle_to_addr(dst);
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.insert(key, addr);
	}

	pub fn dict_set_value(&mut self, handle:MemHandle, key:Primitive, value:Value) {
		let cell2 = self.value_to_cell_internal(value);
		let addr2 = self.alloc_internal(cell2);
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.insert(key, addr2);
	}

	pub fn dict_del(&mut self, handle: MemHandle, key:Primitive) {
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.remove(&key);
	}

	pub fn dict_keys(&mut self, handle: MemHandle) -> Vec<Primitive> {
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.keys().map(|x|x.clone()).collect()
	}

	pub fn dict_pull(&mut self, handle_dst:MemHandle, handle_src:MemHandle) {
		let keys = self.dict_keys(handle_src.clone()); // "Safe" but could be more efficient..
		for key in keys {
			let value = self.dict_get(handle_src.clone(), key.clone()).unwrap();
			self.dict_set(handle_dst.clone(), key, value);
		}
	}

	pub fn dict_clone(&mut self, handle:MemHandle) -> MemHandle {
		let dict = self.dict_new();
		self.dict_pull(dict.clone(), handle);
		dict
	}

	pub fn fun_new(&mut self, fun:Fun) -> MemHandle {
		let handle = self.value_new(Value::Fun);
		let cell2 = MemCell::Fun {
			name:fun.name.clone(),
			args:self.handle_to_addr(fun.args),
			locals:self.handle_to_addr(fun.locals),
			body: self.handle_to_addr(fun.body)
		};
		{
			let cell = self.cell_mut(handle.clone());
			*cell = cell2;
		}
		handle
	}

	pub fn fun_unpack(&mut self, handle:MemHandle) -> Fun {
		let (name, args, locals, body) = {
			let cell = self.cell(handle);
			let MemCell::Fun { name, args, locals, body } = cell else { panic!("Expected dict") };
			(name.clone(), *args, *locals, *body)
		};
		let args = self.handle_new(args);
		let locals = self.handle_new(locals);
		let body = self.handle_new(body);

		Fun { name, args, locals, body }
	}

	fn hole_new_alloc(&mut self) -> MemAddr {
		self.alloc_internal(MemCell::Hole)
	}

	pub fn is_hole(&mut self, handle:MemHandle) -> bool {
		matches!(self.cell(handle), MemCell::Hole)
	}

	pub fn dict_set_hole(&mut self, handle:MemHandle, key:Primitive) {
		let addr2 = self.hole_new_alloc();
		let cell = self.cell_mut(handle);
		let MemCell::Dict(dict) = cell else { panic!("Expected dict") };
		dict.insert(key, addr2);
	}
}
