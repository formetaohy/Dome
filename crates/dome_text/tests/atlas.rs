use dome_text::{Atlas, Fonts, GlyphRequest, GlyphStyle, Slot};

#[test]
fn an_atlas_that_fills_a_page_opens_another_instead_of_dropping_glyphs() {
    let mut fonts = Fonts::system();
    let mut atlas = Atlas::with_capacity(128, 2);
    let style = GlyphStyle::new(28.0, 400.0);
    let mut ready = 0;
    let mut dropped = 0;
    for id in 30..=90u16 {
        atlas.advance();
        let slot = atlas.slot(
            &mut fonts,
            GlyphRequest {
                face: 0,
                id,
                style,
                pixels_per_point: 1.0,
            },
        );
        match slot {
            Slot::Ready(held) => {
                ready += 1;
                assert!(
                    held.page < atlas.page_count(),
                    "a glyph slot names the page it was placed on",
                );
            }
            Slot::Missing => {}
            Slot::Full => dropped += 1,
        }
    }
    assert_eq!(
        dropped, 0,
        "an atlas recycles a page rather than refusing a glyph"
    );
    assert!(
        ready >= 40,
        "an atlas places the glyphs of a filled page and the page after it, not {ready}",
    );
    assert!(
        atlas.page_count() > 1,
        "an atlas that fills its page opens another page",
    );
    assert!(
        atlas.page_count() <= 2,
        "an atlas holds no more pages than the capacity it was made with",
    );
    assert!(
        atlas.drain().iter().any(|region| region.page == 1),
        "an atlas dirties the page its new glyph lands on",
    );
}
