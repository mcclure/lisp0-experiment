This language is unnamed, and unstable. This document describes only this specific edition/variant of the language (see [README.md](../README.md) or invoke the interpreter with `-v`).

The general idea is this is a LISP variant with "no special forms", and an optional "low-parentheses" sugar syntax. If you have used other programming languages, you may be able to get a sense of the language just from reading [an example](../sample/fizzbuzz.l0).

# Invocation

The `lisp0` command line executable has a `--help`, but in general:

- Invoke with either the name of a file to execute, or `-e ''` with code in the quotes.
- Arguments to the program should be given after `--`.
- The language comes in two forms, a "pure LISP" form and a sugared form. If the invoked file has a `.ls0` file extension, or the `--lisp` argument is given at the command line, the "pure LISP" form will be enforced. Otherwise sugar is applied (the recommended file extension for this is `.l0`).
- You can block executed programs from interacting with the filesystem by passing `--disable-fs`.
- Currently, there are bugs in the garbage collector which can lead to crashes. A crash can be significantly delayed by increasing the size of the default memory, thus making GCs less frequent. To do this, initialize with `--debug-mem-initial=16777216` (or some other large number). Yes, this is bad.
- Invoking with `-v` will print versions for the interpreter and language, then quit.

Currently the language is not configured for embedding.

# Basics

Read either "Basics— LISP" or "Basics— L0" below.

## Basics— LISP

Programs in this language are a series of statements. A statement is a parenthesis-enclosed, whitespace-separated "word list". A word is:

