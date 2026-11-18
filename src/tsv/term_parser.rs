//! Reading one RDF term in Turtle / N-Triples syntax (§4.2).

use super::TermSyntaxError;
use super::lexical::{
    BlankNodeLabel,
    NumericLexical,
    XSD_BOOLEAN,
    is_iri_char,
};
use crate::{
    BaseDirection,
    ResultValue,
};

type Parsed<T> = Result<T, TermSyntaxError>;

/// A cursor over one non-empty TSV field.
///
/// The parser checks syntax only; the reader then checks the term with
/// [`ResultValue::check`] (language tag well-formedness, literal
/// annotations, triple-term subject and predicate).
pub(super) struct TermParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> TermParser<'a> {
    /// Parse `field` as exactly one term.
    pub(super) fn parse_field(field: &'a str) -> Parsed<ResultValue> {
        let mut parser = Self {
            input: field,
            pos: 0,
        };
        let term = parser.term()?;
        if parser.pos == parser.input.len() {
            Ok(term)
        } else {
            Err(TermSyntaxError::TrailingInput)
        }
    }

    fn rest(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn next(&mut self) -> Parsed<char> {
        let c = self.peek().ok_or(TermSyntaxError::UnexpectedEnd)?;
        self.pos += c.len_utf8();
        Ok(c)
    }

    fn eat(&mut self, token: &str) -> bool {
        let found = self.rest().starts_with(token);
        if found {
            self.pos += token.len();
        }
        found
    }

    fn expect(&mut self, token: &str) -> Parsed<()> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(self.peek().map_or(
                TermSyntaxError::UnexpectedEnd,
                TermSyntaxError::UnexpectedChar,
            ))
        }
    }

    /// Consume characters while `accept` holds and return them.
    fn take_while(&mut self, accept: impl Fn(char) -> bool) -> &'a str {
        let start = self.pos;
        while let Some(c) = self.peek().filter(|c| accept(*c)) {
            self.pos += c.len_utf8();
        }
        &self.input[start..self.pos]
    }

    fn skip_spaces(&mut self) {
        self.take_while(|c| c == ' ');
    }

    fn term(&mut self) -> Parsed<ResultValue> {
        match self.peek().ok_or(TermSyntaxError::UnexpectedEnd)? {
            '<' if self.rest().starts_with("<<(") => self.triple(),
            '<' => self.iri().map(ResultValue::Uri),
            '_' => self.blank_node(),
            '"' | '\'' => self.literal(),
            '0'..='9' | '+' | '-' | '.' => self.number(),
            't' | 'f' => self.boolean(),
            other => Err(TermSyntaxError::UnexpectedStart(other)),
        }
    }

    /// `<<( s p o )>>`, the RDF 1.2 triple term.
    fn triple(&mut self) -> Parsed<ResultValue> {
        self.expect("<<(")?;
        self.skip_spaces();
        let subject = self.term()?;
        self.skip_spaces();
        let predicate = self.term()?;
        self.skip_spaces();
        let object = self.term()?;
        self.skip_spaces();
        self.expect(")>>")?;
        Ok(ResultValue::Triple {
            subject: Box::new(subject),
            predicate: Box::new(predicate),
            object: Box::new(object),
        })
    }

    /// `IRIREF`, decoding `UCHAR` escapes.
    fn iri(&mut self) -> Parsed<String> {
        self.expect("<")?;
        let mut iri = String::new(); // bounded: one term
        loop {
            match self.next()? {
                '>' => return Ok(iri),
                '\\' => iri.push(self.uchar()?),
                c if is_iri_char(c) => iri.push(c),
                c => return Err(TermSyntaxError::UnexpectedChar(c)),
            }
        }
    }

    /// `\uXXXX` or `\UXXXXXXXX`, after the backslash.
    fn uchar(&mut self) -> Parsed<char> {
        let width = match self.next()? {
            'u' => 4,
            'U' => 8,
            _ => return Err(TermSyntaxError::InvalidEscape),
        };
        let digits = self
            .rest()
            .get(..width)
            .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or(TermSyntaxError::InvalidEscape)?;
        self.pos += width;
        u32::from_str_radix(digits, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or(TermSyntaxError::InvalidEscape)
    }

    fn blank_node(&mut self) -> Parsed<ResultValue> {
        self.expect("_:")?;
        let label = self.take_while(|c| BlankNodeLabel::is_label_char(c) || c == '.');
        if BlankNodeLabel::new(label).is_well_formed() {
            Ok(ResultValue::BNode(label.to_owned()))
        } else {
            Err(TermSyntaxError::BlankNodeLabel)
        }
    }

    /// A quoted string with an optional `@lang[--dir]` or `^^<datatype>`.
    fn literal(&mut self) -> Parsed<ResultValue> {
        let value = self.string()?;
        let (mut lang, mut datatype, mut dir) = (None, None, None);
        if self.eat("@") {
            let (tag, direction) = self.language()?;
            lang = Some(tag);
            dir = direction;
        } else if self.eat("^^") {
            datatype = Some(self.iri()?);
        }
        Ok(ResultValue::Literal {
            value,
            lang,
            datatype,
            dir,
        })
    }

    /// `STRING_LITERAL_QUOTE` or `STRING_LITERAL_SINGLE_QUOTE`; §4.2 rules
    /// out the long (triple-quoted) forms.
    fn string(&mut self) -> Parsed<String> {
        let quote = self.next()?;
        let mut value = String::new(); // bounded: one term
        loop {
            match self.next()? {
                c if c == quote => return Ok(value),
                '\\' => value.push(self.echar()?),
                c @ ('\n' | '\r') => return Err(TermSyntaxError::UnexpectedChar(c)),
                c => value.push(c),
            }
        }
    }

    /// `ECHAR` or `UCHAR`, after the backslash.
    fn echar(&mut self) -> Parsed<char> {
        let escaped = match self.peek().ok_or(TermSyntaxError::InvalidEscape)? {
            'u' | 'U' => return self.uchar(),
            't' => '\t',
            'b' => '\u{8}',
            'n' => '\n',
            'r' => '\r',
            'f' => '\u{c}',
            c @ ('"' | '\'' | '\\') => c,
            _ => return Err(TermSyntaxError::InvalidEscape),
        };
        self.pos += 1;
        Ok(escaped)
    }

    /// Turtle 1.2 `LANG_DIR`: `[a-zA-Z]+ ('-' [a-zA-Z0-9]+)* ('--' dir)?`.
    fn language(&mut self) -> Parsed<(String, Option<BaseDirection>)> {
        let start = self.pos;
        if self.take_while(|c| c.is_ascii_alphabetic()).is_empty() {
            return Err(TermSyntaxError::LanguageTag);
        }
        while self.rest().starts_with('-') && !self.rest().starts_with("--") {
            self.pos += 1;
            if self.take_while(|c| c.is_ascii_alphanumeric()).is_empty() {
                return Err(TermSyntaxError::LanguageTag);
            }
        }
        let tag = self.input[start..self.pos].to_owned();
        if !self.eat("--") {
            return Ok((tag, None));
        }
        let direction = self
            .take_while(|c| c.is_ascii_alphabetic())
            .parse()
            .map_err(|_| TermSyntaxError::LanguageTag)?;
        Ok((tag, Some(direction)))
    }

    /// The abbreviated `INTEGER`, `DECIMAL` and `DOUBLE` forms.
    fn number(&mut self) -> Parsed<ResultValue> {
        let lexical =
            self.take_while(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'));
        let datatype = NumericLexical::new(lexical)
            .abbreviated_datatype()
            .ok_or_else(|| {
                TermSyntaxError::UnexpectedChar(lexical.chars().next().unwrap_or('?'))
            })?;
        Ok(Self::typed(lexical, datatype))
    }

    /// The abbreviated `true` / `false` forms.
    fn boolean(&mut self) -> Parsed<ResultValue> {
        let word = self.take_while(|c| c.is_ascii_alphabetic());
        match word {
            "true" | "false" => Ok(Self::typed(word, XSD_BOOLEAN)),
            _ => Err(TermSyntaxError::UnexpectedStart(
                word.chars().next().unwrap_or('?'),
            )),
        }
    }

    fn typed(lexical: &str, datatype: &str) -> ResultValue {
        ResultValue::Literal {
            value: lexical.to_owned(),
            lang: None,
            datatype: Some(datatype.to_owned()),
            dir: None,
        }
    }
}
