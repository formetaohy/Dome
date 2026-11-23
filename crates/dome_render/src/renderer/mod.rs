mod bounds;
mod capture;
mod channel;
mod pass;
mod scene;

mod record;

use channel::ChannelFrame;
use record::{FrameRecord, FrameShared};
use scene::Collection;

use crate::abi::{
    ARGUMENT_WORDS, CascadeRecord, DebugLineRecord, DrawRecord, InstanceRecord, JOINT, JointRecord,
    LIGHT, LIGHT_CAPACITY, LIGHT_SLICES, LIGHT_STRIDE, LIGHT_TILE, LightRecord, MATERIAL,
    MaterialRecord, NO_HIGHLIGHT, POINT_LIGHT, PointLightRecord, SHADOW_ATLAS,
    SHADOW_ATLAS_COLUMNS, SHADOW_CASCADES, SHADOW_SIDE, SkinVertexRecord, SkyRecord, VERTEX_FLOATS,
    ViewRecord, WORKGROUP,
};
use crate::batch::{Pipelines, Shared, Slot, SlotGroups, table};
use crate::capture::{Capture, CaptureFrame, CaptureProbe, Captures, Probe};
use crate::damage::Damage;
use crate::environment::{
    Atmosphere, AtmosphereBaker, AtmospherePanorama, BakeJob, BakeRequest, Baked, Baker,
    Environment,
};
use crate::frame::{
    Batch, Placement, Screen, ambient_of, arguments_of, bases_of, camera_of, direction_record,
    light_of, record_of, record_of_cascade, record_of_main_view, record_of_view, vector,
};
use crate::growing::Growing;
use crate::hidden::Hidden;
use crate::hzb::Hzb;
use crate::instance::MeshInstance;
use crate::instances::Instances;
use crate::light::PointLight;
use crate::line::Lines;
use crate::material::{AlphaMode, Material, MaterialDesc, MaterialLibrary};
use crate::mesh::{Mesh, MeshLibrary};
use crate::palette::Palettes;
use crate::pick::{PickBatch, PickFrame, PickResources, Picker};
use crate::point_shadow::{POINT_SHADOW_NEAR, PointShadowFrame, PointShadows};
use crate::post::Post;
use crate::probe::Probes;
use crate::program;
use crate::shading::Shading;
use crate::shadow::{self, Travel};
use crate::skin::Skin;
use crate::span;
use crate::temporality::{Frame, Pose, Temporality};
use crate::transparency::Transparency;
use crate::view::View;
use dome_ecs::{Entity, World};
use dome_gpu::{
    AddressMode, BindGroup, Binding, BufferUsages, DepthOps, DepthTexture, Device, Draw, Filter,
    GpuBuffer, GpuContext, GraphicsPipeline, Pass, PipelineHandle, PipelineState, Queue, Sampler,
    SamplerDesc, Submission, SurfaceFrame, Texture, TextureFormat, TextureUsages, Viewport,
};
use dome_math::{Mat4, Transform, Vec2, Vec3};
use dome_shader::Fvec4;
use std::collections::HashMap;
use std::mem::size_of;

const VIEW_BYTES: u64 = size_of::<ViewRecord>() as u64;
const LIGHT_BYTES: u64 = size_of::<LightRecord>() as u64;
const CASCADE_BYTES: u64 = size_of::<CascadeRecord>() as u64;
const INSTANCE_BYTES: u64 = size_of::<InstanceRecord>() as u64;
const DRAW_BYTES: u64 = size_of::<DrawRecord>() as u64;
const MATERIAL_BYTES: u64 = size_of::<MaterialRecord>() as u64;
const POINT_LIGHT_BYTES: u64 = size_of::<PointLightRecord>() as u64;
const CLUSTER_BYTES: u64 = LIGHT_STRIDE as u64 * WORD_BYTES;
const ARGUMENT_BYTES: u64 = (ARGUMENT_WORDS * 4) as u64;
const ARGUMENT_USIZE: usize = ARGUMENT_BYTES as usize;
const WORD_BYTES: u64 = 4;
const VERTEX_BYTES: u64 = (VERTEX_FLOATS * 4) as u64;
const JOINT_BYTES: u64 = size_of::<JointRecord>() as u64;
const DEBUG_LINE_BYTES: u64 = size_of::<DebugLineRecord>() as u64;
const SKY_BYTES: u64 = size_of::<SkyRecord>() as u64;
const SKY_VERTICES: u32 = 3;
const CASCADE_COUNT: u32 = SHADOW_CASCADES;

