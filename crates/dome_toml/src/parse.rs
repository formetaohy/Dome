use std::ops::Range;
use toml::Value;

pub(crate) fn document(text: &str, what: &str) -> Value {
    toml::from_str(text).unwrap_or_else(|error| {
        panic!(
            "{what} holds invalid TOML at {}: {}",
            place(text, error.span()),
            error.message(),
        )
    })
}

fn place(text: &str, span: Option<Range<usize>>) -> String {
    let before = span.and_then(|span| text.get(..span.start)).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before
        .chars()
        .rev()
        .take_while(|character| *character != '\n')
        .count()
        + 1;
    format!("line {line} column {column}")
}
