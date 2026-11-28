use crate::Gravity;
use crate::Joint;
use crate::assembly;
use crate::body::Body;
use crate::cache::{self, ContactCache};
use crate::contact::{self, Manifold};
use crate::event::Collisions;
use crate::filter::CollisionFilter;
use crate::island::Islands;
use crate::joint_solver::{self, JointConstraint};
use crate::part::Part;
use crate::solid::Solid;
use crate::solver::{self, Contact};
use crate::table::Table;
use dome_ecs::{Entity, World};
use dome_math::{Aabb, Quat, Vec3};

pub fn step(world: &mut World, delta: f32) {
    assert!(
        delta.is_finite() && delta > 0.0,
        "a physics step of {delta} spans no time",
    );
    let gravity = world.resource::<Gravity>().copied().unwrap_or_default().0;
    assert!(
        gravity.x.is_finite() && gravity.y.is_finite() && gravity.z.is_finite(),
        "a physics step falls under gravity {gravity:?}",
    );
    let mut collisions = world.remove_resource::<Collisions>().unwrap_or_default();
    let mut simulation = world.remove_resource::<Simulation>().unwrap_or_default();
    simulation.run(world, gravity, delta, &mut collisions);
    world.insert_resource(simulation);
    world.insert_resource(collisions);
}

#[derive(Default)]
pub(crate) struct Simulation {
    table: Table,
    cache: ContactCache,
    contacts: Vec<Contact>,
    manifolds: Vec<Manifold>,
    touched: Vec<(Entity, Entity, bool)>,
    kept: Vec<(Entity, Entity, bool)>,
    jointed: Vec<(Entity, Entity)>,
    pairs: Vec<(usize, usize)>,
    motors: Vec<bool>,
    driven: Vec<bool>,
    constraints: Vec<JointConstraint>,
    joints: Vec<(Entity, Joint)>,
    links: Vec<(usize, usize)>,
    wakes: Vec<usize>,
    adjacency: Vec<Vec<usize>>,
    pending: Vec<usize>,
    reaches: Vec<Reach>,
    islands: Islands,
}

struct Reach {
    solid: Solid,
    aabb: Aabb,
    filter: CollisionFilter,
    sensor: bool,
}

impl Reach {
    fn of(part: &Part, position: Vec3, rotation: Quat, margin: f32) -> Self {
        let aabb = part.aabb(position, rotation);
        let margin = Vec3::splat(margin);
        Self {
            solid: part.solid(position, rotation),
            aabb: Aabb::new(aabb.min - margin, aabb.max + margin),
            filter: part.collider.filter(),
            sensor: part.collider.is_sensor(),
        }
    }

    fn reaches(&self, other: &Self) -> bool {
        self.aabb.intersects(other.aabb)
    }
}

