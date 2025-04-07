//! Turns unicode string iterator into parse tree

// If EVERYTHING'S broken
const TRACE_DEBUG:bool = false;

use std::borrow::BorrowMut;
use std::collections::HashSet;
use std::fmt;

type num = i64;

// "Quote" means something different to the Reader than elsewhere.
// To the Reader, "Quote" means "text between quotation marks".
// Elsewhere, "quote" means something is "wrapped" as with ' or ".

#[derive(Debug, Clone, Copy)]
struct ReaderPosition {
	source:u32, // source_tag index
	line:u32, // 1-indexed
	column:u32 // 1-indexed
}

// Special substate of Quote
#[derive(Debug, Clone, Copy, PartialEq)]
enum QuoteState {
	Normal,
	Raw,
	Backslash, // Last char was backslash
	BackslashSeekingNewline, // Last char was backslash followed by whitespace
	BackslashNewline // In backslash newline chomp
}

#[derive(Debug, Clone)]
enum ReadState { // FIXME: Could this be merged with the "ReadFrame" below?
	Scan(bool), // True if looking for BOM
	Comment(bool), // True if in backslash mode
	Backslash(bool), // True if newline cleared
	Minus(ReaderPosition),
	Identifier,
	Number,
	Quote(QuoteState) // is_raw, is_escaped
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum GroupLineState {
	Normal,
	Comma, // Same as Normal, but track differently for clearer message on stray comma.
	Line
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum GroupKind { // What parenthesis closes this group?
	None, // Can occur for quote (important) or non-Scan (unimportant) frames
	File(GroupLineState),  // "Toplevel"
	Round,
	Curly(GroupLineState), // True if "line" has started
	Square(GroupLineState) // True if last comma
}

#[derive(Debug)]
struct StackFrame {
	node: AstNode,    // Building
	group: GroupKind, // For parenthesis matching, line interpretation
}

impl StackFrame {
	fn new(node: AstNode, group: GroupKind) -> Self {
		Self { node, group }
	}
}

#[derive(Debug)]
pub enum AstContent {
	Nil,
	True,
	String(String),
	Int(i64),
	//Float(f64),
	Quote(Box<AstNode>),
	Group(Vec<AstNode>)
}

#[derive(Debug)]
pub struct AstNode {
	pub at:ReaderPosition,
	pub content:AstContent
}

#[derive(Debug)]
pub struct Output {
	pub source_tag: Vec<String>,
	pub source: AstNode
}

#[derive(Debug, Clone)]
pub struct Error {
	pub tag: String, // "Filename"
	pub at: ReaderPosition, // FIXME: source member meaningless
	pub message: String
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Syntax: {} line {} column {}: {}", self.tag, self.at.line, self.at.column, self.message)
	}
}

impl std::error::Error for Error {}

fn generate_illegal_chars() -> HashSet<char> {
	let mut illegal: HashSet<char> = Default::default();
	illegal.insert('\u{00A0}'); // No-break space
	illegal.insert('\u{1680}'); // Ogham space mark
	illegal.insert('\u{200B}'); // Zero width space
	illegal.insert('\u{FEFF}'); // Zero-width non-breaking space
	// TODO: Parens
	illegal
}

fn is_whitespace(ch: char) -> bool { // TODO UNICODE
	ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n'
}

fn is_num(ch:char) -> bool { // TODO UNICODE
	let ch = ch as u32;
	ch >= '0' as u32 && ch <= '9' as u32
}

fn parse_num(ch:char) -> num { // ASSUMES PREFILTERED
	return ch as num - '0' as num;
}

fn is_word_start(ch:char) -> bool { // TODO UNICODE
	let ch = ch as u32;
	(ch >= 'A' as u32 && ch <= 'Z' as u32) || (ch >= 'a' as u32 && ch <= 'z' as u32)
}

fn is_normal_quote_open(ch:char) -> bool { // TODO UNICODE
	ch == '"'
}

fn is_normal_quote_close(ch:char) -> bool { // TODO UNICODE
	is_normal_quote_open(ch)
}

fn is_raw_quote_open(ch:char) -> bool { // TODO UNICODE
	ch == '`'
}

fn is_raw_quote_close(ch:char) -> bool { // TODO UNICODE
	is_raw_quote_open(ch)
}

fn is_word(ch:char) -> bool {
	is_num(ch) || is_word_start(ch)
}

fn is_paren_open(ch:char) -> bool {
	ch == '(' || ch == '[' || ch == '{'
}

fn is_paren_close(ch:char) -> bool {
	ch == ')' || ch == ']' || ch == '}'
}

fn check_illegal(illegal:&HashSet<char>, at:&ReaderPosition, tag:&String, ch:char) -> Result<(), Error> {
	let illegal = &*illegal_chars;
	if illegal.contains(&ch) { // Illegal chars
		return Err(Error {at:*at, tag:tag.clone(), message:format!("Illegal unicode char: U+{:x}", ch as u32)});
	}
	Ok(())
}

static illegal_chars:std::sync::LazyLock<HashSet<char>> = std::sync::LazyLock::new(generate_illegal_chars);

fn die() -> ! {
	panic!("Reader internal error. Please run again with RUST_BACKTRACE=1");
}

// Merge 1 layer of the stack upward.
fn peel(stack: &mut Vec<StackFrame>) {
	loop {
		let top = stack.pop().unwrap().node;
		let into = &mut stack.last_mut().unwrap().node.content;
		match into {
			AstContent::Quote(bx) => {
				**bx = top;
			}
			AstContent::Group(v) => {
				v.push(top);
				break;
			}
			_ => die()
		}
	}
}

// Take input as well as a string identifying the source (such as a filename)
pub fn ast<T: std::io::Read>(mut chars: char_reader::CharReader<T>, tag:String, lisp:bool) -> Result<Output, Error> {
	let mut state = ReadState::Scan(true);
	let mut stack: Vec<StackFrame> = Default::default();
	const NO_POSITION: ReaderPosition = ReaderPosition{source:0, line:0, column:0};
	const PLACEHOLDER:AstNode =  AstNode { at:NO_POSITION, content:AstContent::Nil };
	let mut source = AstNode { at:NO_POSITION, content:AstContent::Group(Default::default())};
	let mut at = ReaderPosition{source:0, line:1, column:1};
	let mut last_cr = false; // For merging \r\n
	let illegal = &*illegal_chars;

	// Create initial "toplevel" group
	stack.push(StackFrame::new(source, GroupKind::File(GroupLineState::Normal)));

	while let Ok(Some(ch)) = chars.next_char() {
		// BOM
		if let ReadState::Scan(true) = state {
			state = ReadState::Scan(false); // BOM check done
			if ch == '\u{FEFF}' {           // Skip BOM
				continue;
			}
		}

		let is_newline = ch == '\r' || ch == '\n';
		if ch == '\n' && last_cr {
			last_cr = false;
			continue; // Do NOTHING, not even increment line counters
		}
		let is_comma = ch == ',';
		let is_comment_start = ch == '#';

		if TRACE_DEBUG { eprintln!("Parsing: `{ch}`"); }
		// Always aborts after one iteration, but is loop to allow continue
		// "break" for "finish character", "continue" for "retry character"
		'process: loop {
			if TRACE_DEBUG {
				let Some(StackFrame{group,..}) = stack.last() else { die(); };
				eprintln!("\tDepth: {} State: {:?} Group: {:?}", stack.len(), state.clone(), group);
			}
			match state.clone() {
			    ReadState::Scan(_) => { // "Normal"
			    	// Before anything, handle EOL in ls0 mode.
			    	// Logic is: If following symbol, close group; if whitespace before symbol, ignore;
			    	// if comma before symbol (or anything in ()), error.
					if !lisp && (is_comma || is_newline || is_comment_start) {
						let Some(StackFrame{group,..}) = stack.last() else { die(); };
						let group = group.clone();
						match group {
							GroupKind::Round => {
								return Err(Error {at, tag, message:format!("{} currently not allowed in (). Use \\.", if is_comma {"Comma"} else {"Newline"})});
							}
							GroupKind::File(line_state) | GroupKind::Square(line_state) | GroupKind::Curly(line_state) => {
								let is_line = line_state == GroupLineState::Line;
								if is_comma && !is_line { // FIXME: Wierd to call [] braces?
									return Err(Error {at, tag, message:format!("Unexpected comma {}", if line_state == GroupLineState::Comma {"after comma"} else {
										match group { GroupKind::File(_) => "at start of file", GroupKind::Square(_) => "after brackets", GroupKind::Curly(_) => "after braces", _=>unreachable!() }
									})});
								}
								if is_line {
				    				peel(&mut stack); // End of line and line has content.

									// Note shadowing of group, line_state here. Wow, a lot of lines of code dedicated to the comma error message here!
									let Some(StackFrame{group,..}) = stack.last_mut() else { die(); };
									match group {
										GroupKind::File(line_state) | GroupKind::Curly(line_state) | GroupKind::Square(line_state) => {
											*line_state = if is_comma {
												GroupLineState::Comma
											} else {
												GroupLineState::Normal
											}
										},
										_ => die() // GroupLineState::Line should only be a child of GroupLineState::Normal or GroupLineState::Comma.
									}
								}
								if !is_comment_start { // comments break right after this
									break 'process;
								}
							}
							GroupKind::None => () // Fall through and , will be illegal as below.
						};
					} else if lisp && is_comma {
						return Err(Error {at, tag, message:"Please don't use commas in LISP mode".to_string()});
					}

			    	if is_comment_start { // Comment
			    		state = ReadState::Comment(false);
			    		break 'process;
			    	}

			    	check_illegal(illegal, &at, &tag, ch)?;

			    	if is_whitespace(ch) {
			    		break 'process;
			    	}

			    	if ch == '\\' {
			    		state = ReadState::Backslash(false);
			    		break 'process;
			    	}

			    	// If we are still here, the character we are interpreting has "substance".
			    	// In other words, if we are in ls0 mode, thi sis potentially the start of a line.
			    	if !lisp {
						let Some(StackFrame{group,..}) = stack.last() else { die(); };
						match *group {
							GroupKind::File(line_state) | GroupKind::Curly(line_state) | GroupKind::Square(line_state) => {
								if line_state != GroupLineState::Line { // A line can be started!
									stack.push(StackFrame::new(
						    			AstNode {at, content:AstContent::Group(Default::default())},
						    			match *group { // Needs sugar
						    				GroupKind::File(_) => GroupKind::File(GroupLineState::Line),
						    				GroupKind::Curly(_) => GroupKind::Curly(GroupLineState::Line),
						    				GroupKind::Square(_) => GroupKind::Square(GroupLineState::Line),
						    				_ => unreachable!()
						    			}
						    		));
								}
							}
							_ => () // Nothing to do
						}
					}

			    	if ch == '-' {
			    		state = ReadState::Minus(at);

			    		break 'process;
			    	}

			    	if ch == '\'' { // '
				    	stack.push(StackFrame::new(
				    		AstNode {at,content:AstContent::Quote(Box::new(PLACEHOLDER))},
				    		GroupKind::None
				    	));

			    		break 'process;
			    	}

			    	if is_num(ch) {
			    		state = ReadState::Number;

				    	stack.push(StackFrame::new(
				    		AstNode {at,content:AstContent::Int(0)},
				    		GroupKind::None
				    	));

			    		continue 'process;
			    	}

			    	let normal_quote = is_normal_quote_open(ch);

			    	if normal_quote || is_raw_quote_open(ch) {
			    		state = ReadState::Quote(if normal_quote {
			    			QuoteState::Normal
			    		} else {
			    			QuoteState::Raw
			    		});

				    	//let Some(AstNode {content:AstContent::Group(v),..}) = stack.last();
				    	stack.push(StackFrame::new(
				    		AstNode {at,content:AstContent::Quote(Box::new(PLACEHOLDER))},
				    		GroupKind::None
				    	));
				    	stack.push(StackFrame::new(
				    		AstNode {at,content:AstContent::String("".to_string())},
				    		GroupKind::None
				    	));

			    		break 'process;
			    	}

			    	// Close parens
			    	if is_paren_close(ch) {
				    	let Some(StackFrame{group,..}) = stack.last() else { die(); };
				    	match (group, ch) {
				    		(GroupKind::Round, ')') => (),
				    		(GroupKind::Curly(line_state), '}') | (GroupKind::Square(line_state), ']') => {
				    			if *line_state == GroupLineState::Line {
				    				peel(&mut stack);
				    			}
				    		}
				    		_ => return Err(Error {at, tag, message:format!("Unbalanced extra {} parenthesis", ch)})
				    	}


			    		peel(&mut stack);

			    		break 'process;
			    	}

			    	// Close parens
			    	if is_paren_open(ch) {
			    		state = ReadState::Scan(false);

			    		match ch {
			    			'[' => { // (make-array ...)
			    				if lisp {
			    					return Err(Error {at, tag, message:format!("No [ support in lisp mode")});
			    				}
			    				stack.push(StackFrame::new(
			    					AstNode {at, content:AstContent::Group(vec![
			    						AstNode {at, content:AstContent::String("make-array".to_string())},
			    					])},
			    					GroupKind::Square(GroupLineState::Normal)
			    				));
			    			},
			    			'{' => { // '((...))
			    				if lisp {
			    					return Err(Error {at, tag, message:format!("No [ support in lisp mode")});
			    				}
			    				stack.push(StackFrame::new(
					    			AstNode {at, content:AstContent::Quote(Box::new(PLACEHOLDER))},
					    			GroupKind::None
					    		));
					    		stack.push(StackFrame::new(
					    			AstNode {at, content:AstContent::Group(Default::default())},
					    			GroupKind::Curly(GroupLineState::Normal)
					    		));
			    			},
			    			'(' => { // (...)
			    				stack.push(StackFrame::new(
					    			AstNode {at, content:AstContent::Group(Default::default())},
					    			GroupKind::Round
					    		));
			    			}
			    			_ => ()
			    		}

			    		break 'process;
			    	}

			    	// If we're still here, it must be a legal identifier character
		    		state = ReadState::Identifier;

			    	stack.push(StackFrame::new(
			    		AstNode {at,content:AstContent::String("".to_string())},
			    		GroupKind::None
			    	));
			    	continue 'process;
			    },
			    ReadState::Comment(in_backslash) => {
			    	if is_newline { // Ignore unless newline
			    		state = if in_backslash {
			    			ReadState::Backslash(true) // Still in backslash, newline cleared
			    		} else {
			    			ReadState::Scan(false) // Return to "Normal"
			    		}
			    	}
			    }
			    ReadState::Backslash(cleared_newline) => {
			    	if ch == '#' {
			    		state = ReadState::Comment(true); // Now in comment, in_backslash true
			    	} else if is_newline {
			    		state = ReadState::Backslash(true); // Still in backslash, newline cleared
			    	} else if !is_whitespace(ch) {
			    		if !cleared_newline {
			    			return Err(Error {at, tag, message:format!("Backslash may only be followed by a comment or newline, but saw: {}", ch)})  // TODO: Sanitize ch printout
			    		} else {
			    			state = ReadState::Scan(false); // Return to "Normal" and reprocess
			    			continue 'process;
			    		}
			    	}
			    },
			    ReadState::Minus(node_at) => {
			    	if is_num(ch) { // It's a number
			    		state = ReadState::Number;

				    	stack.push(StackFrame::new(
				    		AstNode {at:node_at, content:AstContent::Int(-parse_num(ch))},
				    		GroupKind::None
				    	));

				    	break 'process;
			    	}

			    	state = ReadState::Identifier; // It's not, and never was, a number
			    	stack.push(StackFrame::new(
			    		AstNode {at:node_at,content:AstContent::String("-".to_string())},
			    		GroupKind::None
			    	));

			    	continue 'process;
			    }
			    ReadState::Identifier | ReadState::Number => {
			    	check_illegal(illegal, &at, &tag, ch)?;

			    	let is_white = is_whitespace(ch);
			    	if is_white || is_paren_close(ch) || is_comma {
			    		state = ReadState::Scan(false);
			    		peel(&mut stack);

			    		if lisp && is_white {
			    			break 'process; // Tiny efficiency win(?)
			    		} else {
			    			continue 'process;
			    		}
			    	}

			    	if let ReadState::Identifier = state {
				    	if is_paren_open(ch) || is_normal_quote_open(ch) || is_raw_quote_open(ch) {
			    			return Err(Error {at, tag, message:format!("Illegal character for identifier: {}", ch)})  // TODO: Sanitize ch printout
				    	}

				    	let Some(StackFrame{node:AstNode {content:AstContent::String(ref mut s),..},..}) = stack.last_mut() else { die(); };
				    	s.push(ch);
			    	} else { // Number
				    	if !is_num(ch) {
				    		return Err(Error {at, tag, message:format!("Illegal character for number: {}", ch)})  // TODO: Sanitize ch printout
				    	}

				    	let Some(StackFrame{node:AstNode {content:AstContent::Int(ref mut i),..},..}) = stack.last_mut() else { die(); };
				    	let i2 = parse_num(ch);
				    	*i *= 10;
				    	*i += if *i >= 0 { i2 } else { -i2 };
			    	}
			    },
			    ReadState::Quote(quote_state) => {
			    	let closed = ||
			    		quote_state != QuoteState::Raw && is_normal_quote_close(ch)
				    	||  quote_state == QuoteState::Raw && is_raw_quote_close(ch);
				    let mut append: Option<char> = None;

					match quote_state {
						QuoteState::Normal | QuoteState::Raw => {
							if is_newline {
					    		return Err(Error {at, tag, message:format!("Newline inside string")})  // TODO: Sanitize ch printout
				    		} else if quote_state == QuoteState::Normal && ch == '\\' {
				    			state = ReadState::Quote(QuoteState::Backslash)
				    		} else {
				    			if closed() {
						    		state = ReadState::Scan(false);
						    		peel(&mut stack);

						    		break 'process;
				    			} else {
				    				append = Some(ch);
				    			}
				    		}
						}
						QuoteState::Backslash => {
							if is_newline {
								state = ReadState::Quote(QuoteState::BackslashNewline);
							} else {
								match ch {
									't' => append = Some('\t'),
									'n' => append = Some('\n'),
									'\\' => append = Some('\\'),
									'\"' => append = Some('"'),
									_ => {
										if is_whitespace(ch) {
											state = ReadState::Quote(QuoteState::BackslashSeekingNewline);
										} else {
											return Err(Error {at, tag, message:format!("Unrecognized backslash sequence \\{}", ch)})
										}
									}
								}
								if append.is_some() {
									state = ReadState::Quote(QuoteState::Normal);
								}
							}
						}
						QuoteState::BackslashSeekingNewline => {
							if is_newline {
								state = ReadState::Quote(QuoteState::BackslashNewline);
							} else if !is_whitespace(ch) {
								return Err(Error {at, tag, message:format!("Unrecognized backslash sequence: whitespace followed by {}", ch)})
							}
						}
						QuoteState::BackslashNewline => {
							if !is_whitespace(ch) {
								state = ReadState::Quote(QuoteState::Normal);
								continue 'process;
							}
						}
				    }

				    if let Some(ch2) = append {
				    	let Some(StackFrame{node:AstNode {content:AstContent::String(ref mut s),..},..}) = stack.last_mut() else { die(); };
			    		s.push(ch2);
				    }
			    },
			}
			break 'process;
		}

		// Increment
		if is_newline { at.line += 1; at.column = 1; }
		else { at.column += 1; }
		last_cr = ch == '\r';
	}

	// Final unpeel: Destroy symbol-in-progress
	if stack.len() > 1 {
		let into = &stack.last_mut().unwrap().node.content;
		match into {
			AstContent::Group(_) => (),
			AstContent::Quote(_) => {
				return Err(Error {at, tag, message:format!("Stray ' at end of input")})
			}
			_ => {
				if TRACE_DEBUG {
					eprintln!("Extra peel (word)");
				}

				peel(&mut stack);
			}
		}
	}
	if !lisp && stack.len() > 1 {
		let into = &stack.last_mut().unwrap();
		match into {
			StackFrame { node:AstNode { content:AstContent::Group(_), .. }, group:GroupKind::File(_) } => {
				if TRACE_DEBUG {
					eprintln!("Extra peel (file end)");
				}

				peel(&mut stack);
			}
			_ => ()
		}
	}
	if stack.len() > 1 {
		if TRACE_DEBUG {
			eprintln!("Final stack depth {}: {:?}", stack.len(), stack);
		}
		return Err(Error {at, tag, message:format!("Expected ) at end of input")})
	}

	Ok(Output {
		source_tag: vec![tag],
		source: stack.pop().unwrap().node
	})
}
