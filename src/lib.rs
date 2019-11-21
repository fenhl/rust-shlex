// Copyright 2015 Nicholas Allegra (comex).
// Licensed under the Apache License, Version 2.0 <http://www.apache.org/licenses/LICENSE-2.0> or
// the MIT license <http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Same idea as (but implementation not directly based on) the Python shlex module.  However, this
//! implementation does not support any of the Python module's customization because it makes
//! parsing slower and is fairly useless.  You only get the default settings of shlex.split, which
//! mimic the POSIX shell:
//! http://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html
//!
//! This implementation also deviates from the Python version in not treating \r specially, which I
//! believe is more compliant.
//!
//! The algorithms in this crate are oblivious to UTF-8 high bytes, so they iterate over the bytes
//! directly as a micro-optimization.

use std::borrow::Cow;

/// An error that can occur when splitting a string.
///
/// An input string is erroneous if it ends while inside a quotation or right after an unescaped backslash.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Error {
    /// The input string ends with an unescaped backslash.
    EndOfStringBackslash,
    /// The input string has an unmatched `"`.
    UnclosedDoubleQuote,
    /// The input string has an unmatched `'`.
    UnclosedSingleQuote
}

/// An iterator that takes an input string and splits it into the words using the same syntax as
/// the POSIX shell.
pub struct Shlex<I: Iterator<Item = u8>> {
    in_iter: I,
    /// The number of newlines read so far, plus one.
    pub line_no: usize
}

impl<'a> Shlex<std::str::Bytes<'a>> {
    pub fn new(in_str: &'a str) -> Self {
        Shlex {
            in_iter: in_str.bytes(),
            line_no: 1
        }
    }
}

impl<I: Iterator<Item = u8>> Shlex<I> {
    fn parse_word(&mut self, mut ch: u8) -> Result<String, Error> {
        let mut result: Vec<u8> = Vec::new();
        loop {
            match ch as char {
                '"' => { self.parse_double(&mut result)?; }
                '\'' => { self.parse_single(&mut result)?; }
                '\\' => if let Some(ch2) = self.next_char() {
                    if ch2 != '\n' as u8 { result.push(ch2); }
                } else {
                    return Err(Error::EndOfStringBackslash);
                },
                ' ' | '\t' | '\n' => { break; },
                _ => { result.push(ch as u8); },
            }
            if let Some(ch2) = self.next_char() { ch = ch2; } else { break; }
        }
        Ok(unsafe { String::from_utf8_unchecked(result) })
    }

    fn parse_double(&mut self, result: &mut Vec<u8>) -> Result<(), Error> {
        loop {
            if let Some(ch2) = self.next_char() {
                match ch2 as char {
                    '\\' => {
                        if let Some(ch3) = self.next_char() {
                            match ch3 as char {
                                // \$ => $
                                '$' | '`' | '"' | '\\' => { result.push(ch3); },
                                // \<newline> => nothing
                                '\n' => {},
                                // \x => =x
                                _ => { result.push('\\' as u8); result.push(ch3); }
                            }
                        } else {
                            return Err(Error::EndOfStringBackslash);
                        }
                    },
                    '"' => { return Ok(()); },
                    _ => { result.push(ch2); },
                }
            } else {
                return Err(Error::UnclosedDoubleQuote);
            }
        }
    }

    fn parse_single(&mut self, result: &mut Vec<u8>) -> Result<(), Error> {
        loop {
            if let Some(ch2) = self.next_char() {
                match ch2 as char {
                    '\\' => {
                        if let Some(ch3) = self.next_char() {
                            match ch3 as char {
                                // for single quotes, only these can be escaped
                                '\'' | '\\' => { result.push(ch3); },
                                _ => { result.push('\\' as u8); result.push(ch3); }
                            }
                        } else {
                            return Err(Error::EndOfStringBackslash);
                        }
                    },
                    '\'' => { return Ok(()); },
                    _ => { result.push(ch2); },
                }
            } else {
                return Err(Error::UnclosedSingleQuote);
            }
        }
    }

    fn next_char(&mut self) -> Option<u8> {
        let res = self.in_iter.next();
        if res == Some('\n' as u8) { self.line_no += 1; }
        res
    }
}

impl<I: Iterator<Item = u8>, T: IntoIterator<IntoIter = I, Item = u8>> From<T> for Shlex<I> {
    fn from(into_iter: T) -> Self {
        Shlex {
            in_iter: into_iter.into_iter(),
            line_no: 1
        }
    }
}