- An identifier: Any sequence of characters not including `'"#(){}[],`. This is interpreted as the name of a "variable".
- A number (an integer)
- A string, which is either:
    - Non-newline text between two `"` characters, with special behavior when a `\\` appears:
    	- `\\\\` will be taken as a backslash.
    	- `\\n` will be taken as a newline (LF).
    	- `\\t` will be taken as a tab.
    	- `\\"` will be taken as a quotation marks.
    	- A `\\` at the end of a line (IE, whitespace followed by a newline) will be silently deleted
    - Non-newline text between two `\`` characters. (And no special meaning for `\\`.)
    - A `'` followed by a number of non-whitespace characters (as long as the first character is not a number¹.
- A function call: This is a parenthesis-enclosed word list.
- A quoted list: This is a `'` followed by a parenthesis-enclosed word list.

A statement with no parenthesis around it will be treated as a "return value". This should come only as the final line of a block.

Words are separated by any whitespace. A `\\` may be placed at the end of a line, and will be silently discarded.

Placing a `#` anywhere will be treated as a "comment"; everything from the `#` to the next newline will be ignored. If a line within a comment ends with '\\', the comment will be extended until the next non-whitespace character. In regular code, a comment after after a `\\`is allowed.

In advanced use: Function calls can be nested, quoted word lists can contain nested word lists, and quoted word lists can contain other quoted lists. The intent of a quoted word list is to "interpret code as data", but could also be used as a shorthand way of declaring data for your own use. When interpreting a quoted list as data, identifiers become strings, strings become "quoted" strings, word lists become regular lists, and quoted word lists remain quoted word lists.

Below, a quoted list itself containing word lists will be called a "block", and used to make functions; `'((set 'x y) (print x) x)` is equivalent to `{set 'x y, print x, x}` below. (Note the final "x" is *not* wrapped in parenthesis; this is the "return value".) Also below a `[]` sugar syntax will be used to define array literals; in LISP this is done by calling `make-array`. `[w, x y, z]` as written below would be `(make-array w (x y) z)` in the LISP syntax.

¹ Entering a \' followed by an integer will do something surprising, so don't do that.

## Basics— L0

Programs in this language are a series of statements; statements are lists of words. Statements are separated from each other by newlines or commas, and words within a statement are separated from each other by any whitespace. A word is:

- An identifier: Any sequence of characters not including `'"#(){}[],`. This is interpreted as the name of a "variable".
- A number (an integer)
- A string, which is either:
    - Non-newline text between two `"` characters, with special behavior when a `\\` appears:
    	- `\\\\` will be taken as a backslash.
    	- `\\n` will be taken as a newline (LF).
    	- `\\t` will be taken as a tab.
    	- `\\"` will be taken as a quotation marks.
    	- A `\\` at the end of a line (IE, whitespace followed by a newline) will be silently deleted
    - Non-newline text between two `\`` characters. (And no special meaning for `\\`.)
    - A `'` followed by a number of non-whitespace characters (as long as the first character is not a number².
- A function call: This is a parenthesis-enclosed word list.
- A block: This is `{}` curly braces containing one or more statements.
- An array: This is `[]` square brackets containing one or more statements.
- A quoted list: This is a `'` followed by a parenthesis-enclosed word list.

A statement with only one word in it will be treated as a "return value". The "lone" value in this statement will not be executed as a function, but directly evaluates to its value. "Lone" values should only be used in arrays, or as the final line of a block (as otherwise they would be noops¹⁴).

A statement may cross multiple lines by placing a `\\` at the end of a line; all whitespace after the `\\` will be treated as a single space. Newlines inside a function call or quoted list (as opposed to inside a block or array) will be treated as an error.

Placing a `#` anywhere will be treated as a "comment"; everything from the `#` to the next newline will be ignored. If a line within a comment ends with '\\', the comment will be extended until the next non-whitespace character. In regular code, a comment after after a `\\`is allowed.

Function calls, blocks, arrays, and quoted word lists can be nested within each other freely.

Blocks are used to create functions. When a {} is executed as code, any line containing two or more words will be treated as a function call, but a single word by itself will be treated as a "return value" and its value will be returned without being executed³. Likewise, for a `[]` in code, statements with two or more words will be treated as function calls and their value will be stored in the array, whereas single-word statements will be treated as values and that lone value will be stored in the array. In either of these cases, if you want to execute a block/function with no arguments, use `do`.

In advanced use: When interpreting a block or quoted list as data, identifiers become strings, strings become "quoted" strings, word lists become regular lists, and quoted word lists remain quoted word lists. Additionally, the block itself becomes a quoted list; for each statement in the block, the statement becomes (if the statement contains two or more words) a list containing its words or (if the statement contains one word) becomes just that lone word.

² Entering a \' followed by an integer will do something surprising, so don't do that.
³ Note this means that a lone word by itself in the middle of a block is nonsensical; when executing code, this will be treated as an error.
¹⁴ If you *want* a noop statement use `do discard`.

### Why "L0"?

I, the author of this language, don't much like LISP. The parenthesis everywhere bug me. L0 is a "sugar syntax" which tries to cut down on the parenthesis by using newlines as a shorthand for `) (`, curly braces as a shorthand for `'(( … ))`, and square braces as a shorthand for `(make-array)`.

# Writing code

Statements are treated as "function calls", where the first word is the item to be executed and the remaining words are "arguments". The two things that can be executed are blocks and functions¹². Functions are "upgraded" blocks which may have local variables and take arguments. (Arguments can be passed to a block, but the arguments will be discarded.) Before executing a statement, the arguments will first be evaluated. So for example:

    print (+ 3 2) (- 1 2) ln

When executing this statement, `+ 3 2` and `- 1 2` (that second one calculates `1 - 2`, if you've never used a LISP before) will be executed *before* `print` is invoked⁴.

Full standard library documentation is below, but the most important basic builtin symbols are `print`, `sp`, `ln`, `set`, `fn`, `if`, `true` and `nil`. `print` will write⁶ each of its arguments to STDOUT as text; `sp` and `ln` are strings containing a space and a newline.

`set` is used to assign a variable. For its first argument it takes the variable name to set *as a string*. For example:

	set 'x 3
	print x

This will print "3" to STDOUT. Note writing `set x 3` would be accepted but probably not do what you want⁷.

`true` and `nil` are used in boolean expressions (`nil` is used for "false"). `if` takes a condition and two blocks, and executes either the first block or the second depending on whether the condition executes to `nil` or non-`nil`:

    if (> x 3) {
    	print "x is high" ln
	} {
		print "x is low" ln
	}

For the comfort of anyone used to other programming languages, the identifier `else` may be placed between the two blocks, and will be silently ignored:

	if (> x 3) {
    	print "x is high" ln
	} {
		print "x is low" ln
	}

⁴ And consistently from left to right, if it matters.
⁶ No particular guarantees are made about flushing behavior when printing, but you can force a flush by calling `do flush`.
⁷ `set` is an ordinary function and will assign to whatever string it receives as its first argument. So for example if you run:

	set 'x 'y
	set x 'z
	print y

This will print the string "z", because the second line sets the variable whose name is stored in the variable x. If you find this confusing, then simply remember that the first argument to `set` should always begin with a `'`.
¹² And builtins, but those act like functions in every way except they aren't accepted by `fn-unpack`.

## Functions

As shown above, "blocks" are created with `{ curly braces }`. They are similar to functions from other languages, in that they can be executed and can have return values. But they don't have scopes, and they can't take arguments.

We can get these niceties by "upgrading" the block to a function with `fn`. `fn` takes up to four arguments; in its basic form it looks like

	fn 'multiply-10-add {arg1 0, arg2 5} {local1 10} {
		set 'local1 (* local1 arg1)
		set 'local1 (+ local1 arg2)
		local1
	}

Some things to note here. The `fn` form creates an anonymous function, so this code wouldn't do anything unless the fn was captured and passed to something else, such as `set`; the name given is only visible in `multiply-10-add`'s scope, and is there so the function can call itself recursively. The args and locals¹³ each have values after them; these are initial values, and are optional. The arg initial value is a default inserted if that argument is not provided. Excess arguments are silently ignored.

`fn` comes in many forms, which you can find documented in the library reference below; primarily. Notably there are two variants of `fn`, named `set-fn` and `do`. `do` takes the same arguments as `fn` and effectively constructs a function and executes it immediately. `set-fn` calls `set` but also forwards the name argument to the fn. So a more useful version of the anonymous fn above could have been

	set-fn 'multiply-10-add {arg1 0, arg2 5} {local1 10} {
		set 'local1 (* local1 arg1)
		set 'local1 (+ local1 arg2)
		local1
	})

	print (multiply-10-add 5 10) # Prints 60

