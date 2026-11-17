#![allow(missing_docs)]

//! Reader-side validation of language-tagged and directional literals.

use sparql_results::bounded::{
    SparqlResult,
    parse_srj_bounded,
    parse_srx_bounded,
};
use sparql_results::{
    BaseDirection,
    LiteralError,
    ResultValue,
    SparqlResultsError,
};

fn srj_literal(fields: &str) -> String {
    format!(
        r#"{{"head":{{"vars":["x"]}},"results":{{"bindings":[{{"x":{{"type":"literal","value":"v",{fields}}}}}]}}}}"#
    )
}

fn srx_literal(attributes: &str) -> String {
    format!(
        r#"<sparql xmlns="http://www.w3.org/2005/sparql-results#" xmlns:its="http://www.w3.org/2005/11/its"><head><variable name="x"/></head><results><result><binding name="x"><literal {attributes}>v</literal></binding></result></results></sparql>"#
    )
}

async fn srj_error(fields: &str) -> LiteralError {
    let document = srj_literal(fields);
    match parse_srj_bounded(document.as_bytes()).await {
        Err(SparqlResultsError::InvalidLiteral(error)) => error,
        other => panic!("expected an invalid-literal error for {document}, got {other:?}"),
    }
}

async fn srx_error(attributes: &str) -> LiteralError {
    let document = srx_literal(attributes);
    match parse_srx_bounded(document.as_bytes()).await {
        Err(SparqlResultsError::InvalidLiteral(error)) => error,
        other => panic!("expected an invalid-literal error for {document}, got {other:?}"),
    }
}

fn only_value(result: SparqlResult) -> ResultValue {
    let SparqlResult::Select { mut rows, .. } = result else {
        panic!("expected SELECT result");
    };
    rows.remove(0).bindings.shift_remove("x").unwrap()
}

const LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
const DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

#[tokio::test]
async fn srj_bare_dir_key_is_refused_with_its_dir_hint() {
    let error = srj_error(r#""xml:lang":"ar","dir":"rtl""#).await;
    assert_eq!(error, LiteralError::BareDirKey);
    assert!(error.to_string().contains("its:dir"), "{error}");
}

#[tokio::test]
async fn srx_bare_dir_attribute_is_refused_with_its_dir_hint() {
    let error = srx_error(r#"xml:lang="ar" dir="rtl""#).await;
    assert_eq!(error, LiteralError::BareDirKey);
    assert!(error.to_string().contains("its:dir"), "{error}");
}

#[tokio::test]
async fn direction_without_language_is_refused() {
    assert_eq!(
        srj_error(r#""its:dir":"ltr""#).await,
        LiteralError::DirectionWithoutLanguage
    );
    assert_eq!(
        srx_error(r#"its:dir="ltr""#).await,
        LiteralError::DirectionWithoutLanguage
    );
}

#[tokio::test]
async fn datatype_contradicting_language_is_refused() {
    let json = srj_error(&format!(r#""xml:lang":"en","datatype":"{XSD_STRING}""#)).await;
    assert!(
        matches!(json, LiteralError::DatatypeContradictsLanguage { .. }),
        "{json}"
    );
    let json = srj_error(&format!(
        r#""xml:lang":"en","datatype":"{DIR_LANG_STRING}""#
    ))
    .await;
    assert!(
        matches!(json, LiteralError::DatatypeContradictsLanguage { .. }),
        "{json}"
    );
    let json = srj_error(&format!(
        r#""xml:lang":"en","its:dir":"ltr","datatype":"{LANG_STRING}""#
    ))
    .await;
    assert!(
        matches!(json, LiteralError::DatatypeContradictsLanguage { .. }),
        "{json}"
    );
    let xml = srx_error(&format!(r#"xml:lang="en" datatype="{XSD_STRING}""#)).await;
    assert!(
        matches!(xml, LiteralError::DatatypeContradictsLanguage { .. }),
        "{xml}"
    );
}

#[tokio::test]
async fn language_string_datatype_without_language_is_refused() {
    let json = srj_error(&format!(r#""datatype":"{LANG_STRING}""#)).await;
    assert!(
        matches!(json, LiteralError::DatatypeRequiresLanguage { .. }),
        "{json}"
    );
    let xml = srx_error(&format!(r#"datatype="{DIR_LANG_STRING}""#)).await;
    assert!(
        matches!(xml, LiteralError::DatatypeRequiresLanguage { .. }),
        "{xml}"
    );
}

#[tokio::test]
async fn invalid_language_tag_is_refused() {
    for tag in [
        "",
        "en_US",
        "toolongprimary",
        "en-a-bbb-a-ccc",
        "de-1996-1996",
    ] {
        assert_eq!(
            srj_error(&format!(r#""xml:lang":"{tag}""#)).await,
            LiteralError::InvalidLanguageTag(tag.to_owned()),
            "SRJ {tag:?}"
        );
        assert_eq!(
            srx_error(&format!(r#"xml:lang="{tag}""#)).await,
            LiteralError::InvalidLanguageTag(tag.to_owned()),
            "SRX {tag:?}"
        );
    }
}

#[tokio::test]
async fn consistent_literals_are_accepted() {
    let json = parse_srj_bounded(
        srj_literal(&format!(
            r#""xml:lang":"zh-Hant-TW","its:dir":"rtl","datatype":"{DIR_LANG_STRING}""#
        ))
        .as_bytes(),
    )
    .await
    .unwrap();
    assert_eq!(
        only_value(json),
        ResultValue::Literal {
            value: "v".to_owned(),
            lang: Some("zh-Hant-TW".to_owned()),
            datatype: Some(DIR_LANG_STRING.to_owned()),
            dir: Some(BaseDirection::Rtl),
        }
    );
    let xml = parse_srx_bounded(
        srx_literal(&format!(r#"xml:lang="en-GB" datatype="{LANG_STRING}""#)).as_bytes(),
    )
    .await
    .unwrap();
    assert_eq!(
        only_value(xml),
        ResultValue::Literal {
            value: "v".to_owned(),
            lang: Some("en-GB".to_owned()),
            datatype: Some(LANG_STRING.to_owned()),
            dir: None,
        }
    );
    let plain = parse_srj_bounded(srj_literal(&format!(r#""datatype":"{XSD_STRING}""#)).as_bytes())
        .await
        .unwrap();
    assert!(matches!(
        only_value(plain),
        ResultValue::Literal { lang: None, .. }
    ));
}
