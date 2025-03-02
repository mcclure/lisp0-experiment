//! Memory management / allocation / garbage collection

use crate::reader::AstContent;
use std::cell::Cell;
use std::rc::{Rc, Weak};
use std::collections::{HashMap, VecDeque};

const STARTING_SIZE:usize = 1024*1024;

type MemAddr = usize;

#[derive(Debug)]
pub enum Primitive {
	Nil,
	True,
	String(String),
	Int(i64),
	//Float(f64)
}

// TODO: Non-fixed size representation (might require unsafe?)
// TODO: Store vectors within MemSpaces
// TODO: Segregate spaces by type / type in pointer
#[derive(Debug)]
pub enum MemCell {
	Primitive(Primitive),
	Quote(MemAddr),
	Group(Vec<MemAddr>),
	Dict(HashMap<Primitive, MemAddr>),
	Forward(MemAddr) // Used during GC only
}

type MemSpace = Vec<MemCell>;

#[derive(Default)]
struct MemHandleTable {
	handles: Vec<Option<MemAddr>>, // All live handles
	free: VecDeque<usize>          // Indices of all None entries in handles
}

struct MemHandleImpl {
	idx: usize,            // Index in table.handles

	// Every handle implementation keeps a ref to the handle table
	// it indexes into, so that the Drop implementation can set the
	// table entry to None and push the index into the free queue.
	// The reference is weak; the handle is no good without the Memory.
	parent: Weak<Cell<MemHandleTable>>
}

pub type MemHandle = Rc<MemHandleImpl>;

pub struct Memory {
	spaces: [MemSpace;2], // Current space, space to collect into
	space_parity:bool,    // Use as index to "spaces" for current space
	space_top:usize,      // Allocate from this index
	handle_table:Rc<Cell<MemHandleTable>>, // Dispense handles from here
	pub globals: MemHandle    // "Root"
}

impl Memory {
	fn new_sized(starting_size:usize) -> Self {
		let mut space0:MemSpace = Vec::with_capacity(starting_size);;
		space0.push(MemCell::Dict(Default::default()));
		let mut handle_table = MemHandleTable::default();
		handle_table.handles.push(Some(0));
		let handle_table = Rc::new(Cell::new(handle_table));
		let globals = Rc::new(MemHandleImpl {idx:0, parent:Rc::downgrade(&handle_table)});

		Memory {
			spaces: [
				space0,
				Vec::with_capacity(0)
			],
			space_parity:false,
			space_top:0,
			handle_table,
			globals
		}
	}

	fn new() -> Self {
		Self::new_sized(STARTING_SIZE)
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
				MemCell::Group(v2)
			}
		};
		if self.space_top >= self.spaces[self.space_parity as usize].capacity() {
			panic!("Not ready to garbage collect");
		}
		let top = self.space_top;
		self.spaces[self.space_parity as usize].push(data);
		self.space_top += 1;
		top
	}
}