struct CaptureInputs<'a> {
    batches: &'a [Batch],
    bases: &'a [u32],
    arguments: &'a [u32],
    spheres: &'a [(Vec3, f32)],
    points: &'a [PointLightRecord],
    instances: u32,
    slots: u32,
    opaque: u32,
    probe_count: u32,
}
pub enum Target<'a> {
    Texture(&'a Texture),
    Surface(SurfaceFrame),
}

pub struct Renderer {
    device: Device,
    queue: Queue,
    cull: PipelineHandle,
    light_clusters: PipelineHandle,
    draw: GraphicsPipeline,
    capture_draw: GraphicsPipeline,
    transparent_draw: GraphicsPipeline,
    scene_depth: Option<DepthTexture>,
    sky: GraphicsPipeline,
    sky_view: BindGroup,
    sky_record: GpuBuffer,
    sky_shade: BindGroup,
    line_pipeline: GraphicsPipeline,
    post: Post,
    shadow: GraphicsPipeline,
    depth: GraphicsPipeline,
    hzb_base: GraphicsPipeline,
    hzb_reduce: GraphicsPipeline,
    hzb_sampler: Sampler,
    hzb: Option<Hzb>,
    meshes: MeshLibrary,
    materials: MaterialLibrary,
    material_records: Growing,
    materials_dirty: bool,
    shade: BindGroup,
    transparent_shade: BindGroup,
    shadow_map: Texture,
    shadow_depth: DepthTexture,
    views: GpuBuffer,
    shadow_views: GpuBuffer,
    light: GpuBuffer,
    shadow_sampler: Sampler,
    shading: Shading,
    baker: Baker,
    atmospheres: AtmosphereBaker,
    bakes: HashMap<usize, BakeJob>,
    next_bake: usize,
    captures: Captures,
    ambient: Option<(usize, usize, usize)>,
    cascades: GpuBuffer,
    instance_buffer: Growing,
    instances: Instances,
    depth_drawn: Growing,
    depth_arguments: Growing,
    main_drawn: Growing,
    main_arguments: Growing,
    main_batches: u32,
    shadow_drawn: Growing,
    shadow_arguments: Growing,
    shadow_span: u64,
    lighting: Growing,
    lighting_capacity: u64,
    palettes: Palettes,
    line_records: Growing,
    picker: Picker,
    pick_frame: Option<PickFrame>,
    point_shadows: PointShadows,
    point_shadow_identity: usize,
    transparency: Transparency,
    temporality: Temporality,
    spheres: Vec<(Entity, (Vec3, f32))>,
    damage: Damage,
    probes: Probes,
}

