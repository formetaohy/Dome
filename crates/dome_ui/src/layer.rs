use dome_math::Vec2;
use slotmap::SecondaryMap;

use crate::element::{CanvasPaint, Layer};
use crate::geometry::{Corners, Rect};
use crate::node::NodeId;
use crate::paint::{Clip, Instance, Instances, Painter};
use crate::style::State;
use crate::theme::Role;
use crate::tree::Tree;
use crate::ui::Ui;
use crate::widget::PaintInfo;

const CULL_MARGIN: f32 = 64.0;

impl<A: Clone> Ui<A> {
    pub(crate) fn paint_layers<'a>(&mut self, canvases: &SecondaryMap<NodeId, CanvasPaint<'a>>) {
        if std::mem::take(&mut self.tree.structure) {
            for layer in &mut self.layers {
                layer.rebuild = true;
            }
        }
        let root = self.tree.root;
        self.paint_node(root, canvases);
        if self.layers.iter().any(|layer| layer.rebuild) {
            let mut materials: [Vec<Instance>; Layer::ALL.len()] = std::array::from_fn(|index| {
                let mut material = std::mem::take(&mut self.layers[index].material);
                if self.layers[index].rebuild {
                    material.clear();
                }
                material
            });
            Self::collect_layers(&mut self.tree, &mut self.layers, root, &mut materials);
            for (index, material) in materials.into_iter().enumerate() {
                let layer = &mut self.layers[index];
                layer.material = material;
                if !layer.rebuild {
                    continue;
                }
                layer.rebuild = false;
                layer.dirty = Some((0, layer.material.len()));
                layer.touch();
            }
        }
    }

    fn paint_node<'a>(&mut self, id: NodeId, canvases: &SecondaryMap<NodeId, CanvasPaint<'a>>) {
        if !self.tree.nodes[id].on_screen {
            if self.tree.nodes[id].painted {
                let layer = self.tree.nodes[id].layer.index();
                self.tree.nodes[id].painted = false;
                self.layers[layer].rebuild = true;
            }
            return;
        }
        let culled = self.tree.nodes[id].popup_spec.is_none()
            && !self.tree.nodes[id]
                .rect
                .expand(CULL_MARGIN)
                .intersects(self.tree.nodes[id].clip);
        let tracked = self.hover == Some(id) || self.press == Some(id) || self.drag == Some(id);
        let moved = tracked && self.pointer != self.tree.nodes[id].pointer_seen;
        if !culled && (self.tree.nodes[id].paint_dirty || moved) {
            self.repaint(id, canvases);
            self.tree.nodes[id].pointer_seen = self.pointer;
        }
        #[cfg(debug_assertions)]
        if !culled
            && !self.tree.nodes[id].paint_dirty
            && matches!(
                self.tree.nodes[id].canvas,
                crate::element::CanvasKind::Static | crate::element::CanvasKind::Props(_)
            )
        {
            self.verify_canvas(id, canvases);
        }
        let count = self.tree.nodes[id].paint.len();
        let included = !culled && count > 0;
        let layer = self.tree.nodes[id].layer.index();
        if included != self.tree.nodes[id].painted {
            self.tree.nodes[id].painted = included;
            self.layers[layer].rebuild = true;
        }
        if included
            && !self.layers[layer].rebuild
            && self.tree.nodes[id].paint_version != self.tree.nodes[id].layer_version
        {
            let (start, end) = self.tree.nodes[id].layer_range;
            if (end - start) as usize == count {
                let material = std::mem::take(&mut self.tree.nodes[id].paint.material);
                self.layers[layer].patch(start as usize, &material);
                self.tree.nodes[id].paint.material = material;
                self.tree.nodes[id].layer_version = self.tree.nodes[id].paint_version;
            } else {
                self.layers[layer].rebuild = true;
            }
        }
        for index in 0..self.tree.nodes[id].children.len() {
            let child = self.tree.nodes[id].children[index];
            self.paint_node(child, canvases);
        }
    }

    fn repaint<'a>(&mut self, id: NodeId, canvases: &SecondaryMap<NodeId, CanvasPaint<'a>>) {
        let previous = std::mem::take(&mut self.tree.nodes[id].paint);
        let mut fresh = std::mem::take(&mut self.scratch);
        self.paint_into(id, canvases, &mut fresh);
        if previous.material == fresh.material {
            self.tree.nodes[id].paint = previous;
            self.scratch = fresh;
        } else {
            self.tree.nodes[id].paint = fresh;
            self.tree.nodes[id].paint_version = self.tree.nodes[id].paint_version.wrapping_add(1);
            self.scratch = previous;
        }
        self.tree.nodes[id].paint_dirty = false;
    }

    #[cfg(debug_assertions)]
    fn verify_canvas<'a>(&mut self, id: NodeId, canvases: &SecondaryMap<NodeId, CanvasPaint<'a>>) {
        if !canvases.contains_key(id) {
            return;
        }
        let mut instances = std::mem::take(&mut self.scratch);
        self.paint_into(id, canvases, &mut instances);
        assert!(
            instances.material == self.tree.nodes[id].paint.material,
            "the cached canvas {:#x} of node {:#x} changed without new props",
            match self.tree.nodes[id].canvas {
                crate::element::CanvasKind::Props(hash) => hash,
                _ => 0,
            },
            self.tree.nodes[id].key.id()
        );
        self.scratch = instances;
    }

    fn paint_into<'a>(
        &mut self,
        id: NodeId,
        canvases: &SecondaryMap<NodeId, CanvasPaint<'a>>,
        instances: &mut Instances,
    ) {
        let (
            rect,
            clip,
            clip_corners,
            background,
            hover_surface,
            popup,
            enabled,
            hover,
            press,
            engaged,
        ) = {
            let node = &self.tree.nodes[id];
            (
                node.rect,
                node.clip,
                node.clip_corners,
                node.background,
                node.hover_surface,
                node.popup_spec.is_some(),
                node.enabled,
                node.runtime.hover.value,
                node.runtime.press.value,
                node.runtime.engaged.value,
            )
        };
        let background = match (background, hover_surface) {
            (Some(base), Some(target)) => Some(base.lerp(target, engaged.clamp(0.0, 1.0))),
            (base, _) => base,
        };
        let background =
            background.or_else(|| popup.then(|| self.theme.surface(Role::Menu, State::Rest)));
        instances.clear();
        let time = self.time;
        let screen_clip = Clip::new(Rect::from_min_size(Vec2::ZERO, self.size), Corners::ZERO);
        let node_clip = Clip::new(clip, clip_corners);
        {
            let mut painter = Painter::new(
                instances,
                &mut self.text,
                &self.textures,
                if popup { screen_clip } else { node_clip },
            );
            if let Some(surface) = background
                && !rect.is_empty()
            {
                painter.surface(rect, surface);
            }
            if popup {
                painter.pop_clip(node_clip);
            }
            let preedit = if self.edit == Some(id) {
                self.preedit.as_deref()
            } else {
                None
            };
            let info = PaintInfo {
                theme: &self.theme,
                rect,
                hover,
                press,
                engaged,
                time,
                enabled,
                preedit,
            };
            let node = &mut self.tree.nodes[id];
            let caret = crate::widget::paint(&node.content, &node.runtime, &mut painter, &info);
            if self.edit == Some(id) {
                self.edit_caret = caret;
            }
            if let Some(paint) = canvases.get(id) {
                let info = crate::element::CanvasInfo {
                    rect,
                    theme: &self.theme,
                    hover,
                    press,
                    engaged,
                    time,
                    pointer: self.pointer,
                    inside: rect.contains(self.pointer),
                };
                paint(&mut painter, info);
            }
        }
    }

    fn collect_layers(
        tree: &mut Tree<A>,
        layers: &mut [crate::paint::LayerBuffer; Layer::ALL.len()],
        id: NodeId,
        materials: &mut [Vec<Instance>; Layer::ALL.len()],
    ) {
        let node = &mut tree.nodes[id];
        let index = node.layer.index();
        if layers[index].rebuild {
            if node.on_screen && node.painted && !node.paint.is_empty() {
                let start = materials[index].len() as u32;
                materials[index].extend_from_slice(&node.paint.material);
                node.layer_range = (start, materials[index].len() as u32);
                node.layer_version = node.paint_version;
            } else {
                node.painted = false;
                node.layer_range = (0, 0);
            }
        }
        for index in 0..tree.nodes[id].children.len() {
            let child = tree.nodes[id].children[index];
            Self::collect_layers(tree, layers, child, materials);
        }
    }
}
