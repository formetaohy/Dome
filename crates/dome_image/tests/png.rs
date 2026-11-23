use dome_image::{Image, encode};

const RGBA: [u8; 75] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0,
    0, 0, 114, 182, 13, 36, 0, 0, 0, 18, 73, 68, 65, 84, 120, 218, 99, 248, 207, 192, 240, 31, 12,
    129, 52, 24, 0, 0, 73, 200, 9, 247, 3, 217, 100, 241, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
    130,
];

const PALETTE: [u8; 104] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 1, 2, 3, 0,
    0, 0, 132, 82, 231, 94, 0, 0, 0, 12, 80, 76, 84, 69, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100,
    110, 120, 198, 72, 119, 223, 0, 0, 0, 1, 116, 82, 78, 83, 128, 173, 94, 91, 70, 0, 0, 0, 10,
    73, 68, 65, 84, 120, 218, 99, 144, 6, 0, 0, 29, 0, 28, 35, 124, 143, 172, 0, 0, 0, 0, 73, 69,
    78, 68, 174, 66, 96, 130,
];

const INTERLACED: [u8; 104] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 3, 0, 0, 0, 3, 8, 6, 0,
    0, 1, 33, 47, 133, 41, 0, 0, 0, 47, 73, 68, 65, 84, 120, 218, 99, 224, 18, 97, 248, 207, 16,
    37, 98, 243, 159, 129, 171, 199, 230, 127, 84, 79, 197, 127, 6, 35, 17, 57, 32, 209, 19, 5, 20,
    10, 144, 251, 111, 20, 0, 20, 14, 136, 250, 15, 0, 41, 100, 15, 166, 30, 122, 25, 227, 0, 0, 0,
    0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[test]
fn png_encodes_truecolour_images() {
    let pixels = (0..4u8 * 3 * 4)
        .map(|value| value.wrapping_mul(5))
        .collect::<Vec<_>>();
    let image = Image::decode(&encode(4, 3, &pixels), "the encoded test");
    assert_eq!((image.width(), image.height()), (4, 3));
    assert_eq!(image.rgba(), pixels);
}

#[test]
fn png_decodes_packed_paletted_interlaced_and_truecolour_images() {
    let image = Image::decode(&RGBA, "the test");
    assert_eq!((image.width(), image.height()), (2, 2));
    assert_eq!(
        image.rgba(),
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255
        ],
    );

    let image = Image::decode(&PALETTE, "the test");
    assert_eq!((image.width(), image.height()), (4, 1));
    assert_eq!(
        image.rgba(),
        &[
            10, 20, 30, 128, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255
        ],
    );

    let image = Image::decode(&INTERLACED, "the test");
    assert_eq!((image.width(), image.height()), (3, 3));
    let expected = (0..3)
        .flat_map(|y| (0..3).flat_map(move |x| [x * 40 + 10, y * 60 + 20, (x + y) * 30, 255]))
        .collect::<Vec<u8>>();
    assert_eq!(image.rgba(), expected);
}
