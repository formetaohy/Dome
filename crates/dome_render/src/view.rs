use dome_math::{Mat4, Vec4};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub clip: Mat4,
    pub planes: [Vec4; 6],
}

impl View {
    pub(crate) fn of(clip: Mat4) -> Self {
        Self {
            clip,
            planes: planes(clip),
        }
    }
}

fn planes(clip: Mat4) -> [Vec4; 6] {
    let [left, right, bottom, top, near, far] = [
        clip.row(3) + clip.row(0),
        clip.row(3) - clip.row(0),
        clip.row(3) + clip.row(1),
        clip.row(3) - clip.row(1),
        clip.row(2),
        clip.row(3) - clip.row(2),
    ];
    [left, right, bottom, top, near, far].map(normalized)
}

fn normalized(plane: Vec4) -> Vec4 {
    let length = plane.truncate().length();
    assert!(
        length > 0.0,
        "a frustum plane of no extent names no half space"
    );
    plane / length
}
