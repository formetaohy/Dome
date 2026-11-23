use dome_gpu::Fvec4;
use dome_gpu::record;

record!(UiInstanceRecord, UI_INSTANCE, "UiInstance" {
    rect: Fvec4 => F32x4,
    corners: Fvec4 => F32x4,
    color: Fvec4 => F32x4,
    color2: Fvec4 => F32x4,
    uv: Fvec4 => F32x4,
    clip: Fvec4 => F32x4,
    clip_corners: Fvec4 => F32x4,
    meta: dome_gpu::Uvec4 => U32x4,
    extra: Fvec4 => F32x4,
});

record!(UiScreenRecord, UI_SCREEN, "UiScreen" {
    width: f32 => F32,
    height: f32 => F32,
    scale: f32 => F32,
    vertical: f32 => F32,
});
