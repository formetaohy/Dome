@group(0) @binding(0) var<storage, read> view: array<View>;
@group(0) @binding(1) var<storage, read> palette: array<Joint>;
@group(0) @binding(2) var<storage, read> skins: array<SkinVertex>;
@group(0) @binding(3) var<storage, read> visible: array<u32>;
@group(0) @binding(4) var<storage, read> vertices: array<f32>;
@group(0) @binding(5) var<storage, read> instances: array<Instance>;
