use dome_gpu::{
    AddressMode, BufferUsages, FilterMode, Gpu, GpuBuffer, GpuRequest, SamplerDesc, Submission,
    Texture, TextureFormat, TextureUsages,
};
use dome_math::Vec2;
use dome_ui::{Color, Corners, Element, Key, Length, Level, TextStyle, Ui, canvas};

const SIDE: u32 = 160;
const BACKGROUND: Color = Color::rgb(0.1, 0.1, 0.1);
const SCENE: [u8; 4] = [0, 200, 0, 255];
const SAMPLER: SamplerDesc = SamplerDesc {
    min_filter: FilterMode::Nearest,
    mag_filter: FilterMode::Nearest,
    mipmap_filter: FilterMode::Nearest,
    address_mode_u: AddressMode::ClampToEdge,
    address_mode_v: AddressMode::ClampToEdge,
    compare: None,
};

#[test]
fn a_user_interface_paints_its_surfaces_glyphs_and_scene() {
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device paints the interface: {error}"));
    let device = context.device();
    let queue = context.queue();
    let format = TextureFormat::Rgba8Unorm;
    let mut ui: Ui<()> = Ui::new(device, &queue, format);
    let target = Texture::new(
        device,
        "dome ui target",
        SIDE,
        SIDE,
        1,
        format,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let backdrop = Texture::new(
        device,
        "dome ui backdrop",
        1,
        1,
        1,
        format,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    backdrop.write(&queue, &SCENE);

    ui.update(Vec2::splat(SIDE as f32), 1.0, 0.0);
    let element = {
        let ctx = ui.build();
        let theme = *ctx.theme();
        Element::column()
            .key(Key::new("root"))
            .width(Length::Fill)
            .height(Length::Fill)
            .background(dome_ui::Surface::fill(BACKGROUND))
            .children([
                Element::column()
                    .key(Key::new("glyph"))
                    .width(Length::Fixed(120.0))
                    .height(Length::Fixed(48.0))
                    .children([dome_ui::text(
                        String::from("H"),
                        TextStyle::new(theme.font(Level::Display), Color::WHITE),
                    )]),
                canvas(Length::Fixed(64.0), 64.0).view_static(|painter, info| {
                    painter.scene(info.rect, Corners::ZERO, [0.0, 0.0, 1.0, 1.0]);
                }),
            ])
    };
    ui.present(element);

    let mut submission = Submission::new(device, "dome ui paint");
    ui.render(&mut submission, &target, Some(&backdrop));
    let readback = GpuBuffer::new(
        device,
        "dome ui readback",
        target.readback_bytes(),
        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
    );
    submission.copy_texture(&target, &readback, 0);
    let index = submission.submit(&queue);
    let bytes = readback.read(&queue, index, target.readback_bytes());
    let pixel = |x: u32, y: u32| {
        let row = (u64::from(y) * target.row_bytes()) as usize;
        let at = row + (x * 4) as usize;
        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
    };
    let background = pixel(4, SIDE - 4);
    let expected = BACKGROUND.to_array().map(to_byte);
    assert!(
        background[..3]
            .iter()
            .zip(&expected[..3])
            .all(|(held, want)| held.abs_diff(*want) <= 2),
        "the interface fills its surface with {background:?} where the sheet holds {expected:?}",
    );
    let ink = ui
        .bounds("glyph")
        .expect("the interface lays out its glyph");
    let glyph = (0..ink.width() as u32)
        .flat_map(|x| (0..ink.height() as u32).map(move |y| (x, y)))
        .any(|(x, y)| {
            let at = (ink.min.x as u32 + x, ink.min.y as u32 + y);
            pixel(at.0, at.1)[0] > 160 && pixel(at.0, at.1)[1] > 160
        });
    assert!(glyph, "the interface paints the glyphs it shapes");
    let scene = (0..64)
        .flat_map(|x| (64..128).map(move |y| (x, y)))
        .filter(|(x, y)| pixel(*x, *y) == SCENE)
        .count();
    assert!(
        scene > 1000,
        "the interface samples the scene through its texture table, {scene} texels match",
    );
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[test]
fn a_user_interface_recycles_the_texture_slots_of_removed_images() {
    let reserved = dome_ui::RESERVED_SLOTS;
    let context = Gpu::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device recycles interface textures: {error}"));
    let device = context.device();
    let queue = context.queue();
    let format = TextureFormat::Rgba8Unorm;
    let mut ui: Ui<()> = Ui::new(device, &queue, format);
    let target = Texture::new(
        device,
        "dome recycled target",
        SIDE,
        SIDE,
        1,
        format,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let slot = device.tables().textures - reserved;
    let red = Texture::new(
        device,
        "dome recycled red",
        1,
        1,
        1,
        format,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    let green = Texture::new(
        device,
        "dome recycled green",
        1,
        1,
        1,
        format,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    let mut submission = Submission::new(device, "dome recycled textures");
    submission.write_texture(&red, 0, &[255, 0, 0, 255]);
    submission.write_texture(&green, 0, &[0, 255, 0, 255]);
    submission.submit(&queue);
    let old = (0..slot)
        .map(|index| format!("image.{index}"))
        .collect::<Vec<_>>();
    let fresh = (0..slot)
        .map(|index| format!("restored.{index}"))
        .collect::<Vec<_>>();
    ui.set_textures(old.iter().map(|name| (name.as_str(), &red, SAMPLER)));
    ui.set_textures(fresh.iter().map(|name| (name.as_str(), &green, SAMPLER)));

    ui.update(Vec2::splat(SIDE as f32), 1.0, 0.0);
    let element = {
        Element::column()
            .key(Key::new("root"))
            .width(Length::Fill)
            .height(Length::Fill)
            .children([
                canvas(Length::Fixed(64.0), 64.0).view_static(|painter, info| {
                    painter.image(
                        info.rect,
                        Corners::ZERO,
                        fresh.last().expect("a recycled name"),
                        [0.0, 0.0, 1.0, 1.0],
                        Color::WHITE,
                    );
                }),
            ])
    };
    ui.present(element);
    let mut submission = Submission::new(device, "dome recycled paint");
    ui.render(&mut submission, &target, None);
    let readback = GpuBuffer::new(
        device,
        "dome recycled readback",
        target.readback_bytes(),
        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
    );
    submission.copy_texture(&target, &readback, 0);
    let index = submission.submit(&queue);
    let bytes = readback.read(&queue, index, target.readback_bytes());
    let at = (32 * target.row_stride() + 32 * 4) as usize;
    let pixel = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
    assert!(
        pixel[1] > 200 && pixel[0] < 32,
        "the interface paints {pixel:?} where the recycled slot holds green",
    );
}
