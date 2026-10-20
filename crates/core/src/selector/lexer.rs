use crate::error::{Error, Result};

use super::{FOCUSED, SelectorStep, Term};

const CHAIN: &str = ">>";
const KEYS_HINT: &str = "role, name, name~, id, class, text, ocr, window, process and nth, plus the bare word focused";
const QUOTE_HINT: &str = "put text containing ':' in quotes, e.g. \"File name:\"";

const SELECTOR_KEYS: [(&str, TermBuilder); 10] = [
    ("role", |value| value.parse().map(Term::Role)),
    ("name", |value| Ok(Term::Name(value.to_string()))),
    ("name~", |value| Ok(Term::NameContains(value.to_string()))),
    ("id", |value| Ok(Term::Id(value.to_string()))),
    ("class", |value| Ok(Term::Class(value.to_string()))),
    ("text", |value| Ok(Term::Text(value.to_string()))),
    ("ocr", |value| Ok(Term::Ocr(value.to_string()))),
    ("window", |value| Ok(Term::Window(value.to_string()))),
    ("process", |value| Ok(Term::Process(value.to_string()))),
    ("nth", parse_nth),
];

type TermBuilder = fn(&str) -> Result<Term>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Piece {
    Chain,
    Bare(String),
    Term(Term),
}

pub(super) fn lex(text: &str) -> Result<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let (piece, after) = lex_piece(rest)?;
        pieces.push(piece);
        rest = after.trim_start();
    }
    Ok(pieces)
}

fn lex_piece(text: &str) -> Result<(Piece, &str)> {
    if let Some(rest) = text.strip_prefix(CHAIN) {
        return Ok((Piece::Chain, rest));
    }
    if let Some((key, rest)) = split_key(text) {
        let quoted = rest.starts_with('"');
        let (value, rest) = lex_value(rest)?;
        return Ok((Piece::Term(keyed_term(key, &value, quoted)?), rest));
    }
    let quoted = text.starts_with('"');
    let (value, rest) = lex_value(text)?;
    Ok((bare_piece(&value, quoted)?, rest))
}

fn split_key(text: &str) -> Option<(&str, &str)> {
    let letters = text.bytes().take_while(u8::is_ascii_alphabetic).count();
    let key_length = letters + usize::from(text[letters..].starts_with('~'));
    let value = text[key_length..].strip_prefix(':')?;
    (letters > 0).then(|| (&text[..key_length], value))
}

fn lex_value(text: &str) -> Result<(String, &str)> {
    if !text.starts_with('"') {
        let (value, rest) = lex_bare(text);
        return Ok((value.to_string(), rest));
    }
    let (value, rest) = lex_quoted(text)?;
    if !at_boundary(rest) {
        let (extra, _) = lex_bare(rest);
        return Err(Error::invalid(format!("unexpected '{extra}' right after a closing quote in selector")));
    }
    Ok((value, rest))
}

fn lex_bare(text: &str) -> (&str, &str) {
    let end = text
        .char_indices()
        .find(|&(index, ch)| ch.is_whitespace() || text[index..].starts_with(CHAIN))
        .map_or(text.len(), |(index, _)| index);
    text.split_at(end)
}

fn lex_quoted(text: &str) -> Result<(String, &str)> {
    let mut value = String::new();
    let mut chars = text.char_indices().skip(1);
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Ok((value, &text[index + 1..])),
            '\\' => push_escaped(&mut value, chars.next().map(|(_, next)| next)),
            _ => value.push(ch),
        }
    }
    Err(Error::invalid(format!("unterminated quote in selector at '{text}'")))
}

fn push_escaped(value: &mut String, next: Option<char>) {
    match next {
        Some(escaped @ ('"' | '\\')) => value.push(escaped),
        Some(other) => {
            value.push('\\');
            value.push(other);
        }
        None => value.push('\\'),
    }
}

fn at_boundary(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with(char::is_whitespace) || rest.starts_with(CHAIN)
}

fn bare_piece(value: &str, quoted: bool) -> Result<Piece> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::invalid("empty quoted value in selector"));
    }
    if !quoted && value.eq_ignore_ascii_case(FOCUSED) {
        return Ok(Piece::Term(Term::Focused));
    }
    Ok(Piece::Bare(value.to_string()))
}

fn keyed_term(key: &str, value: &str, quoted: bool) -> Result<Term> {
    let key = key.to_ascii_lowercase();
    let build = term_builder(&key).ok_or_else(|| unknown_key(&key))?;
    let value = if key == "id" { value } else { value.trim() };
    if value.trim().is_empty() {
        return Err(empty_value(&key, quoted));
    }
    build(value)
}

fn empty_value(key: &str, quoted: bool) -> Error {
    if quoted {
        return Error::invalid(format!("empty value for '{key}:' in selector"));
    }
    Error::invalid(format!("empty value for '{key}:' in selector; to match text, {QUOTE_HINT}"))
}

pub(super) fn term_builder(key: &str) -> Option<TermBuilder> {
    SELECTOR_KEYS.iter().find(|(name, _)| *name == key).map(|&(_, build)| build)
}

fn unknown_key(key: &str) -> Error {
    Error::invalid(format!("'{key}:' is not a selector key; {QUOTE_HINT}. Keys are {KEYS_HINT}"))
}

fn parse_nth(value: &str) -> Result<Term> {
    value
        .parse::<u32>()
        .ok()
        .filter(|&position| position >= 1)
        .map(Term::Nth)
        .ok_or_else(|| Error::invalid(format!("nth needs a whole number from 1, got '{value}'")))
}

pub(super) fn build_step(pieces: &[Piece], text: &str) -> Result<SelectorStep> {
    if pieces.is_empty() {
        return Err(Error::invalid(format!("empty step in selector '{text}'")));
    }
    let mut terms = Vec::new();
    let mut words: Vec<&str> = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Bare(word) => words.push(word),
            Piece::Term(term) => {
                flush_words(&mut words, &mut terms);
                terms.push(term.clone());
            }
            Piece::Chain => {}
        }
    }
    flush_words(&mut words, &mut terms);
    Ok(SelectorStep { terms })
}

fn flush_words(words: &mut Vec<&str>, terms: &mut Vec<Term>) {
    if !words.is_empty() {
        terms.push(Term::Text(words.join(" ")));
        words.clear();
    }
}