impl Renderer {
    pub fn new(context: &GpuContext, format: TextureFormat) -> Self {
        assert!(
            matches!(
                format,
                TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm
            ),
            "a renderer resolves tone mapped frames into an eight bit target",
        );
        let device = context.device().clone();
        let queue = Queue::of(&device);
        let storage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
        let shadow_map = Texture::new(
            &device,
            "dome shadow map",
            SHADOW_ATLAS,
            SHADOW_ATLAS,
            1,
            TextureFormat::R32Float,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::SAMPLED,
        );
        let shadow_sampler = Sampler::new(
            &device,
            SamplerDesc {
                min_filter: Filter::Nearest,
                mag_filter: Filter::Nearest,
                mip_filter: Filter::Nearest,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
            },
        );
        assert!(
            device.capability().bindless,
            "the renderer needs a device whose shaders index resource tables"
        );
        let limits = device.limits();
        assert!(
            dome_shader::TEXTURE_TABLE_CAPACITY <= limits.max_sampled_images_per_shader_stage,
            "the renderer material table of {} images outruns the device limit of {}",
            dome_shader::TEXTURE_TABLE_CAPACITY,
            limits.max_sampled_images_per_shader_stage,
        );
        assert!(
            dome_shader::SAMPLER_TABLE_CAPACITY <= limits.max_samplers_per_shader_stage,
            "the renderer material table of {} samplers outruns the device limit of {}",
            dome_shader::SAMPLER_TABLE_CAPACITY,
            limits.max_samplers_per_shader_stage,
        );
        let draw = GraphicsPipeline::new_depth(
            &device,
            program::instancing(),
            TextureFormat::Rgba16Float,
            PipelineState::COVERED,
        );
        let capture_draw = GraphicsPipeline::new_depth(
            &device,
            program::instancing(),
            TextureFormat::Rgba16Float,
            PipelineState::DEPTH_WRITE,
        );
        let transparent_draw = GraphicsPipeline::new_depth(
            &device,
            program::instancing(),
            TextureFormat::Rgba16Float,
            PipelineState::TRANSPARENT,
        );
        let line_pipeline = GraphicsPipeline::new_depth(
            &device,
            program::debug_lines(),
            TextureFormat::Rgba16Float,
            PipelineState::DEBUG,
        );
        let sky = GraphicsPipeline::new_depth(
            &device,
            program::sky_program(),
            TextureFormat::Rgba16Float,
            PipelineState::COVERED,
        );
        let shadow = GraphicsPipeline::new_depth(
            &device,
            program::shadowing(),
            TextureFormat::R32Float,
            PipelineState::DEPTH_WRITE,
        );
        let shadow_depth =
            DepthTexture::new(&device, "dome shadow depth", SHADOW_ATLAS, SHADOW_ATLAS);
        let depth = GraphicsPipeline::new_depth(
            &device,
            program::depth_prepass(),
            TextureFormat::R32Float,
            PipelineState::DEPTH_WRITE,
        );
        let hzb_base = GraphicsPipeline::new(
            &device,
            program::hzb_base(),
            TextureFormat::R32Float,
            PipelineState::SCREEN,
        );
        let hzb_reduce = GraphicsPipeline::new(
            &device,
            program::hzb_reduce(),
            TextureFormat::R32Float,
            PipelineState::SCREEN,
        );
        let hzb_sampler = Sampler::new(
            &device,
            SamplerDesc {
                min_filter: Filter::Nearest,
                mag_filter: Filter::Nearest,
                mip_filter: Filter::Nearest,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::ClampToEdge,
            },
        );
        let shading = Shading::new(&device, &queue);
        let probes = Probes::new(&device, shading.fallback(), shading.sampler());
        let baker = Baker::new(context);
        let captures = Captures::new(context);
        let point_shadows = PointShadows::new(&device);
        let point_shadow_identity = point_shadows.atlas().identity();
        let sky_record = GpuBuffer::new(
            &device,
            "dome sky",
            SKY_BYTES,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let sky_view = sky.bind_group(
            0,
            &[Binding::buffer(
                program::SKY_VIEW,
                sky_record.binding(0, SKY_BYTES),
            )],
        );
        let shadows = (&shadow_map, &shadow_sampler);
        let point = (point_shadows.atlas(), point_shadows.sampler());
        let probe_tables = (probes.textures(), probes.samplers());
        let shade = shading.group(&draw, shadows, point, probe_tables);
        let transparent_shade = shading.group(&transparent_draw, shadows, point, probe_tables);
        let sky_shade = shading.sky(&sky);
        let cull = context.declare(program::culling());
        let light_clusters = context.declare(program::clustering());
        let post = Post::new(&device, format);
        let meshes = MeshLibrary::new(&device);
        let materials = MaterialLibrary::new(&device, &queue);
        let views = GpuBuffer::new(&device, "dome views", 2 * VIEW_BYTES, storage);
        let shadow_views = GpuBuffer::new(
            &device,
            "dome shadow views",
            u64::from(CASCADE_COUNT) * VIEW_BYTES,
            storage,
        );
        let light = GpuBuffer::new(&device, "dome light", LIGHT_BYTES, storage);
        let cascades = GpuBuffer::new(
            &device,
            "dome cascades",
            u64::from(CASCADE_COUNT) * CASCADE_BYTES,
            storage,
        );
        let instance_buffer = Growing::new(&device, "dome instances", storage);
        let depth_drawn = Growing::new(&device, "dome depth drawn", storage);
        let depth_arguments = Growing::new(
            &device,
            "dome depth arguments",
            storage | BufferUsages::INDIRECT,
        );
        let main_drawn = Growing::new(&device, "dome drawn", storage);
        let main_arguments = Growing::new(
            &device,
            "dome arguments",
            storage | BufferUsages::INDIRECT | BufferUsages::COPY_SRC,
        );
        let shadow_drawn = Growing::new(&device, "dome shadow drawn", storage);
        let shadow_arguments = Growing::new(
            &device,
            "dome shadow arguments",
            storage | BufferUsages::INDIRECT,
        );
        let lighting = Growing::new(&device, "dome lighting", storage);
        let picker = Picker::new(&device);
        Self {
            cull,
            light_clusters,
            draw,
            capture_draw,
            transparent_draw,
            scene_depth: None,
            sky,
            sky_view,
            sky_record,
            sky_shade,
            line_pipeline,
            post,
            shadow,
            depth,
            hzb_base,
            hzb_reduce,
            hzb_sampler,
            hzb: None,
            meshes,
            materials,
            material_records: Growing::new(&device, "dome material records", storage),
            materials_dirty: true,
            shade,
            transparent_shade,
            shadow_map,
            shadow_depth,
            views,
            shadow_views,
            light,
            shadow_sampler,
            shading,
            baker,
            atmospheres: AtmosphereBaker::new(context),
            bakes: HashMap::new(),
            next_bake: 0,
            ambient: None,
            cascades,
            instance_buffer,
            instances: Instances::new(),
            depth_drawn,
            depth_arguments,
            main_drawn,
            main_arguments,
            main_batches: 0,
            shadow_drawn,
            shadow_arguments,
            shadow_span: 0,
            lighting,
            lighting_capacity: 0,
            palettes: Palettes::new(),
            line_records: Growing::new(&device, "dome debug lines", storage),
            picker,
            pick_frame: None,
            point_shadows,
            point_shadow_identity,
            transparency: Transparency::new(&device),
            temporality: Temporality::new(&device),
            spheres: Vec::new(),
            damage: Damage::default(),
            probes,
            captures,
            queue,
            device,
        }
    }

    fn refresh_ambient(&mut self, environment: Option<&Environment>) {
        self.shading.follow(environment);
        let identity = self.shading.identity();
        let point = self.point_shadows.atlas().identity();
        if self.ambient == identity && self.point_shadow_identity == point {
            return;
        }
        let shadow = (&self.shadow_map, &self.shadow_sampler);
        let point_shadow = (self.point_shadows.atlas(), self.point_shadows.sampler());
        let probe_tables = (self.probes.textures(), self.probes.samplers());
        self.shade = self
            .shading
            .group(&self.draw, shadow, point_shadow, probe_tables);
        self.transparent_shade =
            self.shading
                .group(&self.transparent_draw, shadow, point_shadow, probe_tables);
        self.sky_shade = self.shading.sky(&self.sky);
        self.ambient = identity;
        self.point_shadow_identity = point;
    }

    pub fn mesh(&mut self, vertices: &[crate::abi::Vertex]) -> Mesh {
        self.meshes.add(&self.queue, vertices)
    }

    pub fn skinned_mesh(
        &mut self,
        vertices: &[crate::abi::Vertex],
        skin: &[SkinVertexRecord],
    ) -> Mesh {
        self.meshes.add_skinned(&self.queue, vertices, skin)
    }

    pub fn idle(&self) {
        self.queue.idle();
    }

    pub fn remove_mesh(&mut self, mesh: Mesh) {
        self.meshes.remove(&self.queue, mesh);
        self.pick_frame = None;
    }

    pub fn update_mesh(&mut self, mesh: Mesh, vertices: &[crate::abi::Vertex]) {
        self.meshes.update(&self.queue, mesh, vertices, None);
        self.pick_frame = None;
    }

    pub fn update_skinned_mesh(
        &mut self,
        mesh: Mesh,
        vertices: &[crate::abi::Vertex],
        skin: &[SkinVertexRecord],
    ) {
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
        self.meshes.update(&self.queue, mesh, vertices, Some(skin));
        self.pick_frame = None;
    }

    pub fn texture(
        &self,
        label: &str,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: TextureFormat,
        usages: TextureUsages,
    ) -> Texture {
        Texture::new(
            &self.device,
            label,
            width,
            height,
            mip_levels,
            format,
            usages,
        )
    }

    pub fn write_texture(&self, texture: &Texture, levels: &[&[u8]]) {
        texture.write_levels(&self.queue, levels);
    }

    pub fn sampler(&self, desc: SamplerDesc) -> Sampler {
        Sampler::new(&self.device, desc)
    }

    pub fn bake(&mut self, panorama: &Texture, sampler: &Sampler) -> BakeRequest {
        let request = BakeRequest::of(self.next_bake);
        self.next_bake += 1;
        self.bakes
            .insert(request.index(), self.baker.submit(panorama, sampler));
        request
    }

    pub fn atmosphere(&mut self, atmosphere: &Atmosphere) -> AtmospherePanorama {
        self.atmospheres.bake(atmosphere)
    }

    pub fn take_baked(&mut self, request: BakeRequest) -> Option<Baked> {
        let job = self.bakes.get(&request.index())?;
        let baked = self.baker.take(job)?;
        self.bakes.remove(&request.index());
        Some(baked)
    }

    pub fn wait_baked(&mut self, request: BakeRequest) -> Baked {
        let job = self
            .bakes
            .remove(&request.index())
            .expect("an environment bake waits for a request it holds");
        self.baker.wait(&job)
    }

    pub fn material(&mut self, desc: MaterialDesc) -> Material {
        let material = self.materials.add(desc);
        self.materials_changed();
        material
    }

    pub fn update_material(&mut self, material: Material, desc: MaterialDesc) {
        self.materials.set(material, desc);
        self.materials_changed();
    }

    pub fn remove_material(&mut self, material: Material) {
        self.materials.remove(material);
        self.materials_changed();
    }

    fn scene_depth(&mut self, width: u32, height: u32) -> DepthTexture {
        if let Some(held) = &self.scene_depth
            && held.width() == width
            && held.height() == height
        {
            return held.clone();
        }
        let depth = DepthTexture::new(&self.device, "dome scene depth", width, height);
        self.scene_depth = Some(depth.clone());
        depth
    }

    fn materials_changed(&mut self) {
        self.materials_dirty = true;
    }

    fn flush_materials(&mut self) {
        if !self.materials_dirty {
            return;
        }
        let records = self.materials.records();
        if records.is_empty() {
            self.materials_dirty = false;
            return;
        }
        self.material_records
            .fit(&self.device, records.len() as u64 * MATERIAL_BYTES);
        self.material_records
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(records));
        self.materials_dirty = false;
    }

