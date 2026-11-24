use dome_text::{Align, Text, TypeStyle};

#[test]
fn a_layout_answers_the_caret_and_the_column_of_the_clusters_it_places() {
    let mut text = Text::new();
    let style = TypeStyle::new(16.0, 400.0, 20.0);
    let value = "Ave";
    let layout = text.layout(value, style, 0.0, false, false, Align::Start);
    assert_eq!(
        layout.glyphs.len(),
        value.len(),
        "a plain word places one glyph per letter",
    );
    for (byte, glyph) in layout.glyphs.iter().enumerate() {
        assert_eq!(
            layout.caret(0, byte),
            glyph.x,
            "the caret of letter {byte} sits at the glyph it precedes",
        );
        assert_eq!(
            layout.column(0, glyph.x),
            byte,
            "a click on the glyph of letter {byte} lands on that letter",
        );
    }
    assert_eq!(
        layout.caret(0, value.len()),
        layout.size.x,
        "the caret of the end sits at the width the layout reports",
    );
    assert_eq!(
        layout.column(0, -10.0),
        0,
        "a click left of the text lands on its first cluster",
    );
    assert_eq!(
        layout.column(0, layout.size.x + 10.0),
        value.len(),
        "a click right of the text lands on its last cluster",
    );
}
