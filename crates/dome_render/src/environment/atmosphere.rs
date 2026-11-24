use crate::abi::Fvec4;
use crate::abi::{ATMOSPHERE_RECORDS, AtmosphereRecord};
use crate::frame::vertical;
use crate::program;
use dome_gpu::{
    AddressMode, BindGroup, Binding, BufferUsages, Device, Draw, FilterMode, Gpu, GpuBuffer,
    GraphicsPipeline, Pass, PipelineState, Queue, Sampler, SamplerDesc, Submission, Texture,
    TextureFormat, TextureUsages,
};
use dome_math::Vec3;
use std::mem::size_of;

pub const MODEL_VERSION: u32 = 2;
pub const PANORAMA_WIDTH: u32 = 1024;
pub const PANORAMA_HEIGHT: u32 = PANORAMA_WIDTH / 2;
pub const TRANSMITTANCE_WIDTH: u32 = 256;
pub const TRANSMITTANCE_HEIGHT: u32 = 64;
pub const SCATTERING_WIDTH: u32 = 32;
pub const SCATTERING_HEIGHT: u32 = 32;

const THICKNESS_KM: f32 = 60.0;
const OBSERVER_ALTITUDE_KM: f32 = 0.001;
const RAYLEIGH_SCATTERING_KM: Vec3 = Vec3::new(0.005_802, 0.013_558, 0.033_1);
const MIE_SCATTERING_KM: f32 = 0.003_996;
const MIE_ABSORPTION_KM: f32 = 0.004_44;
const OZONE_ABSORPTION_KM: Vec3 = Vec3::new(0.000_65, 0.001_881, 0.000_085);
const RECORD_BYTES: u64 = size_of::<AtmosphereRecord>() as u64;
const FULLSCREEN_VERTICES: u32 = 3;
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub sun_elevation: f32,
    pub sun_azimuth: f32,
    pub sun_intensity: f32,
    pub rayleigh: f32,
    pub mie: f32,
    pub mie_anisotropy: f32,
    pub ozone: f32,
    pub ground_albedo: Vec3,
    pub altitude: f32,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self {
            sun_elevation: 40.0,
            sun_azimuth: 0.0,
            sun_intensity: 3.0,
            rayleigh: 1.0,
            mie: 1.0,
            mie_anisotropy: 0.8,
            ozone: 1.0,
            ground_albedo: Vec3::splat(0.1),
            altitude: 0.0,
        }
    }
}

impl Atmosphere {
    pub fn sun(&self) -> Vec3 {
        let elevation = self.sun_elevation.to_radians();
        let azimuth = self.sun_azimuth.to_radians();
        Vec3::new(
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
            -elevation.cos() * azimuth.cos(),
        )
    }

    fn parameters(&self) -> Parameters {
        assert!(
            self.sun_elevation.is_finite() && (-90.0..=90.0).contains(&self.sun_elevation),
            "an atmosphere holds the sun elevation {} beyond the sky",
            self.sun_elevation,
        );
        assert!(
            self.sun_azimuth.is_finite(),
            "an atmosphere holds the sun azimuth {}",
            self.sun_azimuth,
        );
        assert!(
            self.sun_intensity > 0.0 && self.sun_intensity.is_finite(),
            "an atmosphere of intensity {} lights nothing",
            self.sun_intensity,
        );
        assert!(
            self.rayleigh >= 0.0 && self.rayleigh.is_finite(),
            "an atmosphere scatters {} of its Rayleigh light",
            self.rayleigh,
        );
        assert!(
            self.mie >= 0.0 && self.mie.is_finite(),
            "an atmosphere scatters {} of its Mie light",
            self.mie,
        );
        assert!(
            self.mie_anisotropy.is_finite() && self.mie_anisotropy.abs() < 1.0,
            "an atmosphere scatters Mie light {} forward",
            self.mie_anisotropy,
        );
        assert!(
            self.ozone >= 0.0 && self.ozone.is_finite(),
            "an atmosphere absorbs {} of its ozone light",
            self.ozone,
        );
        assert!(
            (0.0..=1.0).contains(&self.ground_albedo.x)
                && (0.0..=1.0).contains(&self.ground_albedo.y)
                && (0.0..=1.0).contains(&self.ground_albedo.z),
            "an atmosphere reflects its ground at {:?}",
            self.ground_albedo,
        );
        assert!(
            self.altitude >= 0.0
                && self.altitude.is_finite()
                && self.altitude < THICKNESS_KM * 1000.0,
            "an atmosphere sits {} meters above its ground",
            self.altitude,
        );
        assert!(
            self.mie > 0.0 || self.ozone > 0.0,
            "an atmosphere of mie {} and ozone {} scatters every photon it gathers",
            self.mie,
            self.ozone,
        );
        Parameters {
            rayleigh: RAYLEIGH_SCATTERING_KM * self.rayleigh,
            mie: Vec3::splat(MIE_SCATTERING_KM * self.mie),
            mie_absorption: Vec3::splat(MIE_ABSORPTION_KM * self.mie),
            ozone: OZONE_ABSORPTION_KM * self.ozone,
            mie_anisotropy: self.mie_anisotropy,
            ground_albedo: self.ground_albedo,
            altitude: self.altitude / 1000.0,
        }
    }

