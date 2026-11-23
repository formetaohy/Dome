use crate::abi::{SKIN_VERTEX, SkinVertexRecord, VERTEX_FLOATS, Vertex};
use crate::span;
use dome_gpu::{Fvec4, Uvec4};
use dome_gpu_device::{BufferUsages, Device, GpuBuffer, Queue};
use dome_math::{Aabb, Vec3};
use std::mem::size_of;

const VERTEX_BYTES: u64 = VERTEX_FLOATS as u64 * 4;
const SKIN_BYTES: u64 = size_of::<SkinVertexRecord>() as u64;
const INITIAL_VERTICES: u32 = 1024;
const INITIAL_SKINS: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mesh(u32);

impl Mesh {
    pub(crate) fn index(self) -> u32 {
        self.0
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
    pub mesh: Mesh,
}

pub(crate) struct MeshLibrary {
    device: Device,
    vertices: Vec<Vertex>,
    buffer: GpuBuffer,
    capacity: u32,
    stride: usize,
    skins: Vec<SkinVertexRecord>,
    skin_buffer: GpuBuffer,
    skin_capacity: u32,
    skin_stride: usize,
    geometries: Vec<Geometry>,
    dead_vertices: u64,
    dead_skins: u64,
}

impl MeshLibrary {
    pub(crate) fn new(device: &Device) -> Self {
        Self {
            device: device.clone(),
            vertices: Vec::new(),
            buffer: Self::allocate(device, INITIAL_VERTICES),
            capacity: INITIAL_VERTICES,
            stride: Self::stride(device),
            skins: vec![empty_skin()],
            skin_buffer: Self::allocate_skins(device, INITIAL_SKINS),
            skin_capacity: INITIAL_SKINS,
            skin_stride: Self::skin_stride(device),
            geometries: Vec::new(),
            dead_vertices: 0,
            dead_skins: 0,
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
        let (first, count) = self.write_vertices(queue, vertices);
        let (skin_first, skin_count) = self.write_skin(queue, skin);
        let mesh = Mesh(self.geometries.len() as u32);
        let (center, radius) = Self::bounds(vertices);
        self.geometries.push(Geometry {
            first,
            count,
            skin_first,
            skin_count,
            center,
            radius,
            mesh,
        });
        mesh
    }

    pub(crate) fn remove(&mut self, queue: &Queue, mesh: Mesh) {
        let index = mesh.0 as usize;
        let geometry = self
            .geometries
            .get_mut(index)
            .unwrap_or_else(|| panic!("mesh {} belongs to another renderer", mesh.0));
        self.dead_vertices += u64::from(geometry.count);
        self.dead_skins += u64::from(geometry.skin_count);
        geometry.count = 0;
        geometry.skin_count = 0;
        geometry.center = Vec3::ZERO;
        geometry.radius = 0.0;
        self.compact(queue);
    }

    pub(crate) fn update(
        &mut self,
        queue: &Queue,
        mesh: Mesh,
        vertices: &[Vertex],
        skin: Option<&[SkinVertexRecord]>,
    ) {
        let index = mesh.0 as usize;
        let previous = *self
            .geometries
            .get(index)
            .unwrap_or_else(|| panic!("mesh {} belongs to another renderer", mesh.0));
        let (first, count) = self.write_vertices(queue, vertices);
        let (skin_first, skin_count) = self.write_skin(queue, skin);
        let (center, radius) = Self::bounds(vertices);
        self.dead_vertices += u64::from(previous.count);
        self.dead_skins += u64::from(previous.skin_count);
        self.geometries[index] = Geometry {
            first,
            count,
            skin_first,
            skin_count,
            center,
            radius,
            mesh,
        };
        self.compact(queue);
    }

    fn write_vertices(&mut self, queue: &Queue, vertices: &[Vertex]) -> (u32, u32) {
        assert!(
            !vertices.is_empty(),
            "a mesh of no vertex names no geometry"
        );
        let first = self.vertices.len().next_multiple_of(self.stride) as u32;
        let count = vertices.len() as u32;
        self.vertices.resize(first as usize, empty_vertex());
        self.vertices.extend_from_slice(vertices);
        let written = self.vertices.len() as u32;
        if written > self.capacity {
            self.capacity = written.next_power_of_two();
            self.buffer = Self::allocate(&self.device, self.capacity);
            self.write(queue, 0, &self.vertices[..]);
        } else {
            self.write(queue, first, &self.vertices[first as usize..]);
        }
        (first, count)
    }

    fn write_skin(&mut self, queue: &Queue, skin: Option<&[SkinVertexRecord]>) -> (u32, u32) {
        let Some(skin) = skin else {
            return (0, 0);
        };
        let first = self.skins.len().next_multiple_of(self.skin_stride) as u32;
        self.skins.resize(first as usize, empty_skin());
        self.skins.extend_from_slice(skin);
        let written = self.skins.len() as u32;
        if written > self.skin_capacity {
            self.skin_capacity = written.next_power_of_two();
            self.skin_buffer = Self::allocate_skins(&self.device, self.skin_capacity);
            self.write_skins(queue, 0, &self.skins[..]);
        } else {
            self.write_skins(queue, first, &self.skins[first as usize..]);
        }
        (first, skin.len() as u32)
    }

    fn compact(&mut self, queue: &Queue) {
        if self.dead_vertices < 1024 && self.dead_skins < 1024 {
            return;
        }
        let live = self
            .geometries
            .iter()
            .map(|geometry| u64::from(geometry.count))
            .sum::<u64>();
        let live_skins = self
            .geometries
            .iter()
            .map(|geometry| u64::from(geometry.skin_count))
            .sum::<u64>();
        if self.dead_vertices <= live && self.dead_skins <= live_skins {
            return;
        }
        let vertices = std::mem::take(&mut self.vertices);
        let skins = std::mem::take(&mut self.skins);
        let mut packed = Vec::with_capacity(live as usize);
        let mut packed_skins = vec![empty_skin()];
        for geometry in &mut self.geometries {
            let first = packed.len().next_multiple_of(self.stride);
            packed.resize(first, empty_vertex());
            let start = geometry.first as usize;
            packed.extend_from_slice(&vertices[start..start + geometry.count as usize]);
            geometry.first = first as u32;
            let skin_first = packed_skins.len().next_multiple_of(self.skin_stride);
            packed_skins.resize(skin_first, empty_skin());
            if geometry.skin_count > 0 {
                let start = geometry.skin_first as usize;
                packed_skins.extend_from_slice(&skins[start..start + geometry.skin_count as usize]);
            }
            geometry.skin_first = skin_first as u32;
        }
        self.vertices = packed;
        self.skins = packed_skins;
        if self.vertices.len() as u32 > self.capacity {
            self.capacity = (self.vertices.len() as u32).next_power_of_two();
            self.buffer = Self::allocate(&self.device, self.capacity);
        }
        self.write(queue, 0, &self.vertices[..]);
        if self.skins.len() as u32 > self.skin_capacity {
            self.skin_capacity = (self.skins.len() as u32).next_power_of_two();
            self.skin_buffer = Self::allocate_skins(&self.device, self.skin_capacity);
        }
        self.write_skins(queue, 0, &self.skins[..]);
        self.dead_vertices = 0;
        self.dead_skins = 0;
    }

    pub(crate) fn geometry(&self, mesh: Mesh) -> &Geometry {
        self.geometries
            .get(mesh.0 as usize)
            .unwrap_or_else(|| panic!("mesh {} belongs to another renderer", mesh.0))
    }

    pub(crate) fn buffer(&self) -> &GpuBuffer {
        &self.buffer
    }

    pub(crate) fn skin_binding(&self, geometry: &Geometry) -> dome_gpu_device::BufferBinding<'_> {
        if geometry.skin_count == 0 {
            self.skin_buffer.binding(0, SKIN_BYTES)
        } else {
            self.skin_buffer.binding(
                u64::from(geometry.skin_first) * SKIN_BYTES,
                u64::from(geometry.skin_count) * SKIN_BYTES,
            )
        }
    }