Functions and blocks are both tail-recursive; a safe way to make an infinite loop would be

	set 'x 0
	do 'recurse {
		print x ln
		recurse (+ x 1)
	}

**A warning:** Scopes currently do not work the way you would expect from other languages. There is exactly one global, dynamic scope; the "scopes" created by functions only shuffle new values into the scope, and remove them at the end. (This is not an intentional design decision of the language, but is due to the simplicity of the current interpreter.) If this behavior is not planned for, it can lead to surprising and unwanted behavior:

	set-fn 'fun1 {f} {x 2} {
		do f
	}

	set-fn 'fun2 {v} {x 3} {
		print "x: " x ln     # prints 3
		fun1 {
			print "x: " x ln # prints 2 ??
		}
		print "x: " x ln     # prints 3
	}

	fun2 "idk"

The local x has "leaked" . Blocks are not closures. If closure-like behavior is needed, the best way to do this is to "manually capture" a variable using a local:

	set-fn 'fun1 {f} {x 2} {
		do f
	}

	set-fn 'fun2 {v} {x 3} {
		print "x: " x ln     # prints 3
		fun1 (fn {x x} {     # The parent "x" is embedded in the inner function.
			print "x: " x ln # prints 3
		})
		print "x: " x ln     # prints 3
	}

	fun2 "idk"

¹³ Note the args and locals of the fn here use the `{}` syntax, but these `{}`s aren't blocks; the `fn` builtin interprets the contents as lists of symbols rather than executing them. Warning, although the value on the right is executed like code, not just any code can go here; currently it must be an integer, a string, a quoted list, or a variable name.

## Data

