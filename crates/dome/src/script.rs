use dome_app::{App, Time};
use dome_asset::Change;
use dome_ecs::{Entity, LocalTransform, World};
use dome_math::Quat;
use dome_schema::SceneNode;
use lampas::{Hosts, Scripts, Type, Value};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

pub(crate) fn install(app: &mut App, root: &Path) {
    let scripts = Rc::new(RefCell::new(load(root)));
    let boot = Rc::clone(&scripts);
    app.add_startup(move |world: &mut World, _| boot.borrow_mut().boot(world));
    let update = Rc::clone(&scripts);
    app.add_system(move |world: &mut World| update.borrow_mut().update(world));
    let fixed = Rc::clone(&scripts);
    app.add_fixed(move |world: &mut World| fixed.borrow_mut().fixed(world));
    let reload = Rc::clone(&scripts);
    let folder = root.to_path_buf();
    app.add_reload(move |_: &mut World, change: Change| {
        if !change.script {
            return;
        }
        match Scripts::load(&folder, hosts()) {
            Ok(fresh) => *reload.borrow_mut() = fresh,
            Err(error) => eprintln!(
                "the lampas scripts of {} keep the version they checked: {error}",
                folder.display(),
            ),
        }
    });
}

fn load(root: &Path) -> Scripts {
    Scripts::load(root, hosts())
        .unwrap_or_else(|error| panic!("the lampas scripts of {} check: {error}", root.display()))
}

fn hosts() -> Hosts {
    let mut hosts = Hosts::new();
    hosts
        .function("log", &[Type::TEXT], Type::VOID, |arguments, _world| {
            println!("[lampas] {}", arguments[0].text());
            Value::Unit
        })
        .function("delta", &[], Type::FLOAT, |_, world| {
            Value::Float(seconds(world, Time::delta))
        })
        .function("elapsed", &[], Type::FLOAT, |_, world| {
            Value::Float(seconds(world, Time::elapsed))
        })
        .function("find", &[Type::TEXT], Type::ENTITY, |arguments, world| {
            let id = arguments[0].text();
            let entity = world
                .iter::<SceneNode>()
                .find(|(_, node)| node.id == id)
                .map(|(entity, _)| entity)
                .unwrap_or_else(|| panic!("the lampas script finds no scene node {id}"));
            Value::Entity(entity)
        })
        .function("alive", &[Type::ENTITY], Type::BOOL, |arguments, world| {
            Value::Bool(world.is_alive(arguments[0].entity()))
        })
        .function(
            "position",
            &[Type::ENTITY],
            Type::VECTOR,
            |arguments, world| Value::Vector(transform(world, arguments[0].entity()).0.translation),
        )
        .function(
            "set_position",
            &[Type::ENTITY, Type::VECTOR],
            Type::VOID,
            |arguments, world| {
                transform(world, arguments[0].entity()).0.translation = arguments[1].vector();
                Value::Unit
            },
        )
        .function(
            "rotate_y",
            &[Type::ENTITY, Type::FLOAT],
            Type::VOID,
            |arguments, world| {
                let rotation = Quat::from_rotation_y(arguments[1].float() as f32);
                let transform = transform(world, arguments[0].entity());
                transform.0.rotation = rotation * transform.0.rotation;
                Value::Unit
            },
        );
    hosts
}

fn transform(world: &mut World, entity: Entity) -> &mut LocalTransform {
    world.get_mut::<LocalTransform>(entity).unwrap_or_else(|| {
        panic!(
            "the lampas script transforms the entity {} that holds no local transform",
            entity.index(),
        )
    })
}

fn seconds(world: &World, span: impl FnOnce(&Time) -> Duration) -> f64 {
    let time = world
        .resource::<Time>()
        .expect("the application keeps its time");
    span(time).as_secs_f64()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_starter_script_checks_against_the_engine_hosts() {
        let hosts = super::hosts();
        lampas::compile("main.lampas", dome_project::STARTER_SCRIPT, &hosts)
            .expect("the starter script checks");
    }
}
