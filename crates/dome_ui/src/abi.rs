#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Fvec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uvec4 {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
}

macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
        pub struct $name {
            $(pub $field: $ty),*
        }
    };
}

record!(UiInstanceRecord {
    rect: Fvec4,
    corners: Fvec4,
    color: Fvec4,
    color2: Fvec4,
    uv: Fvec4,
    clip: Fvec4,
    clip_corners: Fvec4,
    meta: crate::abi::Uvec4,
    extra: Fvec4,
});

record!(UiScreenRecord {
    width: f32,
    height: f32,
    scale: f32,
    vertical: f32,
});

#[cfg(test)]
mod abi_layout {
    use super::*;

    #[test]
    fn the_shader_records_match_their_rust_layout() {
        let source = include_str!("wgsl/paint.wgsl");
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("the shader records parse: {error}"));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("the shader records validate: {error}"));
        let expected = [
            ("UiInstance", std::mem::size_of::<UiInstanceRecord>()),
            ("UiScreen", std::mem::size_of::<UiScreenRecord>()),
        ];
        for (name, bytes) in expected {
            let span = module
                .types
                .iter()
                .find_map(|(_, ty)| match &ty.inner {
                    naga::TypeInner::Struct { span, .. } if ty.name.as_deref() == Some(name) => {
                        Some(*span)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("the shader declares no {name} record"));
            assert_eq!(
                u64::from(span),
                bytes as u64,
                "the shader record {name} spans {span} bytes where its Rust layout spans {bytes}",
            );
        }
    }
}
