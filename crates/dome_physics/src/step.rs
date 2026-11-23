use crate::Gravity;
use crate::Joint;
use crate::assembly;
use crate::body::Body;
use crate::cache::{self, ContactCache};
use crate::contact::{self, Manifold};
use crate::event::Collisions;
use crate::island;
use crate::joint_solver::{self, JointConstraint};
use crate::solid::Solid;
use crate::solver::{self, Contact};
use crate::table::Table;
use dome_ecs::{Entity, World};
use dome_math::Vec3;

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
}

impl Simulation {
    fn run(&mut self, world: &mut World, gravity: Vec3, delta: f32, collisions: &mut Collisions) {
        let joints = assembly::joints(world);
        self.table.sync(world, &joints, delta);
        if self.table.bodies().is_empty() {
            collisions.settle(&[]);
            return;
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
            delta,
        );
        for at in self.wakes.drain(..) {
            self.table.bodies_mut()[at].wake();
        }
        self.touched
            .sort_unstable_by_key(|(first, second, _)| (*first, *second));
        for index in 0..collisions.touching().len() {
            let (first, second, sensor) = collisions.touching()[index];
            if self
                .touched
                .binary_search_by_key(&(first, second), |(a, b, _)| (*a, *b))
                .is_ok()
            {
                continue;
            }
            let (Some(one), Some(two)) = (self.table.index(first), self.table.index(second)) else {
                continue;
            };
            let bodies = self.table.bodies();
            if let (Some(one), Some(two)) = (bodies[one].bounds, bodies[two].bounds)
                && one.intersects(two)
            {
                self.touched.push((first, second, sensor));
            }
        }
        self.touched
            .sort_unstable_by_key(|(first, second, _)| (*first, *second));
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
                held.first_part,
                held.second_part,
                0,
            )
        });
        self.constraints.clear();
        self.joints.clear();
        for (entity, joint) in &joints {
            let first = self.locate(*entity);
            let second = self.locate(joint.other());
            let bodies = self.table.bodies();
            if bodies[first].active() || bodies[second].active() {
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
        self.links
            .extend(self.contacts.iter().map(|held| (held.first, held.second)));
        self.links.extend_from_slice(&self.pairs);
        island::settle(self.table.bodies_mut(), &self.links, &self.driven, delta);
        self.cache.clear();
        for at in 0..self.contacts.len() {
            let held = &self.contacts[at];
            let first = self.table.bodies()[held.first].entity;
            let second = self.table.bodies()[held.second].entity;
            self.cache.insert(
                first,
                second,
                held.first_part,
                held.second_part,
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
    delta: f32,
) {
    contacts.clear();
    touched.clear();
    wakes.clear();
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
        let mut sensor: Option<bool> = None;
        let mut solid = false;
        for (first_part, held) in bodies[first].parts.iter().enumerate() {
            let held_solid = held.solid(bodies[first].position, bodies[first].rotation);
            for (second_part, other) in bodies[second].parts.iter().enumerate() {
                if !held.collider.filter().accepts(other.collider.filter()) {
                    continue;
                }
                let other_solid = other.solid(bodies[second].position, bodies[second].rotation);
                if held.collider.is_sensor() || other.collider.is_sensor() {
                    if movable
                        && triggered(
                            bodies,
                            first,
                            second,
                            &held_solid,
                            &other_solid,
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
                contact::collide(&held_solid, &other_solid, gap, manifolds);
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
