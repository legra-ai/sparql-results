use super::language_tag::LanguageTagSyntax;

fn well_formed(tag: &str) -> bool {
    LanguageTagSyntax::new(tag).is_well_formed()
}

#[test]
fn accepts_rfc_5646_examples() {
    for tag in [
        "de",
        "fr",
        "ja",
        "i-enochian",
        "zh-Hant",
        "zh-Hans-CN",
        "sr-Latn-RS",
        "sl-rozaj",
        "sl-rozaj-biske",
        "sl-nedis",
        "de-CH-1901",
        "sl-IT-nedis",
        "hy-Latn-IT-arevela",
        "de-DE",
        "en-US",
        "es-419",
        "de-CH-x-phonebk",
        "az-Arab-x-AZE-derbend",
        "x-whatever",
        "qaa-Qaaa-QM-x-southern",
        "de-Qaaa",
        "sr-Latn-QM",
        "sr-Qaaa-RS",
        "en-US-u-islamcal",
        "zh-CN-a-myext-x-private",
        "en-a-myext-b-another",
        "zh-yue-HK",
        "EN-gb-OED",
        "ar",
    ] {
        assert!(well_formed(tag), "{tag} should be well-formed");
    }
}

#[test]
fn refuses_malformed_tags() {
    for tag in [
        "",
        "-",
        "en-",
        "-en",
        "en--US",
        "en_US",
        "a",
        "toolongprimary",
        "123",
        "de-419-DE",
        "a-DE",
        "ar-a-aaa-b-bbb-a-ccc",
        "de-1996-1996",
        "en-a",
        "en-x",
        "x",
        "en-US-u",
        "énglish",
        "en-abcdefghi",
    ] {
        assert!(!well_formed(tag), "{tag} should be refused");
    }
}
