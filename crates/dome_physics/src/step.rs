use crate::Gravity;
use crate::assembly;
use crate::broadphase;
use crate::cache::{self, ContactCache};
use crate::contact;
use crate::event::Collisions;
use crate::island;
use crate::joint_solver;
use crate::solver;
use dome_ecs::World;
use std::collections::{BTreeMap, BTreeSet};

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
    let mut held_cache = world.remove_resource::<ContactCache>().unwrap_or_default();
    let joints = assembly::joints(world);
    let mut bodies = assembly::gather(world, delta);
    if bodies.is_empty() {
        collisions.settle(BTreeMap::new());
        world.insert_resource(collisions);
        world.insert_resource(held_cache);
        return;
    }
    for body in &mut bodies {
        if body.carried.moves() {
            body.wake();
        }
    }
    let indices = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| (body.entity, index))
        .collect::<BTreeMap<_, _>>();
    let pairs = joints
        .iter()
        .map(|(entity, joint)| (indices[entity], indices[&joint.other()]))
        .collect::<Vec<_>>();
    let motors = joints
        .iter()
        .map(|(_, joint)| joint.driven())
        .collect::<Vec<_>>();
    let driven = assembly::drive(&mut bodies, &pairs, &motors);
    assembly::wake(&mut bodies, &pairs);
    for body in &mut bodies {
        body.integrate_velocities(gravity, delta);
    }
    let swept = bodies
        .iter()
        .map(|body| body.swept_aabb(delta))
        .collect::<Vec<_>>();
    let travels = swept
        .iter()
        .map(|held| held.map_or(crate::body::Travel::STILL, |(_, travel)| travel))
        .collect::<Vec<_>>();
    let boxes = swept
        .iter()
        .enumerate()
        .filter_map(|(index, held)| held.map(|(aabb, _)| (index, aabb)))
        .collect::<Vec<_>>();
    let bounds = boxes.iter().map(|(_, aabb)| *aabb).collect::<Vec<_>>();
    let jointed = joints
        .iter()
        .filter(|(_, joint)| !joint.collide())
        .map(|(entity, joint)| cache::key(*entity, joint.other()))
        .collect::<BTreeSet<_>>();
    let mut touched = BTreeMap::new();
    let mut contacts = Vec::new();
    let mut manifolds = Vec::new();
    for (low, high) in broadphase::pairs(&bounds) {
        let first = boxes[low].0;
        let second = boxes[high].0;
        if jointed.contains(&cache::key(bodies[first].entity, bodies[second].entity)) {
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
        let gap = (travels[first].linear - travels[second].linear).length()
            + travels[first].angular
            + travels[second].angular
            + contact::CONTACT_GAP;
        let mut touched_pair: Option<bool> = None;
        let mut solid = false;
        for (first_part, held) in bodies[first].parts.iter().enumerate() {
            let held_solid = held.solid(bodies[first].position, bodies[first].rotation);
            for (second_part, other) in bodies[second].parts.iter().enumerate() {
                if !held.collider.filter().accepts(other.collider.filter()) {
                    continue;
                }
                let other_solid = other.solid(bodies[second].position, bodies[second].rotation);
                if held.collider.is_sensor() || other.collider.is_sensor() {
                    if movable {
                        let triggered = contact::intersects(&held_solid, &other_solid) || {
                            manifolds.clear();
                            contact::collide(&held_solid, &other_solid, gap, &mut manifolds);
                            manifolds.iter().any(|manifold| {
                                manifold.points.iter().any(|point| {
                                    if point.separation <= 0.0 {
                                        return true;
                                    }
                                    let relative = bodies[second]
                                        .point_velocity(point.position - bodies[second].position)
                                        - bodies[first].point_velocity(
                                            point.position - bodies[first].position,
                                        );
                                    -relative.dot(manifold.normal) * delta
                                        > point.separation - solver::LINEAR_SLOP
                                })
                            })
                        };
                        if triggered {
                            touched_pair = Some(touched_pair.unwrap_or(true));
                        }
                    }
                    continue;
                }
                manifolds.clear();
                contact::collide(&held_solid, &other_solid, gap, &mut manifolds);
                if manifolds.is_empty() {
                    continue;
                }
                if manifolds.iter().any(|manifold| {
                    manifold
                        .points
                        .iter()
                        .any(|point| point.separation <= solver::LINEAR_SLOP)
                }) {
                    touched_pair = Some(false);
                    solid = true;
                }
                for manifold in manifolds.drain(..) {
                    contacts.push(solver::contact(
                        first,
                        second,
                        first_part,
                        second_part,
                        manifold,
                        &bodies,
                        &mut held_cache,
                    ));
                }
            }
        }
        let Some(sensor) = touched_pair else {
            continue;
        };
        touched
            .entry(cache::key(bodies[first].entity, bodies[second].entity))
            .and_modify(|held| *held &= sensor)
            .or_insert(sensor);
        if !solid {
            continue;
        }
        if !bodies[first].active() && !bodies[second].active() {
            continue;
        }
        if bodies[first].sleeping() && bodies[second].active() {
            bodies[first].wake();
        }
        if bodies[second].sleeping() && bodies[first].active() {
            bodies[second].wake();
        }
    }
    for (first, second) in held_cache.pairs() {
        if touched.contains_key(&cache::key(first, second)) {
            continue;
        }
        for entity in [first, second] {
            if let Some(index) = indices.get(&entity)
                && bodies[*index].sleeping()
            {
                bodies[*index].wake();
            }
        }
    }
    assembly::wake(&mut bodies, &pairs);
    contacts.sort_by_key(|contact| {
        cache::contact_key(
            bodies[contact.first].entity,
            bodies[contact.second].entity,
            contact.first_part,
            contact.second_part,
            0,
        )
    });
    let active = joints
        .iter()
        .filter_map(|(entity, joint)| {
            let first = indices[entity];
            let second = indices[&joint.other()];
            (bodies[first].active() || bodies[second].active())
                .then_some((*entity, *joint, first, second))
        })
        .collect::<Vec<_>>();
    let mut constraints = active
        .iter()
        .map(|(_, joint, first, second)| {
            joint_solver::prepare(*first, *second, *joint, &bodies, delta)
        })
        .collect::<Vec<_>>();
    solver::prepare(&bodies, &mut contacts, delta);
    solver::warm_start(&mut bodies, &contacts);
    joint_solver::warm_start(&mut bodies, &constraints);
    for _ in 0..solver::VELOCITY_ITERATIONS {
        solver::solve_velocity(&mut bodies, &mut contacts);
        joint_solver::solve_velocity(&mut bodies, &mut constraints);
    }
    for body in &mut bodies {
        body.integrate_position(delta);
    }
    for _ in 0..solver::POSITION_ITERATIONS {
        solver::solve_position(&mut bodies, &contacts);
        joint_solver::settle(&mut bodies, &mut constraints);
    }
    let mut links = contacts
        .iter()
        .map(|contact| (contact.first, contact.second))
        .collect::<Vec<_>>();
    links.extend_from_slice(&pairs);
    island::settle(&mut bodies, &links, &driven, delta);
    let mut cache = ContactCache::default();
    for contact in &contacts {
        cache.insert(
            bodies[contact.first].entity,
            bodies[contact.second].entity,
            contact.first_part,
            contact.second_part,
            solver::cache(contact),
        );
    }
    world.insert_resource(cache);
    collisions.settle(touched);
    world.insert_resource(collisions);
    for ((entity, mut joint, _, _), constraint) in active.into_iter().zip(constraints) {
        joint.set_cache(constraint.cache());
        world.insert(entity, joint);
    }
    for body in &bodies {
        body.sync(world);
    }
}
