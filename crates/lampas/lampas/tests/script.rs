use dome_ecs::{Entity, World};
use lampas::{Hosts, Scripts, Type, Value, compile, compile_folder};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn hosts() -> (Hosts, Rc<RefCell<Vec<String>>>) {
    let messages = Rc::new(RefCell::new(Vec::new()));
    let held = Rc::clone(&messages);
    let mut hosts = Hosts::new();
    hosts.function(
        "log",
        &[Type::TEXT],
        Type::VOID,
        move |arguments, _world| {
            held.borrow_mut().push(String::from(arguments[0].text()));
            Value::Unit
        },
    );
    hosts.function("delta", &[], Type::FLOAT, |_, _world| Value::Float(1.0));
    hosts.function("damage", &[], Type::FLOAT, |_, _world| Value::Float(1.0));
    (hosts, messages)
}

fn component(scripts: &Scripts, name: &str) -> u16 {
    scripts
        .module()
        .components
        .iter()
        .position(|component| component.name == name)
        .unwrap_or_else(|| panic!("the script holds no {name} component")) as u16
}

fn field(scripts: &Scripts, component_name: &str, name: &str) -> u16 {
    scripts.module().components[component(scripts, component_name) as usize]
        .fields
        .iter()
        .position(|field| field.name == name)
        .unwrap_or_else(|| panic!("the {component_name} component holds no {name} field"))
        as u16
}

fn value(scripts: &Scripts, entity: Entity, component_name: &str, name: &str) -> Value {
    scripts
        .store()
        .read_entity(
            entity,
            component(scripts, component_name),
            field(scripts, component_name, name),
        )
        .unwrap_or_else(|| panic!("the entity {} holds no {component_name}", entity.index()))
}

const WALK: &str = r#"
component Motion {
    speed: float,
    distance: float = 0.0,
}

component Health {
    value: float = 10.0,
}

global ticks: int = 0;

fn spend(health: write Health, amount: float) {
    health.value -= amount;
}

fn tense() -> string {
    if ticks > 0 {
        return "!";
    }
    return "?";
}

boot {
    let first = spawn();
    add(first, Motion { speed: 2.0 });
    add(first, Health {});
}

system move(motion: write Motion) {
    motion.distance += motion.speed * delta();
    if motion.distance > 5.0 {
        motion.speed = 0.0;
    }
}

system hurt(motion: read Motion, health: write Health) {
    if motion.distance > 1.0 {
        spend(health, damage());
    }
}

fixed system count() {
    ticks += 1;
}

system report() {
    let names = ["alpha", "beta"];
    let total = 0.0;
    let size = length(names);
    for name in names {
        total += float(length(name));
    }
    log(text(total) + " " + text(size) + " " + names[0] + tense());
}
"#;

#[test]
fn a_script_walks_the_components_its_systems_hold() {
    let (hosts, messages) = hosts();
    let module = compile("walk.lampas", WALK, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world);
    let entity = scripts
        .store()
        .entities(component(&scripts, "Motion"))
        .next()
        .expect("the boot spawns the entity its systems walk");
    assert_eq!(
        value(&scripts, entity, "Motion", "distance"),
        Value::Float(0.0)
    );
    assert_eq!(
        value(&scripts, entity, "Health", "value"),
        Value::Float(10.0)
    );
    for _ in 0..2 {
        scripts.fixed(&mut world);
    }
    for _ in 0..4 {
        scripts.update(&mut world);
    }
    assert_eq!(
        value(&scripts, entity, "Motion", "speed"),
        Value::Float(0.0)
    );
    assert_eq!(
        value(&scripts, entity, "Motion", "distance"),
        Value::Float(6.0)
    );
    assert_eq!(
        value(&scripts, entity, "Health", "value"),
        Value::Float(6.0)
    );
    assert_eq!(scripts.globals()[0], Value::Int(2));
    assert_eq!(
        messages.borrow().as_slice(),
        ["9 2 alpha!"; 4],
        "the zero component system reports once per frame",
    );
}

const CALL: &str = r#"
boot {
    let first = spawn();
    add(first, Mark {});
    add(first, Tally {});
    let second = spawn();
    add(second, Mark {});
    add(second, Tally {});
    let third = spawn();
    add(third, Mark {});
    add(third, Tally {});
}

component Mark {
    seen: int = 0,
}

