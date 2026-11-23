use crate::edit::Edit;
use crate::graph::{GraphAsset, GraphEntry};
use crate::schema::ReferenceKind;
use dome_animation::{Comparison, Parameter};

impl Edit<'_> {
    pub fn add_graph_state(&mut self, graph: &str, id: &str, clip: &str) -> Result<(), String> {
        if self.animation_index(clip).is_none() {
            return Err(format!("the scene holds no animation {clip}"));
        }
        self.change_graph(graph, |asset| asset.add_state(id, clip, 1.0, true))
    }

    pub fn remove_graph_state(&mut self, graph: &str, id: &str) -> Result<(), String> {
        self.change_graph(graph, |asset| asset.remove_state(id))
    }

    pub fn add_graph_transition(
        &mut self,
        graph: &str,
        from: &str,
        to: &str,
        fade: f32,
        exit: f32,
    ) -> Result<(), String> {
        self.change_graph(graph, |asset| asset.add_transition(from, to, fade, exit))
    }

    pub fn remove_graph_transition(&mut self, graph: &str, index: usize) -> Result<(), String> {
        self.change_graph(graph, |asset| asset.remove_transition(index))
    }

    pub fn add_graph_parameter(
        &mut self,
        graph: &str,
        name: &str,
        default: Parameter,
    ) -> Result<(), String> {
        self.change_graph(graph, |asset| asset.add_parameter(name, default))
    }

    pub fn remove_graph_parameter(&mut self, graph: &str, name: &str) -> Result<(), String> {
        self.change_graph(graph, |asset| asset.remove_parameter(name))
    }

    pub fn add_graph_condition(
        &mut self,
        graph: &str,
        transition: usize,
        parameter: &str,
        comparison: Comparison,
        value: Parameter,
    ) -> Result<(), String> {
        self.change_graph(graph, |asset| {
            asset.add_condition(transition, parameter, comparison, value)
        })
    }

    pub fn remove_graph_condition(
        &mut self,
        graph: &str,
        transition: usize,
        index: usize,
    ) -> Result<(), String> {
        self.change_graph(graph, |asset| asset.remove_condition(transition, index))
    }

    fn change_graph<R>(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut GraphAsset) -> Result<R, String>,
    ) -> Result<R, String> {
        if let Some(prefab) = self.asset_owner(ReferenceKind::Graph, id) {
            return Err(format!(
                "the graph {id} belongs to the prefab {}",
                prefab.display(),
            ));
        }
        let at = self
            .graph_index(id)
            .ok_or_else(|| format!("the scene holds no graph {id}"))?;
        self.change_asset::<GraphEntry, R>(at, |entry, _| change(&mut entry.asset))
    }
}
