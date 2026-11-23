use dome_ecs::World;
use dome_math::{Transform, Vec3, Vec4};
use dome_render::{Environment, Hidden, Lines};
use std::f32::consts::TAU;

pub(crate) fn viewport(world: &World, physics_debug: bool) -> Lines {
    let mut lines = Lines::new();
    let extent = 14i32;
    let max_r = extent as f32;
    let ground_y = -0.5f32;
    for index in -extent..=extent {
        let fixed = index as f32;
        for step in -extent..extent {
            let s0 = step as f32;
            let s1 = (step + 1) as f32;
            let mid = (s0 + s1) * 0.5;
            let dist = (fixed * fixed + mid * mid).sqrt();
            if dist >= max_r {
                continue;
            }
            let t = (dist / max_r).clamp(0.0, 1.0);
            let fade = (1.0 - t * t) * (1.0 - t);
            let x_color = if index == 0 {
                Vec4::new(0.88, 0.32, 0.38, 0.52 * fade)
            } else if index % 2 == 0 {
                Vec4::new(0.46, 0.56, 0.74, 0.46 * fade)
            } else {
                Vec4::new(0.34, 0.42, 0.58, 0.26 * fade)
            };
            lines.push(
                Vec3::new(s0, ground_y, fixed),
                Vec3::new(s1, ground_y, fixed),
                x_color,
            );
            let z_color = if index == 0 {
                Vec4::new(0.28, 0.62, 0.96, 0.52 * fade)
            } else if index % 2 == 0 {
                Vec4::new(0.46, 0.56, 0.74, 0.46 * fade)
            } else {
                Vec4::new(0.34, 0.42, 0.58, 0.26 * fade)
            };
            lines.push(
                Vec3::new(fixed, ground_y, s0),
                Vec3::new(fixed, ground_y, s1),
                z_color,
            );
        }
    }
    let segments = 72usize;
    for (radius, height, color) in [
        (12.0f32, 1.2f32, Vec4::new(0.38, 0.52, 0.74, 0.24)),
        (12.0f32, 2.8f32, Vec4::new(0.34, 0.48, 0.70, 0.18)),
    ] {
        for step in 0..segments {
            let a0 = (step as f32 / segments as f32) * TAU;
            let a1 = ((step + 1) as f32 / segments as f32) * TAU;
            lines.push(
                Vec3::new(a0.cos() * radius, height, a0.sin() * radius),
                Vec3::new(a1.cos() * radius, height, a1.sin() * radius),
                color,
            );
        }
    }
    for (entity, environment) in world.iter::<Environment>() {
        if !environment.local || world.contains::<Hidden>(entity) {
            continue;
        }
        let transform = world
            .get::<Transform>(entity)
            .copied()
            .unwrap_or(Transform::IDENTITY);
        let extent = transform.scale;
        let corner = |x: f32, y: f32, z: f32| {
            transform.transform_point(Vec3::new(extent.x * x, extent.y * y, extent.z * z))
        };
        let color = Vec4::new(0.52, 0.70, 1.0, 0.62);
        for (start, end) in [
            (corner(-1.0, -1.0, -1.0), corner(1.0, -1.0, -1.0)),
            (corner(1.0, -1.0, -1.0), corner(1.0, 1.0, -1.0)),
            (corner(1.0, 1.0, -1.0), corner(-1.0, 1.0, -1.0)),
            (corner(-1.0, 1.0, -1.0), corner(-1.0, -1.0, -1.0)),
            (corner(-1.0, -1.0, 1.0), corner(1.0, -1.0, 1.0)),
            (corner(1.0, -1.0, 1.0), corner(1.0, 1.0, 1.0)),
            (corner(1.0, 1.0, 1.0), corner(-1.0, 1.0, 1.0)),
            (corner(-1.0, 1.0, 1.0), corner(-1.0, -1.0, 1.0)),
            (corner(-1.0, -1.0, -1.0), corner(-1.0, -1.0, 1.0)),
            (corner(1.0, -1.0, -1.0), corner(1.0, -1.0, 1.0)),
            (corner(1.0, 1.0, -1.0), corner(1.0, 1.0, 1.0)),
            (corner(-1.0, 1.0, -1.0), corner(-1.0, 1.0, 1.0)),
        ] {
            lines.push(start, end, color);
        }
    }
    if physics_debug {
        for line in dome_physics::debug_lines(world) {
            lines.push(line.start, line.end, line.color);
        }
    }
    lines
}
