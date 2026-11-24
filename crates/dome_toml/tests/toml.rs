use dome_toml::Toml;

const DOCUMENT: &str =
    "format = \"dome_scene\"\nversion = 1\nroughness = 0.72\nimages = [\"albedo.png\"]\n";

#[test]
fn a_document_writes_back_the_text_it_parses() {
    let parsed = Toml::parse(DOCUMENT, "the test").expect("the test document parses");
    assert_eq!(parsed.write(), DOCUMENT);
    assert_eq!(
        Toml::parse(&parsed.write(), "the test").expect("the written document parses"),
        parsed,
    );
    assert_eq!(
        Toml::from(0.72f32)
            .f32_value("the test")
            .expect("a float reads"),
        0.72,
    );
    assert_eq!(
        parsed
            .object_fields("the test")
            .expect("a document holds a table")
            .len(),
        4,
    );
}
