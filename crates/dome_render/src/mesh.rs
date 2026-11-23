use crate::abi::{SKIN_VERTEX, SkinVertexRecord, VERTEX_FLOATS, Vertex};
use crate::span;
use dome_gpu::{BufferUsages, Device, GpuBuffer, Queue};
use dome_math::{Aabb, Vec3};
use dome_shader::{Fvec4, Uvec4};
use std::mem::size_of;

const VERTEX_BYTES: u64 = VERTEX_FLOATS as u64 * 4;
const SKIN_BYTES: u64 = size_of::<SkinVertexRecord>() as u64;
const INITIAL_BLOCKS: u32 = 4;
const SKIN_BLOCKS: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mesh {
    index: u32,
    generation: u32,
}

impl Mesh {
    pub(crate) fn index(self) -> u32 {
        self.index
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    pub first: u32,
    pub count: u32,
    pub skin_first: u32,
    pub skin_count: u32,
    pub center: Vec3,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug)]
struct Block {
    first: u32,
    blocks: u32,
}

struct Arena {
    blocks: u32,
    free: Vec<Block>,
}

impl Arena {
    fn new(blocks: u32) -> Self {
        Self {
            blocks,
            free: Vec::new(),
        }
    }
    fn claim(&mut self, blocks: u32) -> Block {
        assert!(blocks > 0, "a claimed span holds no record");
        if let Some(at) = self.free.iter().position(|held| held.blocks >= blocks) {
            let held = self.free.swap_remove(at);
            if held.blocks == blocks {
                return held;
            }
            self.free.push(Block {
                first: held.first + blocks,
                blocks: held.blocks - blocks,
            });
            return Block {
                first: held.first,
                blocks,
            };
        }
        let claimed = Block {
            first: self.blocks,
            blocks,
        };
        self.blocks += blocks;
        claimed
    }

    fn release(&mut self, block: Block) {
        self.free.push(block);
        self.free.sort_unstable_by_key(|held| held.first);
        let mut merged: Vec<Block> = Vec::with_capacity(self.free.len());
        for held in self.free.drain(..) {
            match merged.last_mut() {
                Some(last) if last.first + last.blocks == held.first => last.blocks += held.blocks,
                _ => merged.push(held),
            }
        }
        self.free = merged;
    }
}

struct Slot {
    generation: u32,
    geometry: Option<Geometry>,
}

pub(crate) struct MeshLibrary {
    device: Device,
    stride: u32,
    vertices: Vec<Vertex>,
    buffer: GpuBuffer,
    capacity: u32,
    vertex_arena: Arena,
    skin_stride: u32,
    skins: Vec<SkinVertexRecord>,
    skin_buffer: GpuBuffer,
    skin_capacity: u32,
    skin_arena: Arena,
    slots: Vec<Slot>,
    free: Vec<u32>,
}

