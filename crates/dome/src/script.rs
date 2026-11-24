use dome_app::{App, Time};
use dome_asset::Change;
use dome_ecs::{Entity, LocalTransform, Parent, World, children};
use dome_math::Quat;
use dome_schema::NodeMap;
use lampas::{Hosts, Kind, Scripts, Trap, Type, Value};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, Default)]
pub(crate) struct Faults {
    state: Rc<RefCell<State>>,
}

#[derive(Default)]
struct State {
    reported: Option<String>,
    first: Option<String>,
}

impl Faults {
    pub(crate) fn take(&self) -> Option<String> {
        self.state.borrow_mut().first.take()
    }

    fn clear_reported(&self) {
        self.state.borrow_mut().reported = None;
    }

    fn report(&self, root: &Path, outcome: Result<(), Trap>) {
        let Err(trap) = outcome else {
            return;
        };
        let message = format!("[lampas {}] {trap}", root.display());
        let mut state = self.state.borrow_mut();
        if state.first.is_none() {
            state.first = Some(message.clone());
        }
        if state.reported.as_deref() == Some(message.as_str()) {
            return;
        }
        eprintln!("{message}");
        state.reported = Some(message);
    }
}

pub(crate) fn install(app: &mut App, root: &Path) -> Result<Faults, String> {
    let scripts = Rc::new(RefCell::new(load(root)?));
    let faults = Faults::default();
    let boot = Rc::clone(&scripts);
    let boot_faults = faults.clone();
    let boot_root = root.to_path_buf();
    app.add_startup(move |world: &mut World, _| {
        let outcome = boot.borrow_mut().boot(world);
        let failed = outcome.is_err();
        boot_faults.report(&boot_root, outcome);
        if failed {
            world.insert_resource(dome_app::Exit);
        }
    });
    let update = Rc::clone(&scripts);
    let update_faults = faults.clone();
    let update_root = root.to_path_buf();
    app.add_system(move |world: &mut World| {
        update_faults.report(&update_root, update.borrow_mut().update(world));
    });
    let fixed = Rc::clone(&scripts);
    let fixed_faults = faults.clone();
    let fixed_root = root.to_path_buf();
    app.add_fixed(move |world: &mut World| {
        fixed_faults.report(&fixed_root, fixed.borrow_mut().fixed(world));
    });
    let reload = Rc::clone(&scripts);
    let reload_faults = faults.clone();
    let folder = root.to_path_buf();
    app.add_reload(move |world: &mut World, change: Change| {
        if !change.script {
            return;
        }
        match Scripts::load(&folder, hosts()) {
            Ok(mut fresh) => {
                lampas::reset(world);
                reload_faults.clear_reported();
                reload_faults.report(&folder, fresh.boot(world));
                *reload.borrow_mut() = fresh;
            }
            Err(error) => eprintln!(
                "the lampas scripts of {} keep the version they checked: {error}",
                folder.display(),
            ),
        }
    });
    Ok(faults)
}

fn load(root: &Path) -> Result<Scripts, String> {
    Scripts::load(root, hosts())
        .map_err(|error| format!("the lampas scripts of {} check: {error}", root.display()))
}

fn hosts() -> Hosts {
    let mut hosts = Hosts::new();
    hosts
        .function("log", &[Type::TEXT], Type::VOID, |arguments, _| {
            println!("[lampas] {}", arguments[0].text());
            Ok(Value::Unit)
        })
        .function("delta", &[], Type::FLOAT, |_, call| {
            Ok(Value::Float(seconds(call.world(), Time::delta)))
        })
        .function("elapsed", &[], Type::FLOAT, |_, call| {
            Ok(Value::Float(seconds(call.world(), Time::elapsed)))
        })
        .function("find", &[Type::TEXT], Type::ENTITY, |arguments, call| {
            let id = arguments[0].text();
            let entity = call
                .world()
                .resource::<NodeMap>()
                .and_then(|nodes| nodes.id(id))
                .ok_or_else(|| Trap::new(format!("the lampas script finds no scene node {id}")))?;
            Ok(Value::Entity(entity))
        })
        .function("alive", &[Type::ENTITY], Type::BOOL, |arguments, call| {
            Ok(Value::Bool(call.world().is_alive(arguments[0].entity())))
        })
        .function(
            "position",
            &[Type::ENTITY],
            Type::VECTOR,
            |arguments, call| {
                Ok(Value::Vector(
                    transform(call.world(), arguments[0].entity())?
                        .0
                        .translation,
                ))
            },
        )
        .function(
            "set_position",
            &[Type::ENTITY, Type::VECTOR],
            Type::VOID,
            |arguments, call| {
                let translation = arguments[1].vector();
                call.edit::<LocalTransform>(arguments[0].entity(), |held| {
                    held.0.translation = translation;
                })
                .map(|()| Value::Unit)
            },
        )
        .function(
            "rotate_y",
            &[Type::ENTITY, Type::FLOAT],
            Type::VOID,
            |arguments, call| {
                let rotation = Quat::from_rotation_y(arguments[1].float() as f32);
                call.edit::<LocalTransform>(arguments[0].entity(), |held| {
                    held.0.rotation = rotation * held.0.rotation;
                })
                .map(|()| Value::Unit)
            },
        )
        .function(
            "parent",
            &[Type::ENTITY],
            Type::list(Kind::Entity),
            |arguments, call| {
                let parent = call.world().get::<Parent>(arguments[0].entity());
                Ok(Value::from_list(
                    parent
                        .map(|held| Value::Entity(held.entity()))
                        .into_iter()
                        .collect(),
                ))
            },
        )
        .function(
            "children",
            &[Type::ENTITY],
            Type::list(Kind::Entity),
            |arguments, call| {
                let held = children(call.world(), arguments[0].entity())
                    .iter()
                    .map(|child| Value::Entity(*child))
                    .collect();
                Ok(Value::from_list(held))
            },
        );
    hosts
}

fn transform(world: &World, entity: Entity) -> Result<&LocalTransform, Trap> {
    world.get::<LocalTransform>(entity).ok_or_else(|| {
        Trap::new(format!(
            "the lampas script reads the entity {} that holds no local transform",
            entity.index(),
        ))
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
        lampas::compile("main.amp", dome_project::STARTER_SCRIPT, &hosts)
            .expect("the starter script checks");
    }
}
