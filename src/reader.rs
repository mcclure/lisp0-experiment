// Turns unicode string iterator into parse tree

use std::collections::HashSet;
use std::fmt;

#[derive(Debug)]
enum QuoteKind {
	Normal, // "
	Raw     // `
}

#[derive(Debug)]
enum ReadState {
	Scan(bool), // True if looking for BOM
	Comment(bool), // True if in backslash mode
	Backslash, // In comment?
	Identifier,
	Number,
	Quote(QuoteKind, u8) // Number of quotes
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
	Quote(QuoteKind, Box<AstContent>),
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

		'process: { // For early abort
			match state {
			    ReadState::Scan(_) | ReadState::Identifier | ReadState::Number => {
			    	// Illegal chars
			    	if illegal.contains(&ch) {
			    		return Err(Error {at, tag, message:format!("Illegal unicode char: U+{:x}", ch as u32)});
			    	}

			    	// Close parens
			    	if ch == ')' || ch == ']' || ch == '}' {
			    		return Err(Error {at, tag, message:format!("Unbalanced {} parenthesis", ch)});
			    	}
			    },
			    _ => (),
			}
		}

		// Increment
		if ch == '\r' || (ch == '\n' && !last_cr) { at.line += 1; at.column = 1; }
		else { at.column += 1; }
		last_cr = ch == '\r';
	}

	Ok(Output {
		source_tag: vec![tag],
		source
	})
}