    pub(crate) fn extinction(&self) -> (Vec3, Vec3, Vec3) {
        let held = self.parameters();
        (held.rayleigh, held.mie, held.mie_absorption)
    }

    fn record(&self, vertical: f32, disk: bool) -> AtmosphereRecord {
        let held = self.parameters();
        AtmosphereRecord {
            rayleigh: vector(held.rayleigh),
            mie: vector(held.mie),
            absorption: vector(held.mie_absorption),
            ozone: vector(held.ozone),
            ground: vector(held.ground_albedo),
            sun: vector(self.sun()),
            altitude: held.altitude.max(OBSERVER_ALTITUDE_KM),
            anisotropy: held.mie_anisotropy,
            vertical,
            width: PANORAMA_WIDTH as f32,
            height: PANORAMA_HEIGHT as f32,
            disk: u32::from(disk),
            pad0: 0,
            pad1: 0,
        }
    }
}

struct Parameters {
    rayleigh: Vec3,
    mie: Vec3,
    mie_absorption: Vec3,
    ozone: Vec3,
    mie_anisotropy: f32,
    ground_albedo: Vec3,
    altitude: f32,
}

pub struct AtmospherePanorama {
    pub radiance: Texture,
    pub background: Texture,
}

pub(crate) struct AtmosphereBaker {
    device: Device,
    queue: Queue,
    transmittance: Texture,
    scattering: Texture,
    transmittance_pass: GraphicsPipeline,
    scattering_pass: GraphicsPipeline,
    sky: GraphicsPipeline,
    setup: GpuBuffer,
    setup_span: u64,
    transmittance_group: BindGroup,
    scattering_group: BindGroup,
    radiance_group: BindGroup,
    background_group: BindGroup,
    scattering_source: BindGroup,
    sky_source: BindGroup,
}

