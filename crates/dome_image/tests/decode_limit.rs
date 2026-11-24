use dome_image::{Image, Radiance};

const OVERSIZED: [u8; 45] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 64, 0, 0, 0, 64, 0, 8, 6,
    0, 0, 0, 169, 200, 16, 132, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

const UNREACHABLE: [u8; 45] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 8, 0, 0, 0, 8, 0, 8, 6, 0,
    0, 0, 178, 167, 211, 48, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

const ABSURD: &[u8] = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 65535 +X 65535\n";

const TRUNCATED: &[u8] = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 64 +X 64\n";

#[test]
fn a_decode_refuses_a_picture_beyond_its_budget_image_data_or_scanlines() {
    let error = Image::decode(&OVERSIZED, "the oversized png")
        .err()
        .expect("the oversized png decodes");
    assert!(error.contains("texels"), "{error}");

    let error = Image::decode(&UNREACHABLE, "the unreachable png")
        .err()
        .expect("the unreachable png decodes");
    assert!(error.contains("image data"), "{error}");

    let error = Radiance::decode(ABSURD, "the absurd radiance")
        .err()
        .expect("the absurd radiance picture decodes");
    assert!(error.contains("texels"), "{error}");

    let error = Radiance::decode(TRUNCATED, "the truncated radiance")
        .err()
        .expect("the truncated radiance picture decodes");
    assert!(error.contains("need at least"), "{error}");
}