impl Simulation {
    fn run(&mut self, world: &mut World, gravity: Vec3, delta: f32, collisions: &mut Collisions) {
        let joints = assembly::joints(world);
        self.table.sync(world, &joints, delta);
        if self.table.bodies().is_empty() {
            collisions.settle(&[]);
            return;
        }
        for entity in self.table.teleported() {
            self.cache.forget(*entity);
        }
        for body in self.table.bodies_mut() {
            if body.carried.moves() {
                body.wake();
            }
        }
        self.pairs.clear();
        self.motors.clear();
        for (entity, joint) in &joints {
            let first = self.locate(*entity);
            let second = self.locate(joint.other());
            self.pairs.push((first, second));
            self.motors.push(joint.driven());
        }
        assembly::drive(
            self.table.bodies_mut(),
            &self.pairs,
            &self.motors,
            &mut self.driven,
        );
        assembly::wake(
            self.table.bodies_mut(),
            &self.pairs,
            &mut self.adjacency,
            &mut self.pending,
        );
        for body in self.table.bodies_mut() {
            body.integrate_velocities(gravity, delta);
        }
        self.table.advance(delta);
        self.table.pairs();
        self.jointed.clear();
        for (entity, joint) in &joints {
            if !joint.collide() {
                self.jointed.push(cache::key(*entity, joint.other()));
            }
        }
        self.jointed.sort_unstable();
        let (bodies, broad) = self.table.split();
        collide(
            bodies,
            broad,
            &self.jointed,
            &mut self.manifolds,
            &mut self.contacts,
            &mut self.cache,
            &mut self.touched,
            &mut self.wakes,
            &mut self.reaches,
            delta,
        );
        for at in self.wakes.drain(..) {
            self.table.bodies_mut()[at].wake();
        }
        self.kept.clear();
        for index in 0..collisions.touching().len() {
            let (first, second, sensor) = collisions.touching()[index];
            let (Some(one), Some(two)) = (self.table.index(first), self.table.index(second)) else {
                continue;
            };
            let bodies = self.table.bodies();
            if bodies[one].moving()
                || bodies[one].senses()
                || bodies[two].moving()
                || bodies[two].senses()
            {
                continue;
            }
            self.kept.push((first, second, sensor));
        }
        self.touched.append(&mut self.kept);
        self.touched
            .sort_unstable_by_key(|(first, second, _)| (*first, *second));
        self.touched
            .dedup_by_key(|(first, second, _)| (*first, *second));
        assembly::wake(
            self.table.bodies_mut(),
            &self.pairs,
            &mut self.adjacency,
            &mut self.pending,
        );
        let bodies = self.table.bodies();
        self.contacts.sort_by_key(|held| {
            cache::contact_key(
                bodies[held.first].entity,
                bodies[held.second].entity,
                bodies[held.first].parts[held.first_part].entity,
                bodies[held.second].parts[held.second_part].entity,
                0,
            )
        });
        self.constraints.clear();
        self.joints.clear();
        for (entity, joint) in &joints {
            let first = self.locate(*entity);
            let second = self.locate(joint.other());
            let bodies = self.table.bodies();
            if bodies[first].moving() || bodies[second].moving() {
                self.constraints
                    .push(joint_solver::prepare(first, second, *joint, bodies, delta));
                self.joints.push((*entity, *joint));
            }
        }
        solver::prepare(self.table.bodies(), &mut self.contacts, delta);
        solver::warm_start(self.table.bodies_mut(), &self.contacts);
        joint_solver::warm_start(self.table.bodies_mut(), &self.constraints);
        for _ in 0..solver::VELOCITY_ITERATIONS {
            solver::solve_velocity(self.table.bodies_mut(), &mut self.contacts);
            joint_solver::solve_velocity(self.table.bodies_mut(), &mut self.constraints);
        }
        for body in self.table.bodies_mut() {
            body.integrate_position(delta);
        }
        for _ in 0..solver::POSITION_ITERATIONS {
            solver::solve_position(self.table.bodies_mut(), &self.contacts);
            joint_solver::settle(self.table.bodies_mut(), &mut self.constraints);
        }
        self.links.clear();
        self.links.extend(
            self.contacts
                .iter()
                .filter(|held| held.touching())
                .map(|held| (held.first, held.second)),
        );
        self.links.extend_from_slice(&self.pairs);
        self.islands
            .settle(self.table.bodies_mut(), &self.links, &self.driven, delta);
        self.cache.clear();
        let bodies = self.table.bodies();
        for at in 0..self.contacts.len() {
            let held = &self.contacts[at];
            self.cache.insert(
                bodies[held.first].entity,
                bodies[held.second].entity,
                bodies[held.first].parts[held.first_part].entity,
                bodies[held.second].parts[held.second_part].entity,
                solver::cache(held),
            );
        }
        collisions.settle(&self.touched);
        for at in 0..self.constraints.len() {
            let (entity, mut joint) = self.joints[at];
            joint.set_cache(self.constraints[at].cache());
            world.insert(entity, joint);
        }
        for body in self.table.bodies_mut() {
            body.sync(world);
        }
    }

