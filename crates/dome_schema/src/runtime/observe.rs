use crate::SceneNode;
use crate::registry::Registry;
use dome_ecs::World;
use dome_scene::Schema;
use dome_toml::Toml;

pub fn observe_runtime(world: &World, registry: &Registry) -> Toml {
    let mut nodes = world
        .iter::<SceneNode>()
        .map(|(entity, node)| {
            let mut fields = vec![
                (String::from("id"), Toml::string(node.id.as_str())),
                (String::from("entity"), Toml::from(entity.index())),
            ];
            if let Some(transform) = crate::runtime::transform::observe(world, entity) {
                fields.push((String::from("transform"), transform));
            }
            for spec in registry.specs() {
                if let Some(component) = registry.observe(spec.name, world, entity) {
                    fields.push((String::from(spec.name), component.runtime()));
                }
            }
            (node.id.clone(), Toml::object(fields))
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.0.cmp(&right.0));
    Toml::array(nodes.into_iter().map(|(_, node)| node).collect())
}
