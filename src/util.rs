// Functions shared between interpreter and userland

use std::borrow::Borrow;
use crate::reader;

pub fn ast_to_string(node:&reader::AstNode) -> String {
	match &node.content {
		reader::AstContent::Nil => "nil".to_string(),
		reader::AstContent::True => "true".to_string(),
		reader::AstContent::String(s) => format!("{s:?}"),
		reader::AstContent::Int(n) /* | AstContent::Float(n) */ => format!("{n}"),
		reader::AstContent::Quote(bx) => format!("'{}", ast_to_string(bx.borrow())),
		reader::AstContent::Group(v) => {
			let mut s = "(".to_string();
			let mut first = true;
			for n2 in v {
				if first { first = false; } else { s += ", "; }
				s += &ast_to_string(n2);
			}
			s += ")";
			s
		}
	}
}

pub fn is_lisp_filename(s:&str) -> bool {
	s.ends_with(".ls0") // FIXME: allow ".cl" or some other things?
}