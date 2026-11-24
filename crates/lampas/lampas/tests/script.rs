use dome_ecs::{Entity, World};
use lampas::{Hosts, Scripts, Type, Value, compile, compile_folder};
use lampas_bytecode::Op;
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

fn value(
    world: &World,
    scripts: &Scripts,
    entity: Entity,
    component_name: &str,
    name: &str,
) -> Value {
    let held = world
        .dynamic()
        .read_entity(
            entity,
            component(scripts, component_name),
            field(scripts, component_name, name),
        )
        .unwrap_or_else(|| panic!("the entity {} holds no {component_name}", entity.index()));
    Value::from(held)
}

fn fault_at(scripts: &Scripts, function: &str, op: Op) -> usize {
    scripts
        .module()
        .functions
        .iter()
        .find(|held| held.name == function)
        .and_then(|held| {
            held.code
                .iter()
                .position(|instruction| instruction.op == op)
        })
        .unwrap_or_else(|| panic!("the {function} function holds no {op:?} instruction"))
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
    let module = compile("walk.amp", WALK, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world).expect("the script boots");
    let entity = world
        .dynamic()
        .entities(component(&scripts, "Motion"))
        .next()
        .expect("the boot spawns the entity its systems walk");
    assert_eq!(
        value(&world, &scripts, entity, "Motion", "distance"),
        Value::Float(0.0)
    );
    assert_eq!(
        value(&world, &scripts, entity, "Health", "value"),
        Value::Float(10.0)
    );
    for _ in 0..2 {
        scripts.fixed(&mut world).expect("the script fixes");
    }
    for _ in 0..4 {
        scripts.update(&mut world).expect("the script updates");
    }
    assert_eq!(
        value(&world, &scripts, entity, "Motion", "speed"),
        Value::Float(0.0)
    );
    assert_eq!(
        value(&world, &scripts, entity, "Motion", "distance"),
        Value::Float(6.0)
    );
    assert_eq!(
        value(&world, &scripts, entity, "Health", "value"),
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
    let module = compile("call.amp", CALL, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world).expect("the script boots");
    scripts.update(&mut world).expect("the script updates");
    let mark = component(&scripts, "Mark");
    let tally = component(&scripts, "Tally");
    let seen = world
        .dynamic()
        .entities(mark)
        .map(|entity| value(&world, &scripts, entity, "Mark", "seen"))
        .collect::<Vec<_>>();
    assert_eq!(seen, [Value::Int(1), Value::Int(0), Value::Int(0)]);
    let hits = world
        .dynamic()
        .entities(tally)
        .map(|entity| value(&world, &scripts, entity, "Tally", "hits"))
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
    let module = compile("commands.amp", COMMANDS, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world).expect("the script boots");
    assert_eq!(world.dynamic().column(component(&scripts, "Mark")).len(), 1);
    assert_eq!(
        world.dynamic().column(component(&scripts, "Timer")).len(),
        0
    );
    scripts.update(&mut world).expect("the script updates");
    assert_eq!(world.dynamic().column(component(&scripts, "Mark")).len(), 0);
    assert_eq!(
        world.dynamic().column(component(&scripts, "Timer")).len(),
        1
    );
    let first = world
        .dynamic()
        .entities(component(&scripts, "Timer"))
        .next()
        .expect("the breeding system spawns its timer");
    assert_eq!(
        value(&world, &scripts, first, "Timer", "life"),
        Value::Float(2.0)
    );
    scripts.update(&mut world).expect("the script updates");
    scripts.update(&mut world).expect("the script updates");
    assert!(
        !world.is_alive(first),
        "the aging system despawns its timer",
    );
    assert_eq!(
        world.dynamic().column(component(&scripts, "Timer")).len(),
        2
    );
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
    let error = compile("wrong.amp", WRONG, &hosts).expect_err("an int holds no float place");
    assert_eq!(error.source(), "wrong.amp");
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
    let root = folder("split", &[("answer.amp", ANSWER), ("total.amp", TOTAL)]);
    let module = compile_folder(&root, &hosts).expect("the split script checks");
    let _ = std::fs::remove_dir_all(&root);
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world).expect("the script boots");
    assert_eq!(messages.borrow().as_slice(), ["10"]);
}