Data in this language can be a string, an integer, an array⁵, a dictionary, a quote, a function, or the two special values `true` and `nil`⁸.

⁵ Notice I say "array". Users of other LISPs may expect "lists" to be linked lists in a car-cdr structure. This LISP doesn't have those, and when I use "list" in this document, I mean it in the generic sense of a *sequence*.

⁸ Note the `print` function only really expects strings and integers; any other kind of data will result in a square-bracketed debug print such as `[nil]` or `[array]`.

### Arrays

Arrays are created with `[]` (or manually, with `make-array`) You set their values using extended arguments for `set`, and can read them back using the function `get`:

	set 'a [4]
	print (get a 0) ln # Prints 4
	set a 0 5          # Note no '
	print (get a 0) ln # Prints 5

Arrays are 0-indexed; they can be extended using `push`, truncated using `trunc`, and their length can be queried using `len`. Using `get` with an invalid numeric index returns `nil`; if you need to distinguish between an invalid index and a valid index containing `nil`, use `len` or the helper `has`.

	set 'print-state {
		print (get a 0) ", " (get a 1) "; " (len a) ", " (has a 1) ln
	}
    set 'a [4]
    push 'a 5
    do print-state
    trunc 'a 1 # Truncate to length 1
    do print-state

The first `print-state` will print "4, 5; 2, [true]" and the second will print "4, [nil]; 1, [nil]".

### Dicts

Dicts are created with `make-dict`, which takes an even number of arguments and assigns the even arguments to keys and the odd arguments to 0. Similar to arrays, you can fetch and set keys with `get` and `set`. Also similar to arrays, fetching an absent key will return `nil`, but `has` can be used to test if a key is present. `del` can be used to remove a key from an array, and `len` will return its number of items. Keys to dictionary can be any of a string, an integer, or `true` or `nil`⁹.

	set 'print-state {
		print (get a 'x) ", " (get a 'y) ", " (get a 'z) "; " \
		(len a) ", " (has a 'y) ln
	}
    set 'a (make-dict \
    	'x 5
    	'y 6
    )
    do print-state
    set 'a 'z 7
    del 'a 'y
    do print-state

The first `print-state` will print "4, 5, [nil]; 2, [true]" and the second will print "4, [nil], 6; 2, [true]".

⁹ As I'm writing this document, it occurs to me that using builtin functions, such as `set` and `if`, as keys to a dictionary *probably* works. Let's say that's "undefined behavior".

### Advanced use: Quotes

"Quotes" have been glossed by a few times now in this document because they are largely an implementation detail of the language. However quotes *are* exposed to the user because of homoiconicity¹⁰, and they're potentially useful.

The basic use of quotes would be to leverage the fact that quoted lists become lists of words. For example, you could say `(make-array "x", "y", "z", "w")`, but `'(x y z w)` is exactly equivalent to that and shorter. This is because when executing a statement, at the same time that identifiers are looked up, any `'`s in the code¹¹ will be stripped away.

However, you can also manipulate quotes at runtime as data objects; in this sense a quote is like a "box" that any value can be placed into or taken out of. You can make a runtime quote with `make-quote` and extract its value with `unquote`. Moreover if the first argument to `set` is a quote, the second argument will replace the single value within the quote. This "box"-like functionality means quotes can be used like OCaml `ref`.

    # If you do not understand this code, that is a sign
    # That you probably do not need it.

    set 'a (make-quote 0)
    set 'incr (fn {self a} { set a (+ (unquote a) 1) })
    set 'fetch (fn {self a} { print (unquote a) })
    set 'a nil

    # This is object-oriented programming. No, really
    do fetch # Print "0"
    do incr
    do fetch # Print "1"
    do incr
    do incr
    do fetch # Print "3"

As I'm writing this document, I found a bug which will prevent the above code from working. See comment on "fun" in globals.rs.

¹⁰ The lambda, of course, is a homosexual icon.
¹¹ Well, the outermost layer anyway. You can quote a quote.

# Library reference

TODO, until then read [globals.rs](../src/globals.rs)