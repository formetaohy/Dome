@group(0) @binding(0) var<storage, read> partials: array<f32>;
@group(0) @binding(1) var<storage, read_write> total: array<f32>;

@compute @workgroup_size(64)
fn compute(@builtin(local_invocation_index) lid: u32) {
    if (lid < 27u) {
        let at = (lid / 3u) * 4u + lid % 3u;
        var sum = 0.0;
        for (var index = 0u; index < 1024u; index = index + 1u) {
            sum += partials[index * 36u + at];
        }
        total[lid] = sum;
    }
}