const DIVIDE: &str = r#"
component Mark {
    on: bool = true,
}

fn divide(value: int) -> int {
    return 10 / value;
}

boot {
    let first = spawn();
    add(first, Mark {});
    log(text(divide(0)));
}
"#;

#[test]
fn a_boot_fault_returns_the_trap_its_function_holds() {
    let (hosts, messages) = hosts();
    let module = compile("divide.amp", DIVIDE, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    let at = fault_at(&scripts, "divide", Op::DivideInt);
    let trap = scripts.boot(&mut world).expect_err("the divide faults");
    assert_eq!(trap.function(), "divide");
    assert_eq!(trap.instruction(), at);
    assert_eq!(trap.message(), "the int divides by zero");
    assert_eq!(
        trap.to_string(),
        format!("the lampas function divide fails at instruction {at}: the int divides by zero"),
    );
    assert!(messages.borrow().is_empty());
    assert_eq!(world.dynamic().column(component(&scripts, "Mark")).len(), 0);
    assert_eq!(
        scripts
            .boot(&mut world)
            .expect_err("the machine runs again after a fault"),
        trap,
    );
}

const FAULT: &str = r#"
component Mark {
    on: bool = true,
}

system mark() {
    let first = spawn();
    add(first, Mark {});
}

system explode() {
    let second = spawn();
    add(second, Mark {});
    let items = [1, 2];
    log(text(items[5]));
}
"#;

#[test]
fn a_system_fault_discards_the_commands_its_run_recorded() {
    let (hosts, messages) = hosts();
    let module = compile("fault.amp", FAULT, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts.boot(&mut world).expect("the script boots");
    let at = fault_at(&scripts, "explode", Op::ListGet);
    let trap = scripts
        .update(&mut world)
        .expect_err("the list read faults");
    assert_eq!(trap.function(), "explode");
    assert_eq!(trap.instruction(), at);
    assert_eq!(trap.message(), "the list holds no item 5");
    assert_eq!(
        trap.to_string(),
        format!("the lampas function explode fails at instruction {at}: the list holds no item 5"),
    );
    let mark = component(&scripts, "Mark");
    assert_eq!(
        world.dynamic().column(mark).len(),
        1,
        "the fault discards the commands of its run",
    );
    assert!(messages.borrow().is_empty());
    assert_eq!(
        scripts
            .update(&mut world)
            .expect_err("the machine runs the next frame after a fault"),
        trap,
    );
}

const RECURSE: &str = r#"
fn recurse(depth: int) -> int {
    return recurse(depth + 1);
}

boot {
    log(text(recurse(0)));
}
"#;

#[test]
fn a_deep_call_returns_the_trap_the_call_stack_names() {
    let (hosts, messages) = hosts();
    let module = compile("recurse.amp", RECURSE, &hosts).expect("the script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    let at = fault_at(&scripts, "recurse", Op::Call);
    let trap = scripts
        .boot(&mut world)
        .expect_err("the call stack overflows");
    assert_eq!(trap.function(), "recurse");
    assert_eq!(trap.instruction(), at);
    assert_eq!(trap.message(), "the lampas call stack overflows");
    assert!(messages.borrow().is_empty());
}

const SPAWN: &str = r#"
boot {
    let first = spawn();
    let second = spawn();
}
"#;

#[test]
fn a_reset_reclaims_the_entities_its_scripts_spawned() {
    let (hosts, _messages) = hosts();
    let module = compile("spawn.amp", SPAWN, &hosts).expect("the spawning script checks");
    let mut scripts = Scripts::new(module, hosts);
    let mut world = World::new();
    scripts
        .boot(&mut world)
        .expect("the boot spawns its entities");
    assert_eq!(world.len(), 2, "a boot spawns the entities it names");
    lampas::reset(&mut world);
    assert_eq!(
        world.len(),
        0,
        "a reset reclaims the entities its scripts spawned",
    );
    scripts
        .boot(&mut world)
        .expect("the boot spawns its entities again");
    assert_eq!(
        world.len(),
        2,
        "a boot after a reset spawns no stale entity"
    );
}
