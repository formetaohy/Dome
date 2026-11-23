use dome_animation::Clip;
use dome_audio::Source;
use dome_physics::TriangleMesh;
use dome_render::{CaptureProbe, Environment, Material, Mesh};
use dome_scene::SkinData;
use std::sync::Arc;

pub trait Assets {
    fn mesh(&self, id: &str) -> Mesh;

    fn environment(&self, id: &str) -> Environment {
        panic!("the assets hold no environment {id}")
    }

    fn capture(&self, _id: &str) -> Option<CaptureProbe> {
        None
    }

    fn material(&self, id: &str) -> Material;

    fn skin(&self, id: &str) -> SkinData;

    fn animation(&self, id: &str) -> Arc<Clip>;

    fn mesh_shape(&self, id: &str) -> Arc<TriangleMesh> {
        panic!("the assets hold no mesh shape {id}")
    }

    fn sound(&self, id: &str) -> Source {
        panic!("the assets hold no sound {id}")
    }
}
