use dome_scene::{Delta, Scene};
use std::collections::VecDeque;

const DEPTH: usize = 128;

#[derive(Default)]
pub(crate) struct History {
    undo: VecDeque<Delta>,
    redo: Vec<Delta>,
}

impl History {
    pub(crate) fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub(crate) fn record(&mut self, delta: Delta) {
        if delta.is_empty() {
            return;
        }
        self.redo.clear();
        self.undo.push_back(delta);
        while self.undo.len() > DEPTH {
            self.undo.pop_front();
        }
    }

    pub(crate) fn undo(&mut self, scene: &mut Scene) -> bool {
        let Some(delta) = self.undo.pop_back() else {
            return false;
        };
        delta.revert(scene);
        self.redo.push(delta);
        true
    }

    pub(crate) fn redo(&mut self, scene: &mut Scene) -> bool {
        let Some(delta) = self.redo.pop() else {
            return false;
        };
        delta.apply(scene);
        self.undo.push_back(delta);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{DEPTH, History};
    use dome_scene::Scene;

    #[test]
    fn undo_history_keeps_its_newest_steps() {
        let mut scene = Scene::new(".", std::sync::Arc::new(dome_schema::Registry::default()));
        let mut history = History::default();
        for index in 0..DEPTH + 4 {
            let (_, delta) = scene
                .edit(|edit| edit.add_node(&format!("node{index}")))
                .expect("a scene adds its node");
            history.record(delta);
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
}
