use dome_image::{ColorSpace, mip_chain};

const CHECKER: [u8; 16] = [
    0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
];

const CORNER: [u8; 16] = [255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

#[test]
fn a_mip_chain_filters_color_in_linear_light_and_keeps_its_extents() {
    let srgb = mip_chain(2, 2, ColorSpace::Srgb, &CHECKER);
    assert_eq!(srgb.len(), 2);
    assert_eq!(srgb[1], [188, 188, 188, 255]);

    let linear = mip_chain(2, 2, ColorSpace::Linear, &CHECKER);
    assert_eq!(linear[1], [128, 128, 128, 255]);

    let covered = mip_chain(2, 2, ColorSpace::Srgb, &CORNER);
    assert_eq!(covered[1], [255, 0, 0, 64]);

    let hidden = [255u8, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 255, 255, 255, 0];
    let preserved = mip_chain(2, 2, ColorSpace::Srgb, &hidden);
    assert_eq!(preserved[1], [188, 137, 188, 0]);

    let odd = vec![255u8; 3 * 5 * 4];
    let levels = mip_chain(3, 5, ColorSpace::Linear, &odd);
    assert_eq!(levels.len(), 3);
    assert_eq!(levels[1].len(), 2 * 4);
    assert_eq!(levels[2].len(), 4);
}
