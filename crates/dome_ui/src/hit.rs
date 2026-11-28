use dome_math::Vec2;

use crate::geometry::Rect;
use crate::node::NodeId;
use crate::text::Text;
use crate::theme::Theme;
use crate::tree::Tree;

#[derive(Clone, Copy)]
pub(crate) struct Hit {
    pub node: NodeId,
    pub rect: Rect,
    pub clip: Rect,
    pub layer: usize,
    pub blocking: bool,
}

pub(crate) fn collect<A>(
    tree: &Tree<A>,
    id: NodeId,
    hits: &mut Vec<Hit>,
    text: &mut Text,
    theme: &Theme,
) {
    walk(tree, id, hits, text, theme, false, true);
}

fn walk<A>(
    tree: &Tree<A>,
    id: NodeId,
    hits: &mut Vec<Hit>,
    text: &mut Text,
    theme: &Theme,
    within: bool,
    enabled: bool,
) {
    let node = &tree.nodes[id];
    if !node.on_screen {
        return;
    }
    let enabled = enabled && node.enabled;
    let clip = node.clip;
    let visible = clip.width() > 0.0 && clip.height() > 0.0;
    let interactive = node.interactive();
    let scroll = node.scroll_spec.is_some();
    let popup = node.popup_spec.is_some() && node.runtime.open;
    let blocking =
        node.blocks_pointer || (!within && !interactive && !scroll && !popup && opaque(node));
    if visible && (interactive || scroll || popup || blocking) && (enabled || blocking) {
        let rect = if interactive {
            crate::widget::hit_rect(node, text, theme)
        } else {
            node.rect
        };
        hits.push(Hit {
            node: id,
            rect,
            clip,
            layer: node.layer.index(),
            blocking,
        });
    }
    let held = within || interactive || scroll || popup;
    for child in &node.children {
        walk(tree, *child, hits, text, theme, held, enabled);
    }
}

fn opaque<A>(node: &crate::node::Node<A>) -> bool {
    (node.popup_spec.is_some() && node.runtime.open)
        || node
            .background
            .is_some_and(|surface| surface.fill.is_visible())
}

pub(crate) fn top(hits: &[Hit], pointer: Vec2) -> Option<&Hit> {
    hits.iter()
        .filter(|hit| hit.clip.contains(pointer) && hit.rect.contains(pointer))
        .max_by_key(|hit| hit.layer)
}

pub(crate) fn resolve(hits: &[Hit], pointer: Vec2) -> Option<&Hit> {
    top(hits, pointer).filter(|hit| !hit.blocking)
}