    pub fn render(
        &mut self,
        world: &mut World,
        target: Target<'_>,
        clear: [f32; 4],
        highlight: Option<Entity>,
    ) {
        let (texture, frame) = match target {
            Target::Texture(texture) => (texture.clone(), None),
            Target::Surface(frame) => (frame.texture().clone(), Some(frame)),
        };
        self.flush_materials();
        self.settle_captures();
        let collection = self.collect(world, highlight);
        self.absorb_damage(world, &collection);
        let ambient = ambient_of(world);
        let (environment, environment_transform) = match ambient.global {
            Some((environment, transform)) => (Some(environment), transform),
            None => (None, Transform::IDENTITY),
        };
        let (capture_rotation, capture_intensity) = match ambient.global {
            Some((held, transform)) => (
                Mat4::from_quat(transform.rotation.normalize().inverse()),
                held.intensity,
            ),
            None => (Mat4::IDENTITY, 0.0),
        };
        let (probe_count, probe_textures) =
            self.probes
                .prepare(&self.device, &self.queue, &ambient.local);
        if probe_textures {
            self.ambient = None;
        }
        let mut point_light_records = Vec::new();
        world.for_each_pair::<Transform, PointLight>(|entity, transform, light| {
            if world.contains::<Hidden>(entity) {
                return;
            }
            point_light_records.push(PointLightRecord {
                position: Fvec4 {
                    x: transform.translation.x,
                    y: transform.translation.y,
                    z: transform.translation.z,
                    w: light.range,
                },
                radiance: Fvec4 {
                    x: light.color.x * light.intensity,
                    y: light.color.y * light.intensity,
                    z: light.color.z * light.intensity,
                    w: 0.0,
                },
                shadow: Fvec4 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                },
            });
        });
        let point_light_count = point_light_records.len() as u32;
        let rows = point_light_count.max(1) as f32;
        for (index, record) in point_light_records.iter_mut().enumerate() {
            record.shadow = Fvec4 {
                x: index as f32 / rows,
                y: 1.0 / rows,
                z: POINT_SHADOW_NEAR,
                w: record.position.w,
            };
        }
        self.point_shadows
            .ensure(&self.device, point_light_records.len());
        self.refresh_ambient(environment);
        let width = texture.width();
        let height = texture.height();
        let vertical = crate::frame::vertical(&self.device);
        self.temporality
            .prepare(&self.device, &self.queue, width, height);
        let jitter = self.temporality.jitter();
        let aspect = width as f32 / height as f32;
        let (camera, transform) = camera_of(world);
        let (light, light_transform) = light_of(world);
        let eye = transform.translation;
        let forward = transform.rotation.rotate_vec3(-Vec3::Z).normalize();
        let view = camera.view_jittered(
            transform,
            aspect,
            self.device.clip_space(),
            jitter,
            width,
            height,
        );
        let pose = Pose {
            clip: view.clip,
            eye,
            forward,
        };
        self.temporality.begin(
            &self.queue,
            pose,
            vertical,
            self.palettes.base(),
            self.palettes.previous_base(),
        );
        let probe = match self.captures.pending() {
            Some(_) => None,
            None => self.stale_probe(world),
        };
        let recorded = self.record(FrameRecord {
            texture: &texture,
            clear,
            view: &view,
            eye,
            forward,
            camera,
            transform,
            light,
            light_transform,
            environment,
            environment_transform,
            vertical,
            aspect,
            point_lights: &point_light_records,
            probe_count,
            spheres: &collection.spheres,
            highlight: collection.highlight,
            total: collection.total,
            world,
        });
        let FrameShared {
            travel,
            line_groups,
            line_vertices,
            sky,
            cluster_offset_bytes,
            cluster_bytes,
            cluster_x,
            cluster_y,
            lighting_bytes,
            ..
        } = recorded;
        self.lighting_capacity = lighting_bytes;
        let depth = self.scene_depth(texture.width(), texture.height());
        let mut submission = Submission::new(&self.device, "dome frame");
        if collection.total == 0 {
            self.main_batches = 0;
            self.pick_frame = None;
            let sky_groups = [self.sky_view.clone(), self.sky_shade.clone()];
            let mut debug_draws = Vec::new();
            if sky {
                debug_draws.push(Draw::new(&self.sky, &sky_groups, SKY_VERTICES));
            }
            if !line_groups.is_empty() {
                debug_draws.push(Draw::new(&self.line_pipeline, &line_groups, line_vertices));
            }
            submission.render(
                Pass::new(self.temporality.target(), [0.0, 0.0, 0.0, 0.0]),
                &[],
            );
            self.transparency.clear_dropped(&mut submission);
            submission.render(
                Pass::new(self.post.hdr(), [clear[0], clear[1], clear[2], 0.0])
                    .depth(&depth, DepthOps::CLEAR),
                &debug_draws,
            );
            self.temporality.resolve(
                &self.queue,
                &mut submission,
                Frame {
                    source: self.post.hdr(),
                    pose,
                    near: camera.near,
                    far: camera.far,
                    vertical,
                },
            );
            let resolved = self.temporality.resolved().clone();
            self.post.resolve(&mut submission, &resolved);
            self.post.tonemap(&mut submission, &texture, &resolved);
            if let Some(frame) = frame {
                submission.present(frame);
            }
            submission.submit(&self.queue);
            self.palettes.finish();
            self.temporality.finish(pose);
            self.capture(
                probe,
                &CaptureInputs {
                    batches: &[],
                    bases: &[],
                    arguments: &[],
                    spheres: &[],
                    points: &point_light_records,
                    instances: 0,
                    slots: 0,
                    opaque: 0,
                    probe_count,
                },
                &travel,
                capture_rotation,
                capture_intensity,
            );
            return;
        }
        self.pick_frame = Some(PickFrame {
            clip: view.clip,
            width: texture.width(),
            height: texture.height(),
            batches: collection
                .batches
                .iter()
                .zip(&collection.bases)
                .map(|(batch, base)| PickBatch {
                    mesh: batch.mesh,
                    base: *base,
                    count: batch.count,
                })
                .collect(),
            entities: collection.entities.clone(),
        });
        let blended = collection.depth_batches < collection.batches.len() as u32;
        let channels = self.channels(
            &mut submission,
            ChannelFrame {
                clear,
                collection: &collection,
                batches: &collection.batches,
                bases: &collection.bases,
                slots: collection.slots,
                total: collection.total,
                depth_batches: collection.depth_batches,
                points: &point_light_records,
                cluster_offset_bytes,
                cluster_bytes,
                cluster_x,
                cluster_y,
                sky,
                line_groups: &line_groups,
                line_vertices,
                probe_count,
                depth: depth.clone(),
            },
        );
        let arguments = channels.arguments;
        let lit = if blended {
            self.transparency
                .resolve(&mut submission, self.post.hdr(), self.post.lit());
            self.post.lit()
        } else {
            self.post.hdr()
        };
        self.temporality.resolve(
            &self.queue,
            &mut submission,
            Frame {
                source: lit,
                pose,
                near: camera.near,
                far: camera.far,
                vertical,
            },
        );
        let resolved = self.temporality.resolved().clone();
        self.post.resolve(&mut submission, &resolved);
        self.post.tonemap(&mut submission, &texture, &resolved);
        if let Some(frame) = frame {
            submission.present(frame);
        }
        submission.submit(&self.queue);
        self.palettes.finish();
        self.temporality.finish(pose);
        self.capture(
            probe,
            &CaptureInputs {
                batches: &collection.batches,
                bases: &collection.bases,
                arguments: &arguments,
                spheres: &collection.spheres,
                points: &point_light_records,
                instances: collection.total,
                slots: collection.slots,
                opaque: collection.depth_batches,
                probe_count,
            },
            &travel,
            capture_rotation,
            capture_intensity,
        );
    }

    pub fn take_captures(&mut self) -> Vec<Capture> {
        self.settle_captures();
        self.captures.take()
    }

    pub fn pick(&mut self, point: Vec2) -> Option<Entity> {
        let frame = self.pick_frame.as_ref()?;
        let clip_space = self.device.clip_space();
        let resources = PickResources {
            drawn: self.main_drawn.buffer(),
            arguments: self.main_arguments.buffer(),
            meshes: &self.meshes,
            palette: self.palettes.binding(),
        };
        self.picker.pick(
            &self.device,
            &self.queue,
            clip_space,
            frame,
            &resources,
            point,
        )
    }

    pub fn visible_instances(&self) -> u32 {
        let batches = self.main_batches;
        if batches == 0 {
            return 0;
        }
        let bytes = u64::from(batches) * ARGUMENT_BYTES;
        let readback = GpuBuffer::new(
            &self.device,
            "dome visibility readback",
            bytes,
            BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        );
        let mut submission = Submission::new(&self.device, "dome visibility");
        submission.copy(self.main_arguments.buffer(), 0, &readback, 0, bytes);
        let index = submission.submit(&self.queue);
        let bytes = readback.read(&self.queue, index, bytes);
        bytes
            .as_chunks::<ARGUMENT_USIZE>()
            .0
            .iter()
            .map(|held| u32::from_ne_bytes(held[4..8].try_into().unwrap()))
            .sum()
    }

    pub fn dropped_transparent_fragments(&self) -> u32 {
        let Some(fragments) = self.transparency.dropped_buffer() else {
            return 0;
        };
        let readback = GpuBuffer::new(
            &self.device,
            "dome dropped readback",
            WORD_BYTES,
            BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        );
        let mut submission = Submission::new(&self.device, "dome dropped");
        submission.copy(fragments, 0, &readback, 0, WORD_BYTES);
        let index = submission.submit(&self.queue);
        let bytes = readback.read(&self.queue, index, WORD_BYTES);
        u32::from_ne_bytes(bytes[..WORD_BYTES as usize].try_into().unwrap())
    }
}

