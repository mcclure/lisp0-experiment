This language is unnamed, and unstable.

# Parsing text

Input files are UTF-8. A beginning BOM will be discarded.

A file is a series of statements. Statements are separated by newlines or commas. A newline is a CR, LF, or CRLF sequence.

When inside of a comment: All input is discarded until the newline is reached.

When neither inside a comment or string, if a line ends with `\`, all whitespace and comments following that `\` leading up to the next non-whitespace character are treated as a single space (in other words, a `\` may consume multiple blank lines). Placing anything after a `\` other than whitespace or comments are an error.

A statement is a series of symbols. A symbol is:

	* An identifier, as described below.
	* A number, as described below.
	* Any number of `'` marks followed by a symbol.
	* A string, as described below.
	* A paired `()`, `{}` or `[]` delimiter containing zero or more statements.

An identifier is a sequence of characters not including `#`, `\` or any whitespace, quote, delimiter character; and which does not a legal number as a prefix.

A string is:
	* A `"` character, followed by a sequence of non-newline characters and ending with another ". Within a `"` string, a backslash followed by another character has special meaning:
		* `\\t` - a tab
		* `\\n` - a tab
		* `\\\\` - a backslash
		* `\"` - a quote (does not terminate string)

	* A `\`` character, followed by a sequence of non-newline characters and ending with another `.

## Unicode oddness

Except in the interior of strings or comments, and except when U+FEFF is the opening BOM:

* U+00A0 NO-BREAK SPACE, U+1680 OGHAM SPACE MARK, U+200B ZERO WIDTH SPACE and U+FEFF ZERO WIDTH NO-BREAK SPACE are illegal.
* U+201C LEFT DOUBLE QUOTATION MARK and U+201D RIGHT DOUBLE QUOTATION MARK are equivalent to `"` (that is, smart double quotes may open or close a `"` type string).
* U+2018 LEFT SINGLE QUOTATION MARK and U+2019 RIGHT SINGLE QUOTATION MARK are equivalent to `'`.
* U+00B4 ACUTE ACCENT is equivalent to `\`` (that is, it may open or close a `\`` type string).
* Any Unicode character in category "Open Punctuation" or "Close Punctuation", unless it is explicitly mentioned earlier in this document, is an error. (These may be given semantics in a later version.)

# Semantics

TODO

## Wait, TODO?

If you're really curious how to use this, I suggest examining the examples in [sample/](../sample) as well as the names of the functions defined in [src/globals.rs](../src/globals.rs).

# Verification

The directory `sample/test` of this directory contains a sort of "verification test" suite of sample files. Each file is annotated with an expected output in the comments, as parsed by the (pending) script `tools/regression.py`. Scripts which are expected to fail are sorted in subdirectories named "fail". If this file ever differs from a script in this directory, that is to be considered a bug.

Some of these files are commented with "tags":

* unicode: This script assumes a unicode-capable parser.
* ltt: This script accurately describes current behavior, but the behavior is specifically planned to change.
