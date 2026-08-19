//! SRJ document and value parsing.

use std::future::Future;
use std::pin::Pin;

use indexmap::IndexMap;

use super::parse::{SrjStreamKind, SrjStreamSink, SrjStreamSummary};
use super::reader::{JsonReader, Token, TokenKind};
use crate::{Result, ResultRow, ResultValue, SparqlResultsError};

impl<R> JsonReader<R>
where
    R: tokio::io::AsyncRead + Unpin + Send,
{
    pub(super) async fn parse_document<S>(&mut self, sink: &mut S) -> Result<SrjStreamSummary>
    where
        S: SrjStreamSink + Send,
    {
        self.expect(TokenKind::ObjectStart).await?;
        let mut head = None;
        let mut summary = None;
        loop {
            if self.next_is(TokenKind::ObjectEnd).await? {
                break;
            }
            let key = self.expect_string().await?;
            self.expect(TokenKind::Colon).await?;
            match key.as_str() {
                "head" => {
                    head = Some(self.parse_head().await?);
                }
                "results" => {
                    let vars = head.take().ok_or_else(|| {
                        SparqlResultsError::InvalidDocument(
                            "SRJ results appeared before head".to_owned(),
                        )
                    })?;
                    sink.select_header(vars).await?;
                    let row_count = self.parse_results(sink).await?;
                    summary = Some(SrjStreamSummary {
                        kind: SrjStreamKind::Select,
                        row_count,
                    });
                }
                "boolean" => {
                    let result = self.expect_bool().await?;
                    sink.ask(result).await?;
                    summary = Some(SrjStreamSummary {
                        kind: SrjStreamKind::Ask,
                        row_count: 1,
                    });
                }
                _ => self.skip_value().await?,
            }
            if !self.consume_comma_or_end().await? {
                break;
            }
        }
        self.ensure_eof().await?;
        summary
            .ok_or_else(|| SparqlResultsError::MissingElement("SRJ results or boolean".to_owned()))
    }

    async fn parse_head(&mut self) -> Result<Vec<String>> {
        self.expect(TokenKind::ObjectStart).await?;
        let mut vars = Vec::new();
        loop {
            if self.next_is(TokenKind::ObjectEnd).await? {
                return Ok(vars);
            }
            let key = self.expect_string().await?;
            self.expect(TokenKind::Colon).await?;
            if key == "vars" {
                vars = self.parse_vars_array().await?;
            } else {
                self.skip_value().await?;
            }
            if !self.consume_comma_or_end().await? {
                return Ok(vars);
            }
        }
    }

    async fn parse_results<S>(&mut self, sink: &mut S) -> Result<u64>
    where
        S: SrjStreamSink + Send,
    {
        self.expect(TokenKind::ObjectStart).await?;
        let mut row_count = None;
        loop {
            if self.next_is(TokenKind::ObjectEnd).await? {
                break;
            }
            let key = self.expect_string().await?;
            self.expect(TokenKind::Colon).await?;
            if key == "bindings" {
                row_count = Some(self.parse_bindings_array(sink).await?);
            } else {
                self.skip_value().await?;
            }
            if !self.consume_comma_or_end().await? {
                break;
            }
        }
        row_count.ok_or_else(|| SparqlResultsError::MissingElement("SRJ bindings".to_owned()))
    }

    async fn parse_vars_array(&mut self) -> Result<Vec<String>> {
        self.expect(TokenKind::ArrayStart).await?;
        let mut vars = Vec::new();
        while !self.next_is(TokenKind::ArrayEnd).await? {
            vars.push(self.expect_string().await?);
            if !self.consume_comma_or_end().await? {
                break;
            }
        }
        Ok(vars)
    }

    async fn parse_bindings_array<S>(&mut self, sink: &mut S) -> Result<u64>
    where
        S: SrjStreamSink + Send,
    {
        self.expect(TokenKind::ArrayStart).await?;
        let mut count = 0_u64;
        while !self.next_is(TokenKind::ArrayEnd).await? {
            sink.select_row(self.parse_binding().await?).await?;
            count = count.checked_add(1).ok_or_else(|| {
                SparqlResultsError::InvalidDocument("SRJ row count overflow".to_owned())
            })?;
            if !self.consume_comma_or_end().await? {
                break;
            }
        }
        Ok(count)
    }

    async fn parse_binding(&mut self) -> Result<ResultRow> {
        self.expect(TokenKind::ObjectStart).await?;
        let mut bindings = IndexMap::new();
        loop {
            if self.next_is(TokenKind::ObjectEnd).await? {
                return Ok(ResultRow { bindings });
            }
            let variable = self.expect_string().await?;
            self.expect(TokenKind::Colon).await?;
            bindings.insert(variable, self.parse_value().await?);
            if !self.consume_comma_or_end().await? {
                return Ok(ResultRow { bindings });
            }
        }
    }

    fn parse_value(&mut self) -> Pin<Box<dyn Future<Output = Result<ResultValue>> + Send + '_>> {
        Box::pin(async move {
            self.expect(TokenKind::ObjectStart).await?;
            let mut value = None;
            let mut datatype = None;
            let mut lang = None;
            let mut term_type = None;
            loop {
                if self.next_is(TokenKind::ObjectEnd).await? {
                    break;
                }
                let key = self.expect_string().await?;
                self.expect(TokenKind::Colon).await?;
                match key.as_str() {
                    "type" => term_type = Some(self.expect_string().await?),
                    "value" => value = Some(self.parse_value_field().await?),
                    "xml:lang" | "lang" => lang = Some(self.expect_string().await?),
                    "datatype" => datatype = Some(self.expect_string().await?),
                    _ => self.skip_value().await?,
                }
                if !self.consume_comma_or_end().await? {
                    break;
                }
            }
            let term_type = term_type
                .ok_or_else(|| SparqlResultsError::MissingElement("SRJ term type".to_owned()))?;
            match term_type.as_str() {
                "uri" => Ok(ResultValue::Uri(expect_string_value(value)?)),
                "bnode" => Ok(ResultValue::BNode(expect_string_value(value)?)),
                "literal" => Ok(ResultValue::Literal {
                    value: expect_string_value(value)?,
                    lang,
                    datatype,
                }),
                "triple" => match value {
                    Some(JsonValueField::Triple {
                        subject,
                        predicate,
                        object,
                    }) => Ok(ResultValue::Triple {
                        subject: Box::new(subject),
                        predicate: Box::new(predicate),
                        object: Box::new(object),
                    }),
                    Some(JsonValueField::String(_)) | None => {
                        Err(SparqlResultsError::InvalidDocument(
                            "SRJ triple value must be an object".to_owned(),
                        ))
                    }
                },
                other => Err(SparqlResultsError::UnexpectedElement(format!(
                    "SRJ term type {other}"
                ))),
            }
        })
    }

    async fn parse_value_field(&mut self) -> Result<JsonValueField> {
        match self.next_token().await? {
            Token::String(value) => Ok(JsonValueField::String(value)),
            Token::ObjectStart => self.parse_triple_value().await,
            _ => Err(SparqlResultsError::Json(
                "SRJ value must be a string or object".to_owned(),
            )),
        }
    }

    async fn parse_triple_value(&mut self) -> Result<JsonValueField> {
        let mut subject = None;
        let mut predicate = None;
        let mut object = None;
        loop {
            if self.next_is(TokenKind::ObjectEnd).await? {
                break;
            }
            let key = self.expect_string().await?;
            self.expect(TokenKind::Colon).await?;
            match key.as_str() {
                "subject" => subject = Some(self.parse_value().await?),
                "predicate" => predicate = Some(self.parse_value().await?),
                "object" => object = Some(self.parse_value().await?),
                _ => self.skip_value().await?,
            }
            if !self.consume_comma_or_end().await? {
                break;
            }
        }
        Ok(JsonValueField::Triple {
            subject: subject.ok_or_else(|| missing_triple_part("subject"))?,
            predicate: predicate.ok_or_else(|| missing_triple_part("predicate"))?,
            object: object.ok_or_else(|| missing_triple_part("object"))?,
        })
    }

    fn skip_value(&mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            match self.next_token().await? {
                Token::ObjectStart => loop {
                    if self.next_is(TokenKind::ObjectEnd).await? {
                        break;
                    }
                    self.expect_string().await?;
                    self.expect(TokenKind::Colon).await?;
                    self.skip_value().await?;
                    if !self.consume_comma_or_end().await? {
                        break;
                    }
                },
                Token::ArrayStart => self.skip_array().await?,
                Token::String(_) | Token::Bool(_) | Token::Null | Token::Number => {}
                Token::Eof | Token::ObjectEnd | Token::ArrayEnd | Token::Colon | Token::Comma => {
                    return Err(SparqlResultsError::Json("invalid JSON value".to_owned()));
                }
            }
            Ok(())
        })
    }

    fn skip_array(&mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            while !self.next_is(TokenKind::ArrayEnd).await? {
                self.skip_value().await?;
                if !self.consume_comma_or_end().await? {
                    break;
                }
            }
            Ok(())
        })
    }

    async fn consume_comma_or_end(&mut self) -> Result<bool> {
        match self.next_token().await? {
            Token::Comma => {
                let next = self.next_token().await?;
                if matches!(next, Token::ObjectEnd | Token::ArrayEnd) {
                    return Err(SparqlResultsError::Json(
                        "trailing comma in SRJ document".to_owned(),
                    ));
                }
                self.unread_token(next);
                Ok(true)
            }
            Token::ObjectEnd | Token::ArrayEnd => Ok(false),
            _ => Err(SparqlResultsError::Json(
                "expected comma or closing delimiter".to_owned(),
            )),
        }
    }

    async fn next_is(&mut self, kind: TokenKind) -> Result<bool> {
        let token = self.next_token().await?;
        if token.kind() == kind {
            Ok(true)
        } else {
            self.unread_token(token);
            Ok(false)
        }
    }

    async fn expect(&mut self, kind: TokenKind) -> Result<()> {
        let token = self.next_token().await?;
        if token.kind() == kind {
            Ok(())
        } else {
            Err(SparqlResultsError::Json(format!(
                "expected {kind:?}, got {token:?}"
            )))
        }
    }

    async fn expect_string(&mut self) -> Result<String> {
        match self.next_token().await? {
            Token::String(value) => Ok(value),
            token => Err(SparqlResultsError::Json(format!(
                "expected JSON string, got {token:?}"
            ))),
        }
    }

    async fn expect_bool(&mut self) -> Result<bool> {
        match self.next_token().await? {
            Token::Bool(value) => Ok(value),
            token => Err(SparqlResultsError::Json(format!(
                "expected JSON boolean, got {token:?}"
            ))),
        }
    }

    async fn ensure_eof(&mut self) -> Result<()> {
        match self.next_token().await? {
            Token::Eof => Ok(()),
            token => Err(SparqlResultsError::Json(format!(
                "trailing JSON after SRJ document: {token:?}"
            ))),
        }
    }
}

fn expect_string_value(value: Option<JsonValueField>) -> Result<String> {
    match value {
        Some(JsonValueField::String(value)) => Ok(value),
        Some(JsonValueField::Triple { .. }) | None => Err(SparqlResultsError::MissingElement(
            "SRJ term value".to_owned(),
        )),
    }
}

fn missing_triple_part(part: &str) -> SparqlResultsError {
    SparqlResultsError::MissingElement(format!("SRJ triple {part}"))
}

enum JsonValueField {
    String(String),
    Triple {
        subject: ResultValue,
        predicate: ResultValue,
        object: ResultValue,
    },
}