const _: () = assert!(
    JOINT_BYTES == JOINT.size as u64,
    "the joint record keeps the layout its shader reads",
);
const _: () = assert!(
    LIGHT_BYTES == size_of::<LightRecord>() as u64,
    "the light record keeps the layout its shader reads",
);
const _: () = assert!(
    LIGHT.size as u64 == LIGHT_BYTES,
    "the light layout spans its record",
);
const _: () = assert!(
    CASCADE_BYTES == size_of::<CascadeRecord>() as u64,
    "the cascade record keeps the layout its shader reads",
);
const _: () = assert!(
    crate::abi::CASCADE.size as u64 == CASCADE_BYTES,
    "the cascade layout spans its record",
);
const _: () = assert!(
    MATERIAL_BYTES == size_of::<MaterialRecord>() as u64,
    "the material record keeps the layout its shader reads",
);
const _: () = assert!(
    MATERIAL.size as u64 == MATERIAL_BYTES,
    "the material layout spans its record",
);
const _: () = assert!(
    POINT_LIGHT_BYTES == size_of::<PointLightRecord>() as u64,
    "the point light record keeps the layout its shader reads",
);
const _: () = assert!(
    POINT_LIGHT.size as u64 == POINT_LIGHT_BYTES,
    "the point light layout spans its record",
);
const _: () = assert!(
    POINT_LIGHT_BYTES == 12 * WORD_BYTES,
    "a point light record spans the twelve words its shader indexes",
);
const _: () = assert!(
    CLUSTER_BYTES == (LIGHT_CAPACITY + 1) as u64 * WORD_BYTES,
    "a light cluster keeps its count and every light beside it",
);
const _: () = assert!(
    VIEW_BYTES == 256,
    "a frame keeps its main and culling views on a storage binding boundary",
);
