use dome_image::Radiance;

const HEADER: &str = "#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n";

#[test]
fn a_radiance_picture_decodes_flat_and_run_length_scanlines() {
    let mut flat = HEADER.as_bytes().to_vec();
    flat.extend_from_slice(b"-Y 1 +X 4\n");
    flat.extend_from_slice(&[
        128, 64, 32, 128, 128, 64, 32, 128, 128, 64, 32, 128, 128, 64, 32, 128,
    ]);
    let decoded = Radiance::decode(&flat, "a flat picture");
    assert_eq!((decoded.width(), decoded.height()), (4, 1));
    assert_eq!(decoded.rgb()[..3], [0.5, 0.25, 0.125]);

    let mut runs = HEADER.as_bytes().to_vec();
    runs.extend_from_slice(b"-Y 2 +X 16\n");
    for _ in 0..2 {
        runs.extend_from_slice(&[2, 2, 0, 16]);
        for channel in [160u8, 128, 96, 136] {
            runs.extend_from_slice(&[128 + 16, channel]);
        }
    }
    let decoded = Radiance::decode(&runs, "a run picture");
    assert_eq!((decoded.width(), decoded.height()), (16, 2));
    assert_eq!(decoded.rgb()[..3], [160.0, 128.0, 96.0]);
    assert_eq!(decoded.rgb().len(), 16 * 2 * 3);

    let mut repeats = HEADER.as_bytes().to_vec();
    repeats.extend_from_slice(b"-Y 1 +X 4\n");
    repeats.extend_from_slice(&[128, 64, 32, 128, 1, 1, 1, 3]);
    let decoded = Radiance::decode(&repeats, "a repeated picture");
    assert_eq!(decoded.rgb(), [0.5, 0.25, 0.125].repeat(4));

    let mut exposed = "#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nEXPOSURE=2\nEXPOSURE=2\n\n"
        .as_bytes()
        .to_vec();
    exposed.extend_from_slice(b"-Y 1 +X 4\n");
    exposed.extend_from_slice(&[
        128, 64, 32, 128, 128, 64, 32, 128, 128, 64, 32, 128, 128, 64, 32, 128,
    ]);
    let decoded = Radiance::decode(&exposed, "an exposed picture");
    assert_eq!(decoded.rgb()[..3], [0.125, 0.0625, 0.03125]);

    let mut ambiguous = HEADER.as_bytes().to_vec();
    ambiguous.extend_from_slice(b"-Y 1 +X 4\n");
    ambiguous.extend_from_slice(&[
        2, 2, 200, 128, 128, 64, 32, 128, 128, 64, 32, 128, 128, 64, 32, 128,
    ]);
    let decoded = Radiance::decode(&ambiguous, "an ambiguous picture");
    assert_eq!(decoded.rgb()[9..], [0.5, 0.25, 0.125]);
}
