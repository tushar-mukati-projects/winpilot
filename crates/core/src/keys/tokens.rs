use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Token<'a> {
    Word(&'a str),
    Plus,
    Comma,
    Space,
}

const LITERAL_PLUS: &str = "+";

pub(super) fn tokenize(spec: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut word_start: Option<usize> = None;
    for (index, ch) in spec.char_indices() {
        let Some(token) = punctuation(ch) else {
            word_start.get_or_insert(index);
            continue;
        };
        if let Some(start) = word_start.take() {
            tokens.push(Token::Word(&spec[start..index]));
        }
        tokens.push(token);
    }
    if let Some(start) = word_start {
        tokens.push(Token::Word(&spec[start..]));
    }
    tokens
}

fn punctuation(ch: char) -> Option<Token<'static>> {
    match ch {
        '+' => Some(Token::Plus),
        ',' => Some(Token::Comma),
        _ if ch.is_whitespace() => Some(Token::Space),
        _ => None,
    }
}

pub(super) fn group_chords<'a>(tokens: &[Token<'a>], spec: &str) -> Result<Vec<Vec<&'a str>>> {
    let mut chords = Vec::new();
    let mut position = skip_separators(tokens, 0);
    while position < tokens.len() {
        let (parts, next) = read_chord(tokens, position, spec)?;
        chords.push(parts);
        position = skip_separators(tokens, next);
    }
    Ok(chords)
}

fn skip_while(tokens: &[Token<'_>], from: usize, skip: impl Fn(Token<'_>) -> bool) -> usize {
    tokens.iter().skip(from).position(|token| !skip(*token)).map_or(tokens.len(), |offset| from + offset)
}

fn skip_separators(tokens: &[Token<'_>], from: usize) -> usize {
    skip_while(tokens, from, |token| matches!(token, Token::Space | Token::Comma))
}

fn skip_spaces(tokens: &[Token<'_>], from: usize) -> usize {
    skip_while(tokens, from, |token| token == Token::Space)
}

fn read_chord<'a>(tokens: &[Token<'a>], start: usize, spec: &str) -> Result<(Vec<&'a str>, usize)> {
    let mut parts = Vec::new();
    let mut position = start;
    loop {
        let (part, next) = read_part(tokens, position, spec)?;
        parts.push(part);
        let joiner = skip_spaces(tokens, next);
        if tokens.get(joiner) != Some(&Token::Plus) {
            return Ok((parts, next));
        }
        position = skip_spaces(tokens, joiner + 1);
    }
}

fn read_part<'a>(tokens: &[Token<'a>], position: usize, spec: &str) -> Result<(&'a str, usize)> {
    match (tokens.get(position), tokens.get(position + 1)) {
        (Some(&Token::Word(word)), _) => Ok((word, position + 1)),
        (Some(Token::Plus), Some(Token::Word(_))) => Err(Error::invalid(format!("empty key before '+' in '{spec}'"))),
        (Some(Token::Plus), _) => Ok((LITERAL_PLUS, position + 1)),
        (Some(Token::Comma), _) => {
            Err(Error::invalid(format!("empty key after '+' in '{spec}'; write a comma key as 'comma'")))
        }
        _ => Err(Error::invalid(format!("empty key after '+' in '{spec}'"))),
    }
}
