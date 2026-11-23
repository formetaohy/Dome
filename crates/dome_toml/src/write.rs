use toml::Value;

pub(crate) fn document(value: &Value) -> String {
    let mut text = toml::to_string(value)
        .unwrap_or_else(|error| panic!("a TOML document writes no text: {error}"));
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
