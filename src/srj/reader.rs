//! Low-level bounded-memory JSON token reader for SRJ.

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{Result, SparqlResultsError};

const READ_CHUNK_BYTES: usize = 8 * 1024;

pub(super) struct JsonReader<R> {
    reader: R,
    buffer: Vec<u8>,
    position: usize,
    available: usize,
    pending: Option<Token>,
}

impl<R> JsonReader<R>
where
    R: AsyncRead + Unpin + Send,
{
    pub(super) fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: vec![0; READ_CHUNK_BYTES],
            position: 0,
            available: 0,
            pending: None,
        }
    }

    pub(super) fn unread_token(&mut self, token: Token) {
        self.pending = Some(token);
    }

    pub(super) async fn next_byte(&mut self) -> Result<Option<u8>> {
        if self.position == self.available {
            self.available = self
                .reader
                .read(&mut self.buffer)
                .await
                .map_err(|err| SparqlResultsError::Json(format!("SRJ read error: {err}")))?;
            self.position = 0;
            if self.available == 0 {
                return Ok(None);
            }
        }
        let byte = self.buffer[self.position];
        self.position += 1;
        Ok(Some(byte))
    }

    pub(super) async fn next_token(&mut self) -> Result<Token> {
        if let Some(token) = self.pending.take() {
            return Ok(token);
        }
        let byte = loop {
            match self.next_byte().await? {
                Some(byte) if !byte.is_ascii_whitespace() => break byte,
                Some(_) => {}
                None => return Ok(Token::Eof),
            }
        };
        match byte {
            b'{' => Ok(Token::ObjectStart),
            b'}' => Ok(Token::ObjectEnd),
            b'[' => Ok(Token::ArrayStart),
            b']' => Ok(Token::ArrayEnd),
            b':' => Ok(Token::Colon),
            b',' => Ok(Token::Comma),
            b'"' => self.read_string().await,
            b't' => self
                .expect_keyword(b"rue")
                .await
                .map(|()| Token::Bool(true)),
            b'f' => self
                .expect_keyword(b"alse")
                .await
                .map(|()| Token::Bool(false)),
            b'n' => self.expect_keyword(b"ull").await.map(|()| Token::Null),
            b'-' | b'0'..=b'9' => self.read_number(byte).await,
            other => Err(SparqlResultsError::Json(format!(
                "invalid JSON byte 0x{other:02x}"
            ))),
        }
    }

    async fn read_string(&mut self) -> Result<Token> {
        let mut raw = vec![b'"'];
        let mut escaped = false;
        loop {
            let byte = self
                .next_byte()
                .await?
                .ok_or_else(|| SparqlResultsError::Json("unterminated JSON string".to_owned()))?;
            raw.push(byte);
            if byte == b'"' && !escaped {
                let value = serde_json::from_slice::<String>(&raw)
                    .map_err(|err| SparqlResultsError::Json(err.to_string()))?;
                return Ok(Token::String(value));
            }
            escaped = byte == b'\\' && !escaped;
            if byte != b'\\' {
                escaped = false;
            }
        }
    }

    async fn expect_keyword(&mut self, suffix: &[u8]) -> Result<()> {
        for expected in suffix {
            if self.next_byte().await? != Some(*expected) {
                return Err(SparqlResultsError::Json("invalid JSON keyword".to_owned()));
            }
        }
        Ok(())
    }

    async fn read_number(&mut self, first: u8) -> Result<Token> {
        let mut number = vec![first];
        loop {
            let Some(byte) = self.next_byte().await? else {
                break;
            };
            if byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-') {
                number.push(byte);
            } else {
                self.position -= 1;
                break;
            }
        }
        let number =
            String::from_utf8(number).map_err(|err| SparqlResultsError::Json(err.to_string()))?;
        if number.parse::<serde_json::Number>().is_err() {
            return Err(SparqlResultsError::Json("invalid JSON number".to_owned()));
        }
        let _ = number;
        Ok(Token::Number)
    }
}

#[derive(Debug)]
pub(super) enum Token {
    Eof,
    ObjectStart,
    ObjectEnd,
    ArrayStart,
    ArrayEnd,
    Colon,
    Comma,
    String(String),
    Bool(bool),
    Null,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenKind {
    Eof,
    ObjectStart,
    ObjectEnd,
    ArrayStart,
    ArrayEnd,
    Colon,
    Comma,
    String,
    Bool,
    Null,
    Number,
}

impl Token {
    pub(super) fn kind(&self) -> TokenKind {
        match self {
            Self::Eof => TokenKind::Eof,
            Self::ObjectStart => TokenKind::ObjectStart,
            Self::ObjectEnd => TokenKind::ObjectEnd,
            Self::ArrayStart => TokenKind::ArrayStart,
            Self::ArrayEnd => TokenKind::ArrayEnd,
            Self::Colon => TokenKind::Colon,
            Self::Comma => TokenKind::Comma,
            Self::String(_) => TokenKind::String,
            Self::Bool(_) => TokenKind::Bool,
            Self::Null => TokenKind::Null,
            Self::Number => TokenKind::Number,
        }
    }
}
