use dome_math::Vec2;

use crate::element::Layer;
use crate::geometry::{Corners, Rect};
use crate::paint::{Clip, Painter};
use crate::style::{Border, Fill, State};
use crate::text::TextStyle;
use crate::theme::{Level, Role};
use crate::ui::{TOOLTIP_DELAY, Ui};

impl<A: Clone> Ui<A> {
    pub(crate) fn update_tooltip(&mut self) {
        let Some(node) = self.hover else {
            self.hovered_since = None;
            self.tooltip = None;
            return;
        };
        let label = self.tree.nodes[node].tooltip.as_deref().or_else(|| {
            match &self.tree.nodes[node].content {
                crate::element::Content::Segment(segment) => segment
                    .tooltips
                    .get(self.tree.nodes[node].runtime.thumb as usize)
                    .map(String::as_str)
                    .filter(|text| !text.is_empty()),
                _ => None,
            }
        });
        let Some(label) = label else {
            self.hovered_since = Some((node, self.time));
            self.tooltip = None;
            return;
        };
        let since = match self.hovered_since {
            Some((id, since)) if id == node => since,
            _ => {
                self.hovered_since = Some((node, self.time));
                self.tooltip = None;
                self.wake(TOOLTIP_DELAY);
                return;
            }
        };
        let remaining = TOOLTIP_DELAY - (self.time - since);
        if remaining > 0.0 {
            self.wake(remaining);
            return;
        }
        if self
            .tooltip
            .as_ref()
            .is_some_and(|(held, shown)| *held == node && shown == label)
        {
            return;
        }
        self.tooltip = Some((node, label.to_string()));
    }

    pub(crate) fn paint_overlays(&mut self) {
        let tooltip = self.tooltip.take();
        let focus = self.focus.filter(|node| self.inside_modal(*node));
        let focus_visible = self.focus_visible;
        let pointer = self.pointer;
        let size = self.size;
        for (index, layer) in Layer::ALL.into_iter().enumerate() {
            let mut instances = std::mem::take(&mut self.chrome_scratch[index]);
            instances.clear();
            {
                let clip = Clip::new(Rect::from_min_size(Vec2::ZERO, size), Corners::ZERO);
                let Self {
                    text,
                    textures,
                    theme,
                    tree,
                    ..
                } = self;
                let mut painter = Painter::new(&mut instances, text, textures, clip);
                if focus_visible
                    && let Some(node) = focus
                    && let Some(held) = tree.nodes.get(node)
                    && held.on_screen
                    && held.layer == layer
                {
                    let corners = held
                        .focus_ring
                        .unwrap_or_else(|| crate::widget::focus_corners(&held.content, theme));
                    let previous = painter.clip();
                    painter.push_clip(held.clip, Corners::ZERO);
                    painter.outlined(
                        held.rect.expand(2.0),
                        corners,
                        Fill::None,
                        Border {
                            width: 2.0,
                            color: theme.palette.primary,
                        },
                    );
                    painter.pop_clip(previous);
                }
                for id in &tree.scrolls {
                    let Some(held) = tree.nodes.get(*id) else {
                        continue;
                    };
                    let (Some(scroll), Some(spec)) = (held.scroll, held.scroll_spec) else {
                        continue;
                    };
                    if held.layer != layer
                        || !held.on_screen
                        || scroll.bar <= 0.01
                        || scroll.limit() <= 0.0
                    {
                        continue;
                    }
                    let bar = crate::scroll::bar(held.rect, spec.axis, scroll);
                    let previous = painter.clip();
                    painter.push_clip(held.clip, Corners::ZERO);
                    painter.rect(
                        bar.thumb,
                        Corners::capsule(),
                        theme.palette.outline.with_alpha(0.65 * scroll.bar),
                    );
                    painter.pop_clip(previous);
                }
            }
            if self.chrome[index].material != instances.material {
                self.chrome[index].assign(&instances.material);
            }
            self.chrome_scratch[index] = instances;
        }
        let mut instances = std::mem::take(&mut self.scratch);
        instances.clear();
        {
            let clip = Clip::new(Rect::from_min_size(Vec2::ZERO, size), Corners::ZERO);
            let mut painter = Painter::new(&mut instances, &mut self.text, &self.textures, clip);
            if let Some((_, text)) = &tooltip {
                let style = TextStyle::new(
                    self.theme.font(Level::Caption),
                    self.theme.palette.on_surface,
                );
                let layout = painter.layout(text, style, 0.0);
                let box_size = layout.size + Vec2::new(16.0, 10.0);
                let mut origin = pointer + Vec2::new(12.0, 16.0);
                if origin.x + box_size.x > self.size.x - 8.0 {
                    origin.x = self.size.x - 8.0 - box_size.x;
                }
                if origin.y + box_size.y > self.size.y - 8.0 {
                    origin.y = pointer.y - 12.0 - box_size.y;
                }
                let rect = Rect::from_min_size(origin, box_size);
                let surface = self
                    .theme
                    .surface(Role::Menu, State::Rest)
                    .with_corners(Corners::all(self.theme.metrics.corner_sm));
                painter.surface(rect, surface);
                painter.text(
                    &layout,
                    rect.min + Vec2::new(8.0, 5.0),
                    self.theme.palette.on_surface,
                );
            }
        }
        if self.overlay.material != instances.material {
            self.overlay.assign(&instances.material);
        }
        self.scratch = instances;
        self.tooltip = tooltip;
    }
}