impl MeshLibrary {
    pub(crate) fn new(device: &Device) -> Self {
        let stride = Self::stride(device);
        let skin_stride = Self::skin_stride(device);
        let vertices = vec![empty_vertex(); INITIAL_BLOCKS as usize * stride as usize];
        let skins = vec![empty_skin(); SKIN_BLOCKS as usize * skin_stride as usize];
        Self {
            device: device.clone(),
            stride,
            buffer: Self::allocate(device, vertices.len()),
            vertices,
            capacity: INITIAL_BLOCKS * stride,
            vertex_arena: Arena::new(0),
            skin_stride,
            skin_buffer: Self::allocate_skins(device, skins.len()),
            skins,
            skin_capacity: SKIN_BLOCKS * skin_stride,
            skin_arena: Arena::new(SKIN_BLOCKS),
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    pub(crate) fn add(&mut self, queue: &Queue, vertices: &[Vertex]) -> Mesh {
        self.add_with_skin(queue, vertices, None)
    }

    pub(crate) fn add_skinned(
        &mut self,
        queue: &Queue,
        vertices: &[Vertex],
        skin: &[SkinVertexRecord],
    ) -> Mesh {
        assert_eq!(
            vertices.len(),
            skin.len(),
            "a mesh of {} vertices holds {} skin weights",
            vertices.len(),
            skin.len(),
        );
        assert!(
            !skin.is_empty(),
            "a skinned mesh of no vertex names no geometry",
        );
        self.add_with_skin(queue, vertices, Some(skin))
    }

    fn add_with_skin(
        &mut self,
        queue: &Queue,
        vertices: &[Vertex],
        skin: Option<&[SkinVertexRecord]>,
    ) -> Mesh {
        assert!(
            !vertices.is_empty(),
            "a mesh of no vertex names no geometry"
        );
        let geometry = self.write(queue, vertices, skin);
        let index = match self.free.pop() {
            Some(index) => index,
            None => {
                self.slots.push(Slot {
                    generation: 0,
                    geometry: None,
                });
                self.slots.len() as u32 - 1
            }
        };
        let slot = &mut self.slots[index as usize];
        slot.generation += 1;
        slot.geometry = Some(geometry);
        Mesh {
            index,
            generation: slot.generation,
        }
    }

    pub(crate) fn remove(&mut self, mesh: Mesh) {
        let geometry = self.release(mesh);
        self.return_spans(geometry);
        self.slots[mesh.index as usize].geometry = None;
        self.free.push(mesh.index);
    }

    pub(crate) fn update(
        &mut self,
        queue: &Queue,
        mesh: Mesh,
        vertices: &[Vertex],
        skin: Option<&[SkinVertexRecord]>,
    ) {
        assert!(
            !vertices.is_empty(),
            "a mesh of no vertex names no geometry"
        );
        if let Some(skin) = skin {
            assert_eq!(
                vertices.len(),
                skin.len(),
                "a mesh of {} vertices holds {} skin weights",
                vertices.len(),
                skin.len(),
            );
            assert!(
                !skin.is_empty(),
                "a skinned mesh of no vertex names no geometry",
            );
        }
        let previous = self.release(mesh);
        self.return_spans(previous);
        let geometry = self.write(queue, vertices, skin);
        self.slots[mesh.index as usize].geometry = Some(geometry);
    }

    fn release(&self, mesh: Mesh) -> Geometry {
        self.slots
            .get(mesh.index as usize)
            .filter(|slot| slot.generation == mesh.generation)
            .and_then(|slot| slot.geometry)
            .unwrap_or_else(|| panic!("mesh {} holds no geometry", mesh.index))
    }

    fn return_spans(&mut self, geometry: Geometry) {
        self.vertex_arena.release(Block {
            first: geometry.first / self.stride,
            blocks: geometry.count.div_ceil(self.stride),
        });
        if geometry.skin_count > 0 {
            self.skin_arena.release(Block {
                first: geometry.skin_first / self.skin_stride,
                blocks: geometry.skin_count.div_ceil(self.skin_stride),
            });
        }
    }

    fn write(
        &mut self,
        queue: &Queue,
        vertices: &[Vertex],
        skin: Option<&[SkinVertexRecord]>,
    ) -> Geometry {
        let block = self
            .vertex_arena
            .claim((vertices.len() as u32).div_ceil(self.stride));
        let first = block.first * self.stride;
        let written = block.first * self.stride + block.blocks * self.stride;
        if self.vertices.len() < written as usize {
            self.vertices.resize(written as usize, empty_vertex());
        }
        self.vertices[first as usize..first as usize + vertices.len()].copy_from_slice(vertices);
        if self.fit_vertices(written) {
            self.write_vertices(queue, 0, &self.vertices);
        } else {
            self.write_vertices(queue, first, vertices);
        }
        let (skin_first, skin_count) = match skin {
            None => (0, 0),
            Some(skin) => {
                let block = self
                    .skin_arena
                    .claim((skin.len() as u32).div_ceil(self.skin_stride));
                let first = block.first * self.skin_stride;
                let written = block.first * self.skin_stride + block.blocks * self.skin_stride;
                if self.skins.len() < written as usize {
                    self.skins.resize(written as usize, empty_skin());
                }
                self.skins[first as usize..first as usize + skin.len()].copy_from_slice(skin);
                if self.fit_skins(written) {
                    self.write_skins(queue, 0, &self.skins);
                } else {
                    self.write_skins(queue, first, skin);
                }
                (first, skin.len() as u32)
            }
        };
        let (center, radius) = Self::bounds(vertices);
        Geometry {
            first,
            count: vertices.len() as u32,
            skin_first,
            skin_count,
            center,
            radius,
        }
    }

    fn fit_vertices(&mut self, records: u32) -> bool {
        if records <= self.capacity {
            return false;
        }
        self.capacity = records.next_power_of_two();
        self.buffer = Self::allocate(&self.device, self.capacity as usize);
        true
    }

    fn fit_skins(&mut self, records: u32) -> bool {
        if records <= self.skin_capacity {
            return false;
        }
        self.skin_capacity = records.next_power_of_two();
        self.skin_buffer = Self::allocate_skins(&self.device, self.skin_capacity as usize);
        true
    }

    pub(crate) fn geometry(&self, mesh: Mesh) -> &Geometry {
        self.slots
            .get(mesh.index as usize)
            .filter(|slot| slot.generation == mesh.generation)
            .and_then(|slot| slot.geometry.as_ref())
            .unwrap_or_else(|| panic!("mesh {} holds no geometry", mesh.index))
    }

    pub(crate) fn buffer(&self) -> &GpuBuffer {
        &self.buffer
    }

    pub(crate) fn skin_binding(&self, geometry: &Geometry) -> dome_gpu::BufferBinding<'_> {
        if geometry.skin_count == 0 {
            self.skin_buffer.binding(0, SKIN_BYTES)
        } else {
            self.skin_buffer.binding(
                u64::from(geometry.skin_first) * SKIN_BYTES,
                u64::from(geometry.skin_count) * SKIN_BYTES,
            )
        }
    }

    fn write_vertices(&self, queue: &Queue, first: u32, vertices: &[Vertex]) {
        if vertices.is_empty() {
            return;
        }
        self.buffer.write_at(
            queue,
            u64::from(first) * VERTEX_BYTES,
            bytemuck::cast_slice(vertices),
        );
    }

    fn write_skins(&self, queue: &Queue, first: u32, skins: &[SkinVertexRecord]) {
        if skins.is_empty() {
            return;
        }
        self.skin_buffer.write_at(
            queue,
            u64::from(first) * SKIN_BYTES,
            bytemuck::cast_slice(skins),
        );
    }

    fn stride(device: &Device) -> u32 {
        let alignment = device.limits().min_storage_buffer_offset_alignment.max(4);
        span::blocks(alignment, VERTEX_BYTES)
    }

    fn skin_stride(device: &Device) -> u32 {
        let alignment = device.limits().min_storage_buffer_offset_alignment.max(4);
        span::blocks(alignment, SKIN_BYTES)
    }

    fn allocate(device: &Device, records: usize) -> GpuBuffer {
        GpuBuffer::new(
            device,
            "dome mesh vertices",
            records as u64 * VERTEX_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        )
    }

    fn allocate_skins(device: &Device, records: usize) -> GpuBuffer {
        GpuBuffer::new(
            device,
            "dome skin vertices",
            records as u64 * SKIN_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        )
    }

    fn bounds(vertices: &[Vertex]) -> (Vec3, f32) {
        let points = vertices
            .iter()
            .map(|vertex| Vec3::from_array(vertex.position))
            .collect::<Vec<_>>();
        let center = Aabb::from_points(&points).center();
        let radius = points
            .iter()
            .map(|point| point.distance(center))
            .fold(0.0, f32::max);
        (center, radius)
    }
}

fn empty_vertex() -> Vertex {
    Vertex {
        position: [0.0; 3],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0; 2],
    }
}