    fn locate(&self, entity: Entity) -> usize {
        self.table.index(entity).unwrap_or_else(|| {
            panic!(
                "the joint names the body of entity {}, which the table holds none",
                entity.index(),
            )
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn collide(
    bodies: &[Body],
    pairs: &[(u32, u32)],
    jointed: &[(Entity, Entity)],
    manifolds: &mut Vec<Manifold>,
    contacts: &mut Vec<Contact>,
    cache: &mut ContactCache,
    touched: &mut Vec<(Entity, Entity, bool)>,
    wakes: &mut Vec<usize>,
    reaches: &mut Vec<Reach>,
    delta: f32,
) {
    contacts.clear();
    touched.clear();
    wakes.clear();
    reaches.clear();
    for pair in pairs {
        let (first, second) = (pair.0 as usize, pair.1 as usize);
        let key = cache::key(bodies[first].entity, bodies[second].entity);
        if jointed.binary_search(&key).is_ok() {
            continue;
        }
        if !bodies[first].pushable()
            && !bodies[second].pushable()
            && !bodies[first].senses()
            && !bodies[second].senses()
        {
            continue;
        }
        let movable = bodies[first].movable() || bodies[second].movable();
        let gap = (bodies[first].travel.linear - bodies[second].travel.linear).length()
            + bodies[first].travel.angular
            + bodies[second].travel.angular
            + contact::CONTACT_GAP;
        reaches.clear();
        for part in &bodies[second].parts {
            reaches.push(Reach::of(
                part,
                bodies[second].position,
                bodies[second].rotation,
                gap,
            ));
        }
        let mut sensor: Option<bool> = None;
        let mut solid = false;
        for (first_part, part) in bodies[first].parts.iter().enumerate() {
            let held = Reach::of(part, bodies[first].position, bodies[first].rotation, gap);
            for (second_part, other) in reaches.iter().enumerate() {
                if !held.filter.accepts(other.filter) || !held.reaches(other) {
                    continue;
                }
                if held.sensor || other.sensor {
                    if movable
                        && triggered(
                            bodies,
                            first,
                            second,
                            &held.solid,
                            &other.solid,
                            gap,
                            manifolds,
                            delta,
                        )
                    {
                        sensor = Some(sensor.unwrap_or(true));
                    }
                    continue;
                }
                manifolds.clear();
                contact::collide(&held.solid, &other.solid, gap, manifolds);
                if manifolds.is_empty() {
                    continue;
                }
                if manifolds.iter().any(|manifold| {
                    manifold
                        .points
                        .iter()
                        .any(|point| point.separation <= solver::LINEAR_SLOP)
                }) {
                    sensor = Some(false);
                    solid = true;
                }
                for manifold in manifolds.drain(..) {
                    contacts.push(solver::contact(
                        first,
                        second,
                        first_part,
                        second_part,
                        manifold,
                        bodies,
                        cache,
                    ));
                }
            }
        }
        let Some(sensor) = sensor else {
            continue;
        };
        touched.push((key.0, key.1, sensor));
        if !solid || (!bodies[first].active() && !bodies[second].active()) {
            continue;
        }
        if bodies[first].sleeping() && bodies[second].active() {
            wakes.push(first);
        }
        if bodies[second].sleeping() && bodies[first].active() {
            wakes.push(second);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn triggered(
    bodies: &[Body],
    first: usize,
    second: usize,
    held_solid: &Solid,
    other_solid: &Solid,
    gap: f32,
    manifolds: &mut Vec<Manifold>,
    delta: f32,
) -> bool {
    if contact::intersects(held_solid, other_solid) {
        return true;
    }
    manifolds.clear();
    contact::collide(held_solid, other_solid, gap, manifolds);
    manifolds.iter().any(|manifold| {
        manifold.points.iter().any(|point| {
            if point.separation <= 0.0 {
                return true;
            }
            let relative = bodies[second].point_velocity(point.position - bodies[second].position)
                - bodies[first].point_velocity(point.position - bodies[first].position);
            -relative.dot(manifold.normal) * delta > point.separation - solver::LINEAR_SLOP
        })
    })
}
