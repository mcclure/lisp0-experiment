// Turns unicode string iterator into parse tree

use std::collections::HashSet;
use std::fmt;

type num = i64;

#[derive(Debug, Clone)]
struct ReaderPosition {
	source:u32, // source_tag index
	line:u32, // 1-indexed
	column:u32 // 1-indexed
}

#[derive(Debug)]
enum ReadState { // FIXME: Could this be merged with the "ReadFrame" below?
	Scan(bool), // True if looking for BOM
	Comment(bool), // True if in backslash mode
	Backslash(bool), // True if newline cleared
	Minus(ReaderPosition),
	Identifier,
	Number,
	Quote(bool, u8) // is_raw, Number of quotes
	// TODO: '
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
	at:ReaderPosition,
	content:AstContent
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
	ch == ' ' || ch == '\r' || ch == '\n'
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
	is_normal_quote_close(ch)
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

static illegal_chars:std::sync::LazyLock<HashSet<char>> = std::sync::LazyLock::new(generate_illegal_chars);

fn die() -> ! {
	panic!("Reader internal error. Please run again with RUST_BACKTRACE=1");
}

// Take input as well as a string identifying the source (such as a filename)
pub fn ast<T: std::io::Read>(mut chars: char_reader::CharReader<T>, tag:String) -> Result<Output, Error> {
	let mut state = ReadState::Scan(true);
	let mut stack: Vec<AstNode> = Default::default();
	const NO_POSITION: ReaderPosition = ReaderPosition{source:0, line:0, column:0};
	const PLACEHOLDER:AstNode =  AstNode { at:NO_POSITION, content:AstContent::Nil };
	let mut source = AstNode { at:NO_POSITION, content:AstContent::Group(Default::default())};
	let mut at = ReaderPosition{source:0, line:1, column:1};
	let mut last_cr = false; // For merging \r\n
	let illegal = &*illegal_chars;

	stack.push(source);

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
			last_cr == false;
			continue; // Do NOTHING, not even increment line counters
		}

		// Always aborts after one iteration, but is loop to allow continue
		// "break" for "finish character", "continue" for "retry character"
		'process: loop {
			match &state {
			    ReadState::Scan(_) | ReadState::Identifier | ReadState::Number => { // "Normal"
			    	if illegal.contains(&ch) { // Illegal chars
			    		return Err(Error {at, tag, message:format!("Illegal unicode char: U+{:x}", ch as u32)});
			    	}

			    	if ch == '#' { // Comment
			    		state = ReadState::Comment(false);
			    		break 'process;
			    	}

			    	if ch == '\\' {
			    		state = ReadState::Backslash(false);
			    		break 'process;
			    	}

			    	if ch == '-' {
			    		state = ReadState::Minus(at);
			    	}

			    	if is_num(ch) {
			    		state = ReadState::Number;

				    	stack.push(AstNode {at,content:AstContent::Int(0)});

			    		continue 'process;
			    	}

			    	if is_word_start(ch) {
			    		state = ReadState::Identifier;

				    	stack.push(AstNode {at,content:AstContent::String("".to_string())});

			    		continue 'process;
			    	}

			    	let normal_quote = is_normal_quote_open(ch);

			    	if normal_quote || is_raw_quote_open(ch) {
			    		state = ReadState::Quote(!normal_quote, 1);

				    	//let Some(AstNode {content:AstContent::Group(v),..}) = stack.last();
				    	stack.push(AstNode {at,content:AstContent::Quote(Box::new(PLACEHOLDER))});
				    	stack.push(AstNode {at,content:AstContent::String("".to_string())});

			    		break 'process;
			    	}

			    	// Close parens
			    	if ch == ')' || ch == ']' || ch == '}' {
			    		if stack.len() <= 1 {
				    		return Err(Error {at, tag, message:format!("Unbalanced extra {} parenthesis", ch)});
				    	} else {
				    		// TODO
				    	}
			    	}

			    	// Close parens
			    	if ch == '(' || ch == '[' || ch == '{' {
			    		state = ReadState::Scan(false);

			    		match ch {
			    			'[' => {
			    				return Err(Error {at, tag, message:format!("No [ support yet")});
			    				stack.push(AstNode {at, content:AstContent::Group(vec![
			    					AstNode {at, content:AstContent::String("map".to_string())},
			    					AstNode {at, content:AstContent::String("eval".to_string())}
			    				])})
			    			},
			    			'{' => {
			    				return Err(Error {at, tag, message:format!("No {{ support yet")});
					    		stack.push(AstNode {at, content:AstContent::Quote(Box::new(PLACEHOLDER))}); // FIXME this box will be thrown away
					    		stack.push(AstNode {at, content:AstContent::Group(Default::default())});
			    			},
			    			_ => ()
			    		}
			    		stack.push(AstNode {at, content:AstContent::Group(Default::default())});

			    		continue 'process;
			    	}
			    },
			    ReadState::Comment(in_backslash) => {
			    	if is_newline { // Ignore unless newline
			    		state = if *in_backslash {
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

				    	stack.push(AstNode {at:*node_at, content:AstContent::Int(-parse_num(ch))});

				    	break 'process;
			    	}

			    	state = ReadState::Identifier; // It's not, and never was, a number
			    	continue 'process;
			    }
			    ReadState::Identifier => {
			    	if is_whitespace(ch) {
			    		// PUSH
			    		state = ReadState::Scan(false);
			    		continue 'process;
			    	}

			    	let Some(AstNode {content:AstContent::String(mut v),..}) = &stack.last() else { die(); };
			    	v.push(ch);
//			    	let mut (AstNode {_, content:Ast}) = &stack.last().unwrap();
			    	stack.push(AstNode {at, content:AstContent::String("".to_string())});
			    },
			    ReadState::Number => {
			    	if is_whitespace(ch) {
			    		state = ReadState::Scan(false);
			    	}
			    },
			    ReadState::Quote(is_raw, count) => {
			    	if is_newline {
			    		return Err(Error {at, tag, message:format!("Newline inside string")})  // TODO: Sanitize ch printout
			    	}
			    	if !*is_raw && is_normal_quote_close(ch)
			    	||  *is_raw && is_raw_quote_close(ch) {
			    		state = ReadState::Scan(false);
			    	}
			    },
			}
			break;
		}

		// Increment
		if is_newline { at.line += 1; at.column = 1; }
		else { at.column += 1; }
		last_cr = ch == '\r';
	}

	Ok(Output {
		source_tag: vec![tag],
		source
	})
}