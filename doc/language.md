This language is unnamed, and unstable.

# Parsing text

Input files are UTF-8. A beginning BOM will be discarded.

Statements are separated by newlines or commas. Except in the interior of a string (see below), if a line ends with `\`, all whitespace and comments following that `\` leading up to the next non-whitespace character are treated as a single space (in other words, a `\` may consume multiple blank lines). Placing anything after a `\` other than whitespace or comments are an error.

A statement is a series of symbols. A symbol is:

	* An identifier, as described below.
	* A number, as described below.
	* Any number of `'` marks followed by an identifier or number.
	* A string, as described below.
	* A paired `()`, `{}` or `[]` delimiter containing zero or more statements.

## Unicode oddness

Except in the interior of strings or comments, and except when U+FEFF is the opening BOM:

* U+00A0 NO-BREAK SPACE, U+1680 OGHAM SPACE MARK, U+200B ZERO WIDTH SPACE and U+FEFF ZERO WIDTH NON-BREAKING SPACE are illegal.
* U+201C LEFT DOUBLE QUOTATION MARK, U+201D RIGHT DOUBLE QUOTATION MARK, and U+201E DOUBLE-LOW-REVERSED-9 QUOTATION MARK are equivalent to `"` (that is, they may open or close a `"` type string).
* U+2018 LEFT SINGLE QUOTATION MARK and U+2019 RIGHT SINGLE QUOTATION MARK are equivalent to `'`.
* U+00B4 ACUTE ACCENT is equivalent to `\`` (that is, it may open or close a `\`` type string).
* Any Unicode character in category "Open Punctuation" or "Close Punctuation", unless it is explicitly mentioned earlier in this document, is an error. (These may be given semantics in a later version.)