    fn write(&self, queue: &Queue, first: u32, vertices: &[Vertex]) {
        self.buffer.write_at(
            queue,
            u64::from(first) * VERTEX_BYTES,
            bytemuck::cast_slice(vertices),
        );
    }

    fn write_skins(&self, queue: &Queue, first: u32, skins: &[SkinVertexRecord]) {
        self.skin_buffer.write_at(
            queue,
            u64::from(first) * SKIN_BYTES,
            bytemuck::cast_slice(skins),
        );
    }

    fn stride(device: &Device) -> usize {
        let alignment = device.limits().min_storage_buffer_offset_alignment.max(4);
        span::blocks(alignment, VERTEX_BYTES) as usize
    }

    fn skin_stride(device: &Device) -> usize {
        let alignment = device.limits().min_storage_buffer_offset_alignment.max(4);
        span::blocks(alignment, SKIN_BYTES) as usize
    }

    fn allocate(device: &Device, vertices: u32) -> GpuBuffer {
        GpuBuffer::new(
            device,
            "dome mesh vertices",
            u64::from(vertices) * VERTEX_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        )
    }

    fn allocate_skins(device: &Device, vertices: u32) -> GpuBuffer {
        GpuBuffer::new(
            device,
            "dome skin vertices",
            u64::from(vertices) * SKIN_BYTES,
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
