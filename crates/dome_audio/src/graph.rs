use crate::bus::{AudioBus, AudioCompressor, AudioFilter, AudioReverb};
use dome_ecs::{Entity, World};
use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub(crate) const MAX_BUSES: usize = 32;
pub(crate) const BLOCK_FRAMES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BusParams {
    pub(crate) gain_db: f32,
    pub(crate) mute: bool,
    pub(crate) send_db: f32,
    pub(crate) filter: AudioFilter,
    pub(crate) compressor: AudioCompressor,
    pub(crate) reverb: AudioReverb,
}

impl Default for BusParams {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            mute: false,
            send_db: 0.0,
            filter: AudioFilter::bypassed(),
            compressor: AudioCompressor::bypassed(),
            reverb: AudioReverb::bypassed(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BusNode {
    pub(crate) id: Arc<str>,
    parent: Option<usize>,
    send: Option<usize>,
}

impl BusNode {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn parent(&self) -> Option<usize> {
        self.parent
    }

    pub(crate) fn send(&self) -> Option<usize> {
        self.send
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layout {
    buses: Vec<BusNode>,
    order: Vec<usize>,
    root: usize,
    synthetic: bool,
}

impl Layout {
    pub(crate) fn master() -> Self {
        Self {
            buses: vec![BusNode {
                id: Arc::from(""),
                parent: None,
                send: None,
            }],
            order: vec![0],
            root: 0,
            synthetic: true,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.buses.len()
    }

    pub(crate) fn buses(&self) -> &[BusNode] {
        &self.buses
    }

    pub(crate) fn order(&self) -> &[usize] {
        &self.order
    }

    pub(crate) fn root(&self) -> usize {
        self.root
    }

    pub(crate) fn synthetic(&self) -> bool {
        self.synthetic
    }

    pub(crate) fn index(&self, id: &str) -> Option<usize> {
        self.buses.iter().position(|bus| *bus.id == *id)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BusLink<'a> {
    pub id: &'a str,
    pub parent: Option<&'a str>,
    pub send: Option<&'a str>,
}

struct Topology {
    parents: Vec<Option<usize>>,
    sends: Vec<Option<usize>>,
    root: usize,
    order: Vec<usize>,
}

pub fn validate_buses(links: &[BusLink<'_>]) -> Result<(), String> {
    if links.is_empty() {
        return Ok(());
    }
    topology(links).map(|_| ())
}

pub(crate) fn compile(world: &World) -> (Layout, Vec<Entity>) {
    let mut held = world
        .iter::<AudioBus>()
        .map(|(entity, bus)| (entity, bus.clone()))
        .collect::<Vec<_>>();
    if held.is_empty() {
        return (Layout::master(), Vec::new());
    }
    assert!(
        held.len() <= MAX_BUSES,
        "a scene holds {} audio buses where the mixer holds {MAX_BUSES}",
        held.len(),
    );
    held.sort_by(|left, right| left.1.id().cmp(right.1.id()));
    let links = held
        .iter()
        .map(|(_, bus)| BusLink {
            id: bus.id(),
            parent: bus.parent(),
            send: bus.send(),
        })
        .collect::<Vec<_>>();
    let layout = layout_of(&links).unwrap_or_else(|error| panic!("{error}"));
    let entities = held.iter().map(|(entity, _)| *entity).collect();
    (layout, entities)
}

pub(crate) fn layout_of(links: &[BusLink<'_>]) -> Result<Layout, String> {
    let topology = topology(links)?;
    let buses = links
        .iter()
        .enumerate()
        .map(|(index, link)| BusNode {
            id: Arc::from(link.id),
            parent: topology.parents[index],
            send: topology.sends[index],
        })
        .collect();
    Ok(Layout {
        buses,
        order: topology.order,
        root: topology.root,
        synthetic: false,
    })
}

pub(crate) fn fingerprint(world: &World) -> u64 {
    let mut links = world
        .iter::<AudioBus>()
        .map(|(_, bus)| BusLink {
            id: bus.id(),
            parent: bus.parent(),
            send: bus.send(),
        })
        .collect::<Vec<_>>();
    if links.is_empty() {
        return 0;
    }
    links.sort_by(|left, right| left.id.cmp(right.id));
    let mut hasher = DefaultHasher::new();
    for link in &links {
        link.id.hash(&mut hasher);
        link.parent.hash(&mut hasher);
        link.send.hash(&mut hasher);
    }
    hasher.finish()
}

pub(crate) fn entities(world: &World, layout: &Layout, out: &mut Vec<Entity>) {
    out.clear();
    for bus in layout.buses() {
        let entity = world
            .iter::<AudioBus>()
            .find(|(_, held)| held.id() == bus.id())
            .map(|(entity, _)| entity)
            .unwrap_or_else(|| {
                panic!(
                    "the audio layout holds the bus {}, which no entity holds",
                    bus.id(),
                )
            });
        out.push(entity);
    }
}

pub(crate) fn params(
    world: &World,
    layout: &Layout,
    entities: &[Entity],
    out: &mut Vec<BusParams>,
) {
    out.clear();
    if layout.synthetic() {
        out.push(BusParams::default());
        return;
    }
    assert_eq!(
        entities.len(),
        layout.len(),
        "the audio layout holds {} buses for {} entities",
        layout.len(),
        entities.len(),
    );
    for entity in entities {
        let bus = world.get::<AudioBus>(*entity).unwrap_or_else(|| {
            panic!(
                "the audio layout names the bus of entity {}, which holds none",
                entity.index(),
            )
        });
        out.push(BusParams {
            gain_db: bus.gain_db(),
            mute: bus.mute(),
            send_db: bus.send_db(),
            filter: world
                .get::<AudioFilter>(*entity)
                .copied()
                .unwrap_or_else(AudioFilter::bypassed),
            compressor: world
                .get::<AudioCompressor>(*entity)
                .copied()
                .unwrap_or_else(AudioCompressor::bypassed),
            reverb: world
                .get::<AudioReverb>(*entity)
                .copied()
                .unwrap_or_else(AudioReverb::bypassed),
        });
    }
}

fn topology(links: &[BusLink<'_>]) -> Result<Topology, String> {
    if links.len() > MAX_BUSES {
        return Err(format!(
            "the audio buses hold {} buses where the mixer holds {MAX_BUSES}",
            links.len(),
        ));
    }
    let mut parents = Vec::with_capacity(links.len());
    let mut sends = Vec::with_capacity(links.len());
    for (index, link) in links.iter().enumerate() {
        if links[..index].iter().any(|held| held.id == link.id) {
            return Err(format!("the audio buses hold {} twice", link.id));
        }
        if link.id.is_empty() {
            return Err(String::from("an audio bus holds no name"));
        }
        parents.push(resolve(links, link.parent, "parent", link.id)?);
        sends.push(resolve(links, link.send, "send", link.id)?);
    }
    let roots = parents.iter().filter(|parent| parent.is_none()).count();
    if roots != 1 {
        return Err(format!(
            "the audio buses hold {roots} roots where one bus holds none",
        ));
    }
    let root = parents
        .iter()
        .position(Option::is_none)
        .expect("one audio bus holds no parent");
    let mut incoming = vec![0u32; links.len()];
    for (parent, send) in parents.iter().zip(&sends) {
        for target in [*parent, *send].into_iter().flatten() {
            incoming[target] += 1;
        }
    }
    let mut ready = incoming
        .iter()
        .enumerate()
        .filter(|(_, count)| **count == 0)
        .map(|(index, _)| index)
        .collect::<VecDeque<_>>();
    let mut order = Vec::with_capacity(links.len());
    while let Some(index) = ready.pop_front() {
        order.push(index);
        for target in [parents[index], sends[index]].into_iter().flatten() {
            incoming[target] -= 1;
            if incoming[target] == 0 {
                ready.push_back(target);
            }
        }
    }
    if order.len() != links.len() {
        return Err(String::from("the audio buses close a cycle"));
    }
    Ok(Topology {
        parents,
        sends,
        root,
        order,
    })
}

fn resolve(
    links: &[BusLink<'_>],
    name: Option<&str>,
    kind: &str,
    id: &str,
) -> Result<Option<usize>, String> {
    let Some(name) = name else {
        return Ok(None);
    };
    links
        .iter()
        .position(|link| link.id == name)
        .map(Some)
        .ok_or_else(|| format!("the audio bus {id} {kind}s to {name}, which it does not hold"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bus_graph_holds_one_fingerprint_however_the_scene_orders_it() {
        let build = |order: [(&str, Option<&str>, Option<&str>); 3], lead: usize| {
            let mut world = World::new();
            for _ in 0..lead {
                world.spawn();
            }
            for (id, parent, send) in order {
                let entity = world.spawn();
                world.insert(
                    entity,
                    AudioBus::new(
                        id,
                        parent.map(String::from),
                        send.map(String::from),
                        0.0,
                        false,
                        0.0,
                    ),
                );
            }
            world
        };
        let first = build(
            [
                ("master", None, None),
                ("music", Some("master"), Some("hall")),
                ("hall", Some("master"), None),
            ],
            0,
        );
        let second = build(
            [
                ("hall", Some("master"), None),
                ("master", None, None),
                ("music", Some("master"), Some("hall")),
            ],
            5,
        );
        assert_eq!(fingerprint(&first), fingerprint(&second));
        assert_eq!(compile(&first).0, compile(&second).0);
    }

    #[test]
    fn a_bus_graph_orders_children_before_parents_and_sends() {
        let links = [
            BusLink {
                id: "master",
                parent: None,
                send: None,
            },
            BusLink {
                id: "music",
                parent: Some("master"),
                send: Some("hall"),
            },
            BusLink {
                id: "hall",
                parent: Some("master"),
                send: None,
            },
        ];
        let topology = topology(&links).expect("the graph holds one root");
        assert_eq!(topology.root, 0);
        assert_eq!(topology.order.len(), 3);
        let music = topology.order.iter().position(|at| *at == 1).unwrap();
        let hall = topology.order.iter().position(|at| *at == 2).unwrap();
        let master = topology.order.iter().position(|at| *at == 0).unwrap();
        assert!(music < hall);
        assert!(hall < master);
        assert!(validate_buses(&links).is_ok());
    }

    #[test]
    fn a_bus_graph_rejects_cycles_and_two_roots() {
        let cycle = [
            BusLink {
                id: "a",
                parent: Some("b"),
                send: None,
            },
            BusLink {
                id: "b",
                parent: Some("a"),
                send: None,
            },
        ];
        assert!(validate_buses(&cycle).is_err());
        let roots = [
            BusLink {
                id: "a",
                parent: None,
                send: None,
            },
            BusLink {
                id: "b",
                parent: None,
                send: None,
            },
        ];
        assert!(validate_buses(&roots).is_err());
    }
}
