//! Memory management / allocation / garbage collection

use crate::reader::AstContent;
use std::rc::Rc;

const STARTING_SIZE:usize = 1024*1024;

bitfield::bitfield! {
    struct MemAddr(u64);
    impl Debug;
    impl new;
    u64;
    idx, set_idx: 62, 0;
    space, set_space: 63, 63;
}

// TODO: Non-fixed size representation (might require unsafe?)
// TODO: Store vectors within MemSpaces
// TODO: Segregate spaces by type / type in pointer
pub enum MemCell {
	Nil,
	True,
	String(String),
	Int(i64),
	//Float(f64),
	Quote(MemAddr),
	Group(Vec<MemAddr>),
	Forward(MemAddr) // Used during GC only
}

type MemSpace = Vec<MemCell>;

pub struct Memory {
	spaces: [MemSpace;2],
	space_parity:bool,
	space_top:usize
}

impl Memory {
	fn new_sized(starting_size:usize) -> Self {
		Memory {
			spaces: [
				Vec::with_capacity(starting_size),
				Vec::with_capacity(0)
			],
			space_parity:false,
			space_top:0
		}
	}

	fn new() -> Self {
		Self::new_sized(STARTING_SIZE)
	}

	fn construct_internal(&mut self, src: AstContent) -> MemAddr {
		let data = match src {
			AstContent::Nil => MemCell::Nil,
			AstContent::True => MemCell::True,
			AstContent::String(s) => MemCell::String(s),
			AstContent::Int(s) => MemCell::Int(s),
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
		MemAddr::new(top as u64, self.space_parity as u64)
	}
}