impl AtmosphereBaker {
    pub(crate) fn new(context: &Gpu) -> Self {
        let device = context.device().clone();
        let queue = Queue::of(&device);
        let lut = TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
        let transmittance = Texture::new(
            &device,
            "dome atmosphere transmittance",
            TRANSMITTANCE_WIDTH,
            TRANSMITTANCE_HEIGHT,
            1,
            TextureFormat::Rgba16Float,
            lut,
        );
        let scattering = Texture::new(
            &device,
            "dome atmosphere scattering",
            SCATTERING_WIDTH,
            SCATTERING_HEIGHT,
            1,
            TextureFormat::Rgba16Float,
            lut,
        );
        let sampler = Sampler::new(
            &device,
            SamplerDesc {
                min_filter: FilterMode::Linear,
                mag_filter: FilterMode::Linear,
                mipmap_filter: FilterMode::Nearest,
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                compare: None,
            },
        );
        let transmittance_pass = GraphicsPipeline::new(
            &device,
            program::atmosphere_transmittance_program(),
            TextureFormat::Rgba16Float,
            PipelineState::SCREEN,
        );
        let scattering_pass = GraphicsPipeline::new(
            &device,
            program::atmosphere_scattering_program(),
            TextureFormat::Rgba16Float,
            PipelineState::SCREEN,
        );
        let sky = GraphicsPipeline::new(
            &device,
            program::atmosphere_sky_program(),
            TextureFormat::Rgba16Float,
            PipelineState::SCREEN,
        );
        let setup_span = crate::span::padded(device.binding_alignment(), RECORD_BYTES);
        let setup = GpuBuffer::new(
            &device,
            "dome atmosphere setup",
            setup_span * u64::from(ATMOSPHERE_RECORDS),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let record = |index: u64| setup.binding(index * setup_span, RECORD_BYTES);
        let transmittance_group = transmittance_pass
            .bind_group(0, &[Binding::buffer(program::ATMOSPHERE_SETUP, record(0))]);
        let scattering_group =
            scattering_pass.bind_group(0, &[Binding::buffer(program::ATMOSPHERE_SETUP, record(1))]);
        let radiance_group =
            sky.bind_group(0, &[Binding::buffer(program::ATMOSPHERE_SETUP, record(2))]);
        let background_group =
            sky.bind_group(0, &[Binding::buffer(program::ATMOSPHERE_SETUP, record(3))]);
        let scattering_source = scattering_pass.bind_group(
            1,
            &[
                Binding::texture(program::ATMOSPHERE_TRANSMITTANCE, &transmittance),
                Binding::sampler(program::ATMOSPHERE_LUT_SAMPLER, &sampler),
            ],
        );
        let sky_source = sky.bind_group(
            1,
            &[
                Binding::texture(program::ATMOSPHERE_TRANSMITTANCE, &transmittance),
                Binding::texture(program::ATMOSPHERE_SCATTERING, &scattering),
                Binding::sampler(program::ATMOSPHERE_SKY_SAMPLER, &sampler),
            ],
        );
        Self {
            device,
            queue,
            transmittance,
            scattering,
            transmittance_pass,
            scattering_pass,
            sky,
            setup,
            setup_span,
            transmittance_group,
            scattering_group,
            radiance_group,
            background_group,
            scattering_source,
            sky_source,
        }
    }

    pub(crate) fn bake(&self, atmosphere: &Atmosphere) -> AtmospherePanorama {
        let vertical = vertical();
        let records = [
            atmosphere.record(vertical, false),
            atmosphere.record(vertical, false),
            atmosphere.record(vertical, false),
            atmosphere.record(vertical, true),
        ];
        let mut bytes = vec![0u8; (self.setup_span * u64::from(ATMOSPHERE_RECORDS)) as usize];
        for (index, record) in records.iter().enumerate() {
            let start = index * self.setup_span as usize;
            bytes[start..start + RECORD_BYTES as usize].copy_from_slice(bytemuck::bytes_of(record));
        }
        self.setup.write(&self.queue, &bytes);
        let panorama = || {
            Texture::new(
                &self.device,
                "dome atmosphere panorama",
                PANORAMA_WIDTH,
                PANORAMA_HEIGHT,
                1,
                TextureFormat::Rgba16Float,
                TextureUsages::TEXTURE_BINDING
                    | TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::COPY_SRC,
            )
        };
        let radiance = panorama();
        let background = panorama();
        let mut submission = Submission::new(&self.device, "dome atmosphere");
        submission.render(
            Pass::new(&self.transmittance, CLEAR),
            &[Draw::new(
                &self.transmittance_pass,
                std::slice::from_ref(&self.transmittance_group),
                FULLSCREEN_VERTICES,
            )],
        );
        submission.render(
            Pass::new(&self.scattering, CLEAR),
            &[Draw::new(
                &self.scattering_pass,
                &[
                    self.scattering_group.clone(),
                    self.scattering_source.clone(),
                ],
                FULLSCREEN_VERTICES,
            )],
        );
        submission.render(
            Pass::new(&radiance, CLEAR),
            &[Draw::new(
                &self.sky,
                &[self.radiance_group.clone(), self.sky_source.clone()],
                FULLSCREEN_VERTICES,
            )],
        );
        submission.render(
            Pass::new(&background, CLEAR),
            &[Draw::new(
                &self.sky,
                &[self.background_group.clone(), self.sky_source.clone()],
                FULLSCREEN_VERTICES,
            )],
        );
        submission.submit(&self.queue);
        AtmospherePanorama {
            radiance,
            background,
        }
    }
}

fn vector(value: Vec3) -> Fvec4 {
    Fvec4 {
        x: value.x,
        y: value.y,
        z: value.z,
        w: 0.0,
    }
}