fn empty_skin() -> SkinVertexRecord {
    SkinVertexRecord {
        joints: Uvec4 {
            x: 0,
            y: 0,
            z: 0,
            w: 0,
        },
        weights: Fvec4 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        },
    }
}

const _: () = assert!(
    SKIN_BYTES == SKIN_VERTEX.size as u64,
    "the skin vertex record keeps the layout its shader reads",
);

#[cfg(test)]
mod tests {
    use super::{MeshLibrary, Vertex};
    use dome_gpu::{GpuContext, GpuRequest};

    fn vertices(count: usize) -> Vec<Vertex> {
        vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            };
            count
        ]
    }

    #[test]
    fn a_removed_mesh_hands_the_slots_it_held_back_to_its_successor() {
        let context = GpuContext::open(&GpuRequest::default())
            .unwrap_or_else(|error| panic!("no device reuses mesh slots: {error}"));
        let mut library = MeshLibrary::new(context.device());
        let first = library.add(context.queue(), &vertices(6));
        library.add(context.queue(), &vertices(6));
        let held = library.buffer().size();
        library.remove(first);
        let again = library.add(context.queue(), &vertices(6));
        assert_eq!(
            again.index(),
            first.index(),
            "a removed mesh hands its slot back to its successor",
        );
        assert_ne!(
            again, first,
            "the slot it hands back carries a new identity"
        );
        assert_eq!(
            library.buffer().size(),
            held,
            "reusing a slot leaves the vertex arena the size it grew to",
        );
        assert_eq!(library.geometry(again).count, 6);
    }
}
