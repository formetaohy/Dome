use super::*;
use crate::camera::Camera;
use crate::environment::Environment;

pub(super) struct FrameRecord<'a> {
    pub(super) texture: &'a Texture,
    pub(super) clear: [f32; 4],
    pub(super) view: &'a View,
    pub(super) eye: Vec3,
    pub(super) forward: Vec3,
    pub(super) camera: Camera,
    pub(super) transform: Transform,
    pub(super) light: crate::light::DirectionalLight,
    pub(super) light_transform: Transform,
    pub(super) environment: Option<&'a Environment>,
    pub(super) environment_transform: Transform,
    pub(super) vertical: f32,
    pub(super) aspect: f32,
    pub(super) point_lights: &'a [PointLightRecord],
    pub(super) probe_count: u32,
    pub(super) spheres: &'a [(Vec3, f32)],
    pub(super) total: u32,
    pub(super) highlight: u32,
    pub(super) world: &'a World,
}

pub(super) struct FrameShared {
    pub(super) travel: Travel,
    pub(super) line_groups: Vec<BindGroup>,
    pub(super) line_vertices: u32,
    pub(super) sky: bool,
    pub(super) cluster_offset_bytes: u64,
    pub(super) cluster_bytes: u64,
    pub(super) cluster_x: u32,
    pub(super) cluster_y: u32,
    pub(super) lighting_bytes: u64,
}

