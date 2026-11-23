use crate::element::{Axis, Element, Flow, Key, Layout, Length, Size};
use crate::geometry::Edges;

pub fn popup<'a, A>(
    anchor: Key,
    width: f32,
    children: impl IntoIterator<Item = Element<'a, A>>,
) -> Element<'a, A> {
    Element::new(Layout {
        flow: Flow::Column,
        size: Size::new(Length::Fill, Length::Auto),
        padding: Edges::all(8.0),
        gap: 2.0,
        ..Layout::column()
    })
    .popup(anchor, width)
    .scroll(Axis::Column, Length::Fill, None)
    .layer(crate::element::Layer::Popup)
    .children(children)
}