impl<I: Iterator<Item = u8>> Iterator for Shlex<I> {
    type Item = Result<String, Error>;

    fn next(&mut self) -> Option<Result<String, Error>> {
        if let Some(mut ch) = self.next_char() {
            // skip initial whitespace
            loop {
                match ch as char {
                    ' ' | '\t' | '\n' => {},
                    '#' => {
                        while let Some(ch2) = self.next_char() {
                            if ch2 as char == '\n' { break; }
                        }
                    },
                    _ => { break; }
                }
                if let Some(ch2) = self.next_char() { ch = ch2; } else { return None; }
            }
            Some(self.parse_word(ch))
        } else { // no initial character
            None
        }
    }

}

/// Convenience function that consumes the whole string at once.
pub fn split(in_str: &str) -> Result<Vec<String>, Error> {
    let mut shl = Shlex::new(in_str);
    shl.by_ref().collect()
}

/// Given a single word, return a string suitable to encode it as a shell argument.
pub fn quote(in_str: &str) -> Cow<str> {
    if in_str.len() == 0 {
        "\"\"".into()
    } else if in_str.bytes().any(|c| match c as char {
        '|' | '&' | ';' | '<' | '>' | '(' | ')' | '$' | '`' | '\\' | '"' | '\'' | ' ' | '\t' |
        '\r' | '\n' | '*' | '?' | '[' | '#' | '~' | '=' | '%' => true,
        _ => false
    }) {
        let mut out: Vec<u8> = Vec::new();
        out.push('"' as u8);
        for c in in_str.bytes() {
            match c as char {
                '$' | '`' | '"' | '\\' => out.push('\\' as u8),
                _ => ()
            }
            out.push(c);
        }
        out.push('"' as u8);
        unsafe { String::from_utf8_unchecked(out) }.into()
    } else {
        in_str.into()
    }
}

/// Convenience function that consumes an iterable of words and turns it into a single string,
/// quoting words when necessary. Consecutive words will be separated by a single space.
pub fn join<'a, I: IntoIterator<Item = &'a str>>(words: I) -> String {
    words.into_iter()
        .map(quote)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
static SPLIT_TEST_ITEMS: &'static [(&'static str, Result<&'static [&'static str], Error>)] = &[
    ("foo$baz", Ok(&["foo$baz"])),
    ("foo baz", Ok(&["foo", "baz"])),
    ("foo\"bar\"baz", Ok(&["foobarbaz"])),
    ("foo \"bar\"baz", Ok(&["foo", "barbaz"])),
    ("   foo \nbar", Ok(&["foo", "bar"])),
    ("foo\\\nbar", Ok(&["foobar"])),
    ("\"foo\\\nbar\"", Ok(&["foobar"])),
    ("'baz\\$b'", Ok(&["baz\\$b"])),
    ("'baz\\\''", Ok(&["baz\'"])),
    ("\\", Err(Error::EndOfStringBackslash)),
    ("\"\\", Err(Error::EndOfStringBackslash)),
    ("'\\", Err(Error::EndOfStringBackslash)),
    ("\"", Err(Error::UnclosedDoubleQuote)),
    ("'", Err(Error::UnclosedSingleQuote)),
    ("foo #bar\nbaz", Ok(&["foo", "baz"])),
    ("foo #bar", Ok(&["foo"])),
    ("foo#bar", Ok(&["foo#bar"])),
    ("foo\"#bar", Err(Error::UnclosedDoubleQuote))
];

#[test]
fn test_split() {
    for &(input, output) in SPLIT_TEST_ITEMS {
        assert_eq!(split(input), output.map(|o| o.iter().map(|&x| x.to_owned()).collect()));
    }
}

#[test]
fn test_lineno() -> Result<(), Error> {
    let mut sh = Shlex::new("\nfoo\nbar");
    while let Some(word) = sh.next() {
        if word? == "bar" {
            assert_eq!(sh.line_no, 3);
        }
    }
    Ok(())
}

#[test]
fn test_quote() {
    assert_eq!(quote("foobar"), "foobar");
    assert_eq!(quote("foo bar"), "\"foo bar\"");
    assert_eq!(quote("\""), "\"\\\"\"");
    assert_eq!(quote(""), "\"\"");
}

#[test]
fn test_join() {
    assert_eq!(join(vec![]), "");
    assert_eq!(join(vec![""]), "\"\"");
    assert_eq!(join(vec!["a", "b"]), "a b");
    assert_eq!(join(vec!["foo bar", "baz"]), "\"foo bar\" baz");
}
