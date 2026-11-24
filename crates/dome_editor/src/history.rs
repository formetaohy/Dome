use dome_scene::{Delta, Scene};
use std::collections::VecDeque;

const DEPTH: usize = 128;
const BYTES: usize = 4 << 20;

#[derive(Default)]
pub(crate) struct History {
    undo: VecDeque<(Delta, usize)>,
    redo: Vec<(Delta, usize)>,
    bytes: usize,
}

impl History {
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.bytes = 0;
    }

    pub(crate) fn record(&mut self, delta: Delta, scene: &Scene) {
        if delta.is_empty() {
            return;
        }
        let weight = delta.weight(scene.schema());
        for (_, held) in self.redo.drain(..) {
            self.bytes = self.bytes.saturating_sub(held);
        }
        self.undo.push_back((delta, weight));
        self.bytes += weight;
        while self.undo.len() > 1 && (self.undo.len() > DEPTH || self.bytes > BYTES) {
            let Some((_, held)) = self.undo.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(held);
        }
    }

    pub(crate) fn undo(&mut self, scene: &mut Scene) -> bool {
        let Some((delta, weight)) = self.undo.pop_back() else {
            return false;
        };
        delta.revert(scene);
        self.redo.push((delta, weight));
        true
    }

    pub(crate) fn redo(&mut self, scene: &mut Scene) -> bool {
        let Some((delta, weight)) = self.redo.pop() else {
            return false;
        };
        delta.apply(scene);
        self.undo.push_back((delta, weight));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{BYTES, DEPTH, History};
    use dome_scene::Scene;

    #[test]
    fn undo_history_keeps_its_newest_steps() {
        let mut scene = Scene::new(".", std::sync::Arc::new(dome_schema::Registry::default()));
        let mut history = History::default();
        for index in 0..DEPTH + 4 {
            let (_, delta) = scene
                .edit(|edit| edit.add_node(&format!("node{index}")))
                .expect("a scene adds its node");
            history.record(delta, &scene);
        }
        let mut undone = 0;
        while history.undo(&mut scene) {
            undone += 1;
        }
        assert_eq!(undone, DEPTH, "the history undoes the steps it keeps",);
        assert_eq!(
            scene.nodes().len(),
            4,
            "the history drops the steps its depth outruns",
        );
        let mut redone = 0;
        while history.redo(&mut scene) {
            redone += 1;
        }
        assert_eq!(redone, DEPTH, "the history redoes the steps it keeps");
        assert_eq!(scene.nodes().len(), DEPTH + 4);
    }

    #[test]
    fn a_history_past_its_byte_budget_still_undoes_its_newest_edit() {
        let mut scene = Scene::new(".", std::sync::Arc::new(dome_schema::Registry::default()));
        let mut history = History::default();
        let (_, delta) = scene
            .edit(|edit| edit.add_node("first"))
            .expect("a scene adds its node");
        history.record(delta, &scene);
        history.bytes = BYTES;
        let (_, delta) = scene
            .edit(|edit| edit.add_node("second"))
            .expect("a scene adds its node");
        history.record(delta, &scene);
        assert!(
            history.undo(&mut scene),
            "the newest edit outlives the byte budget its history outruns",
        );
        assert_eq!(
            scene.nodes().len(),
            1,
            "an undone edit leaves the node it added",
        );
    }
}
