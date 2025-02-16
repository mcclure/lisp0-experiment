// Turns unicode string iterator into parse tree

use std::collections::HashSet;
use std::fmt;

#[derive(Debug)]
enum ReadState {
	Scan(bool), // True if looking for BOM
	Comment(bool), // True if in backslash mode
	Backslash(bool), // True if newline cleared
	Identifier,
	Number,
	Quote(bool, u8) // is_raw, Number of quotes
	// TODO: '
}

#[derive(Debug)]
enum GroupKind {
	Normal,
	Array
}

#[derive(Debug)]
struct ReadFrame {

}

#[derive(Debug, Clone)]
struct ReaderPosition {
	source:u32, // source_tag index
	line:u32, // 1-indexed
	column:u32 // 1-indexed
}

#[derive(Debug)]
pub enum AstContent {
	Identifier(String),
	Int(i64),
	Float(f64),
	Quote(Box<AstContent>),
	String(String),
	Group(GroupKind, Vec<AstNode>)
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

fn is_num_start(ch:char) -> bool { // TODO UNICODE
	let ch = ch as u32;
	ch >= '0' as u32 && ch <= '9' as u32
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
	is_num_start(ch) || is_word_start(ch)
}

static illegal_chars:std::sync::LazyLock<HashSet<char>> = std::sync::LazyLock::new(generate_illegal_chars);

// Take input as well as a string identifying the source (such as a filename)
pub fn ast<T: std::io::Read>(mut chars: char_reader::CharReader<T>, tag:String) -> Result<Output, Error> {
	let mut state = ReadState::Scan(true);
	let mut stack: Vec<ReadFrame> = Default::default();
	let mut source = AstNode {at:ReaderPosition{source:0, line:0, column:0}, content:AstContent::Group(GroupKind::Normal, vec![])};
	let mut at = ReaderPosition{source:0, line:1, column:1};
	let mut last_cr = false; // For merging \r\n
	let illegal = &*illegal_chars;

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

		'process: loop { // Always aborts after one iteration, but is loop to allow continue
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

			    	if is_num_start(ch) {
			    		state = ReadState::Number;
			    		continue 'process;
			    	}

			    	if is_word_start(ch) {
			    		state = ReadState::Number;
			    		continue 'process;
			    	}

			    	if is_normal_quote_open(ch) {
			    		state = ReadState::Quote(false, 1);
			    		break 'process;
			    	}

			    	if is_raw_quote_open(ch) {
			    		state = ReadState::Quote(true, 1);
			    		break 'process;
			    	}

			    	// Close parens
			    	if ch == ')' || ch == ']' || ch == '}' {
			    		if (stack.len() <= 1) {
				    		return Err(Error {at, tag, message:format!("Unbalanced extra {} parenthesis", ch)});
				    	} else {
				    		// TODO
				    	}
			    	}

			    	// Close parens
			    	if ch == '(' || ch == '[' || ch == '{' {
			    		// TODO
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
			    ReadState::Identifier => {
			    	if is_whitespace(ch) {
			    		state = ReadState::Scan(false);
			    	}
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