component Tally {
    hits: int = 0,
}

global walks: int = 0;

system walk(mark: write Mark) {
    walks += 1;
    if walks > 1 {
        break;
    }
    mark.seen = 1;
}

system skip(mark: read Mark, tally: write Tally) {
    if mark.seen == 0 {
        continue;
    }
    tally.hits += 1;
}
"#;

#[test]
fn a_system_breaks_and_continues_the_entities_it_walks() {
    let (hosts, _) = hosts();
    let module = compile("call.lampas", CALL, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world);
    scripts.update(&mut world);
    let mark = component(&scripts, "Mark");
    let tally = component(&scripts, "Tally");
    let seen = scripts
        .store()
        .entities(mark)
        .map(|entity| value(&scripts, entity, "Mark", "seen"))
        .collect::<Vec<_>>();
    assert_eq!(seen, [Value::Int(1), Value::Int(0), Value::Int(0)]);
    let hits = scripts
        .store()
        .entities(tally)
        .map(|entity| value(&scripts, entity, "Tally", "hits"))
        .collect::<Vec<_>>();
    assert_eq!(hits, [Value::Int(1), Value::Int(0), Value::Int(0)]);
    assert_eq!(scripts.globals()[0], Value::Int(2));
}

const COMMANDS: &str = r#"
component Timer {
    life: float = 1.0,
}

component Mark {
    on: bool = true,
}

boot {
    let marked = spawn();
    add(marked, Mark {});
}

system age(timer: write Timer) {
    timer.life -= delta();
    if timer.life <= 0.0 {
        despawn(self);
    }
}

system breed() {
    let born = spawn();
    add(born, Timer { life: 2.0 });
}

system clear(mark: read Mark) {
    remove(self, Mark);
}
"#;

#[test]
fn a_script_spawns_despawns_and_marks_the_entities_it_commands() {
    let (hosts, _) = hosts();
    let module = compile("commands.lampas", COMMANDS, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world);
    assert_eq!(scripts.store().len(component(&scripts, "Mark")), 1);
    assert_eq!(scripts.store().len(component(&scripts, "Timer")), 0);
    scripts.update(&mut world);
    assert_eq!(scripts.store().len(component(&scripts, "Mark")), 0);
    assert_eq!(scripts.store().len(component(&scripts, "Timer")), 1);
    let first = scripts
        .store()
        .entities(component(&scripts, "Timer"))
        .next()
        .expect("the breeding system spawns its timer");
    assert_eq!(value(&scripts, first, "Timer", "life"), Value::Float(2.0));
    scripts.update(&mut world);
    scripts.update(&mut world);
    assert!(
        !world.is_alive(first),
        "the aging system despawns its timer",
    );
    assert_eq!(scripts.store().len(component(&scripts, "Timer")), 2);
    assert_eq!(world.len(), 3);
}

const WRONG: &str = r#"
component Speed {
    value: float,
}

system move(speed: write Speed) {
    speed.value = 1;
}
"#;

#[test]
fn the_checker_places_the_type_a_script_holds_wrong() {
    let (hosts, _) = hosts();
    let error = compile("wrong.lampas", WRONG, &hosts).expect_err("an int holds no float place");
    assert_eq!(error.source(), "wrong.lampas");
    assert_eq!(error.line(), 7);
    assert_eq!(error.message(), "int stands where float belongs");
}

fn folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("dome_lampas_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the lampas scripts folder makes");
    for (file, text) in files {
        std::fs::write(root.join(file), text).expect("the lampas script writes");
    }
    root
}

const ANSWER: &str = r#"
fn margin() -> int {
    let factor = 3;
    let boost = 10;
    return boost - factor;
}
"#;

const TOTAL: &str = r#"
fn shifted() -> int {
    let step = 1;
    let spare = 2;
    return step + spare;
}

boot {
    log(text(margin() + shifted()));
}
"#;

#[test]
fn a_script_holds_one_numbering_across_the_files_it_splits() {
    let (hosts, messages) = hosts();
    let root = folder(
        "split",
        &[("answer.lampas", ANSWER), ("total.lampas", TOTAL)],
    );
    let module = compile_folder(&root, &hosts).expect("the split script checks");
    let _ = std::fs::remove_dir_all(&root);
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world);
    assert_eq!(messages.borrow().as_slice(), ["10"]);
}