impl Renderer {
    pub(super) fn record(&mut self, frame: FrameRecord<'_>) -> FrameShared {
        let FrameRecord {
            texture,
            clear,
            view,
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
            point_lights,
            probe_count,
            spheres,
            total,
            highlight,
            world,
        } = frame;
        let _ = clear;
        let travel = Travel::of(light_transform);
        let cascades = shadow::cascades(
            camera,
            transform,
            aspect,
            light,
            &travel,
            spheres,
            self.device.clip_space(),
        );
        let direction = travel.direction();
        let radiance = light.color * light.intensity;
        let hzb_levels = if total == 0 {
            0u32
        } else {
            self.ensure_hzb(texture.width(), texture.height(), vertical);
            let hzb = self
                .hzb
                .as_ref()
                .expect("a rendered frame owns its depth pyramid");
            hzb.levels()
        };
        let cluster_x = texture.width().div_ceil(LIGHT_TILE);
        let cluster_y = texture.height().div_ceil(LIGHT_TILE);
        let cluster_count = cluster_x * cluster_y * LIGHT_SLICES;
        let cluster_bytes = u64::from(cluster_count) * CLUSTER_BYTES;
        assert!(
            cluster_bytes <= self.device.limits().max_storage_buffer_binding_size,
            "a {} by {} frame of {cluster_bytes} cluster bytes outruns the device storage binding",
            texture.width(),
            texture.height(),
        );
        assert!(
            cluster_x * cluster_y <= self.device.limits().max_compute_workgroups_per_dimension,
            "a {} by {} frame dispatches {} light tiles beyond the device grid",
            texture.width(),
            texture.height(),
            cluster_x * cluster_y,
        );
        let point_bytes = u64::from(point_lights.len() as u32) * POINT_LIGHT_BYTES;
        let alignment = self
            .device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let cluster_offset_bytes = point_bytes.next_multiple_of(alignment);
        let cluster_offset = (cluster_offset_bytes / WORD_BYTES) as u32;
        let lighting_bytes = cluster_offset_bytes + cluster_bytes;
        assert!(
            lighting_bytes <= self.device.limits().max_storage_buffer_binding_size,
            "a {} by {} frame of {lighting_bytes} lighting bytes outruns the device storage binding",
            texture.width(),
            texture.height(),
        );
        self.lighting
            .fit(&self.device, lighting_bytes.max(POINT_LIGHT_BYTES));
        self.lighting
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(point_lights));
        let palette_base = self.palettes.base();
        let main_view = record_of_main_view(
            view,
            total,
            eye,
            forward,
            palette_base,
            camera,
            Screen {
                width: texture.width(),
                height: texture.height(),
                vertical,
                hzb_levels,
                cluster_x,
                cluster_y,
                point_lights: point_lights.len() as u32,
                cluster_offset,
                highlight,
            },
        );
        let depth_view = ViewRecord {
            hzb_levels: 0,
            instances: total,
            ..main_view
        };
        self.views
            .write(&self.queue, bytemuck::cast_slice(&[main_view, depth_view]));
        self.shadow_views.write(
            &self.queue,
            bytemuck::cast_slice(
                &cascades
                    .iter()
                    .map(|cascade| {
                        record_of_view(
                            &View::of(cascade.clip),
                            total,
                            Vec3::ZERO,
                            Vec3::Z,
                            palette_base,
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
        );
        self.cascades.write(
            &self.queue,
            bytemuck::cast_slice(&cascades.iter().map(record_of_cascade).collect::<Vec<_>>()),
        );
        let irradiance = environment.map_or([Vec3::ZERO; 9], |held| held.irradiance);
        let intensity = environment.map_or(0.0, |held| held.intensity);
        let levels = environment.map_or(0.0, |held| (held.levels - 1) as f32);
        let (rayleigh, mie, absorption) = environment
            .and_then(|held| held.atmosphere)
            .map_or((Vec3::ZERO, Vec3::ZERO, Vec3::ZERO), |held| {
                held.extinction()
            });
        let environment_rotation =
            Mat4::from_quat(environment_transform.rotation.normalize().inverse());
        self.light.write(
            &self.queue,
            bytemuck::bytes_of(&LightRecord {
                direction: direction_record(direction),
                radiance: vector(radiance.extend(0.0)),
                irradiance0: vector(irradiance[0].extend(0.0)),
                irradiance1: vector(irradiance[1].extend(0.0)),
                irradiance2: vector(irradiance[2].extend(0.0)),
                irradiance3: vector(irradiance[3].extend(0.0)),
                irradiance4: vector(irradiance[4].extend(0.0)),
                irradiance5: vector(irradiance[5].extend(0.0)),
                irradiance6: vector(irradiance[6].extend(0.0)),
                irradiance7: vector(irradiance[7].extend(0.0)),
                irradiance8: vector(irradiance[8].extend(0.0)),
                environment0: vector(environment_rotation.col(0)),
                environment1: vector(environment_rotation.col(1)),
                environment2: vector(environment_rotation.col(2)),
                rayleigh: vector(rayleigh.extend(0.0)),
                mie: vector(mie.extend(0.0)),
                absorption: vector(absorption.extend(0.0)),
                intensity,
                levels,
                vertical,
                probes: probe_count,
                pad0: 0,
                pad1: 0,
                pad2: 0,
                pad3: 0,
            }),
        );
        let sky = environment.filter(|held| held.background);
        if let Some(held) = sky {
            let inverse = view.clip.inverse();
            self.sky_record.write(
                &self.queue,
                bytemuck::bytes_of(&SkyRecord {
                    inverse0: vector(inverse.col(0)),
                    inverse1: vector(inverse.col(1)),
                    inverse2: vector(inverse.col(2)),
                    inverse3: vector(inverse.col(3)),
                    eye: vector(eye.extend(0.0)),
                    environment0: vector(environment_rotation.col(0)),
                    environment1: vector(environment_rotation.col(1)),
                    environment2: vector(environment_rotation.col(2)),
                    intensity: held.intensity,
                    pad0: 0,
                    pad1: 0,
                    pad2: 0,
                }),
            );
        }
        self.post
            .ensure(&self.device, &self.queue, texture.width(), texture.height());
        self.transparency.ensure(
            &self.device,
            &self.queue,
            texture.width(),
            texture.height(),
            vertical,
        );
        let debug = world.resource::<Lines>().filter(|lines| !lines.is_empty());
        let line_segments = debug.map_or(0, Lines::len);
        let mut line_groups = Vec::new();
        if let Some(lines) = debug {
            let bytes = line_segments as u64 * DEBUG_LINE_BYTES;
            self.line_records.fit(&self.device, bytes);
            let records = lines
                .lines()
                .iter()
                .map(|line| DebugLineRecord {
                    start: vector(line.start.extend(1.0)),
                    end: vector(line.end.extend(1.0)),
                    color: vector(line.color),
                })
                .collect::<Vec<_>>();
            self.line_records
                .buffer()
                .write(&self.queue, bytemuck::cast_slice(&records));
            line_groups.push(self.line_pipeline.bind_group(
                0,
                &[
                    Binding::buffer(program::LINE_VIEW, self.views.binding(0, VIEW_BYTES)),
                    Binding::buffer(
                        program::LINE_SEGMENTS,
                        self.line_records.buffer().binding(0, bytes),
                    ),
                ],
            ));
        }
        let line_vertices = line_segments as u32 * 6;
        FrameShared {
            travel,
            line_groups,
            line_vertices,
            sky: sky.is_some(),
            cluster_offset_bytes,
            cluster_bytes,
            cluster_x,
            cluster_y,
            lighting_bytes,
        }
    }
}
