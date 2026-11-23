use crate::node::SceneNode;
use crate::registry::Registry;
use dome_ecs::World;
use dome_toml::Toml;

pub fn observe_runtime(world: &World, registry: &Registry) -> Toml {
    let mut nodes = world
        .iter::<SceneNode>()
        .map(|(entity, node)| {
            let mut fields = vec![
                (String::from("id"), Toml::string(node.id.as_str())),
                (String::from("entity"), Toml::from(entity.index())),
            ];
            for registration in registry.registrations() {
                if let Some(component) = (registration.observe)(world, entity) {
                    fields.push((String::from(registration.spec.name), component.runtime()));
                }
            }
            (node.id.clone(), Toml::object(fields))
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.0.cmp(&right.0));
    Toml::array(nodes.into_iter().map(|(_, node)| node).collect())
}
