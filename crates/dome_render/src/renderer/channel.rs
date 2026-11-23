use super::*;

pub(super) struct ChannelFrame<'a> {
    pub(super) clear: [f32; 4],
    pub(super) collection: &'a Collection,
    pub(super) batches: &'a [Batch],
    pub(super) bases: &'a [u32],
    pub(super) slots: u32,
    pub(super) total: u32,
    pub(super) depth_batches: u32,
    pub(super) points: &'a [PointLightRecord],
    pub(super) cluster_offset_bytes: u64,
    pub(super) cluster_bytes: u64,
    pub(super) cluster_x: u32,
    pub(super) cluster_y: u32,
    pub(super) sky: bool,
    pub(super) line_groups: &'a [BindGroup],
    pub(super) line_vertices: u32,
    pub(super) probe_count: u32,
    pub(super) depth: DepthTexture,
}

pub(super) struct Channels {
    pub(super) arguments: Vec<u32>,
}

impl Renderer {
    pub(super) fn channels(
        &mut self,
        submission: &mut Submission,
        frame: ChannelFrame<'_>,
    ) -> Channels {
        let ChannelFrame {
            clear,
            collection,
            batches,
            bases,
            slots,
            total,
            depth_batches,
            points,
            cluster_offset_bytes,
            cluster_bytes,
            cluster_x,
            cluster_y,
            sky,
            line_groups,
            line_vertices,
            probe_count,
            depth,
        } = frame;
        let scene_depth = &depth;
        let point_light_count = points.len() as u32;
        submission.clear(self.lighting.buffer(), cluster_offset_bytes, cluster_bytes);
        if point_light_count > 0 {
            let group = self.light_clusters.bind_group(
                0,
                &[
                    Binding::buffer(program::CLUSTER_VIEW, self.views.binding(0, VIEW_BYTES)),
                    Binding::buffer(
                        program::CLUSTER_POINTS,
                        self.lighting
                            .buffer()
                            .binding(0, u64::from(point_light_count) * POINT_LIGHT_BYTES),
                    ),
                    Binding::buffer(
                        program::CLUSTER_GRID,
                        self.lighting
                            .buffer()
                            .binding(cluster_offset_bytes, cluster_bytes),
                    ),
                ],
            );
            submission.dispatch(
                &self.light_clusters,
                &group,
                [cluster_x * cluster_y, LIGHT_SLICES, 1],
            );
        }
        let arguments = batches
            .iter()
            .flat_map(|batch| arguments_of(self.meshes.geometry(batch.mesh)))
            .collect::<Vec<u32>>();
        assert!(
            collection
                .uploads
                .iter()
                .flat_map(|upload| &upload.records)
                .all(|record| record.material < self.materials.records().len() as u32),
            "a rendered instance names a material this renderer does not hold"
        );
        self.upload(&collection.uploads, &arguments, slots);
        self.main_batches = batches.len() as u32;
        self.point_shadows.prepare(
            &self.device,
            &self.queue,
            &PointShadowFrame {
                clip_space: self.device.clip_space(),
                lights: points,
                instances: total,
                slots,
                palette_base: self.palettes.base(),
                arguments: &arguments,
            },
        );
        let point_cells = self.point_shadows.cells();
        let hzb = self
            .hzb
            .as_ref()
            .expect("a rendered frame owns its depth pyramid");
        let instances = self
            .instance_buffer
            .buffer()
            .binding(0, u64::from(total) * INSTANCE_BYTES);
        let depth_cull = hzb.cull(
            &self.cull,
            self.views.binding(VIEW_BYTES, VIEW_BYTES),
            instances,
            self.depth_drawn
                .buffer()
                .binding(0, u64::from(slots) * DRAW_BYTES),
            self.depth_arguments
                .buffer()
                .binding(0, arguments.len() as u64 * WORD_BYTES),
        );
        let main_cull = hzb.cull(
            &self.cull,
            self.views.binding(0, VIEW_BYTES),
            instances,
            self.main_drawn
                .buffer()
                .binding(0, u64::from(slots) * DRAW_BYTES),
            self.main_arguments
                .buffer()
                .binding(0, arguments.len() as u64 * WORD_BYTES),
        );
        let mut shadow_cull = Vec::with_capacity(CASCADE_COUNT as usize);
        for cascade in 0..CASCADE_COUNT {
            shadow_cull.push(
                hzb.cull(
                    &self.cull,
                    self.shadow_views
                        .binding(u64::from(cascade) * VIEW_BYTES, VIEW_BYTES),
                    instances,
                    self.shadow_drawn.buffer().binding(
                        u64::from(cascade) * u64::from(slots) * DRAW_BYTES,
                        u64::from(slots) * DRAW_BYTES,
                    ),
                    self.shadow_arguments.buffer().binding(
                        u64::from(cascade) * self.shadow_span,
                        arguments.len() as u64 * WORD_BYTES,
                    ),
                ),
            );
        }
        submission.dispatch(&self.cull, &depth_cull, [total.div_ceil(WORKGROUP), 1, 1]);
        for group in &shadow_cull {
            submission.dispatch(&self.cull, group, [total.div_ceil(WORKGROUP), 1, 1]);
        }
        if point_light_count > 0 {
            for cell in 0..point_cells {
                let group = hzb.cull(
                    &self.cull,
                    self.point_shadows.view(cell),
                    instances,
                    self.point_shadows.drawn_view(cell),
                    self.point_shadows.argument_view(cell),
                );
                submission.dispatch(&self.cull, &group, [total.div_ceil(WORKGROUP), 1, 1]);
            }
        }
        let pipelines = Pipelines {
            depth: &self.depth,
            opaque: &self.draw,
            capture: &self.capture_draw,
            blended: &self.transparent_draw,
            motion: self.temporality.motion(),
            motion_blended: self.temporality.motion_blended(),
            shadow: &self.shadow,
        };
        let mut slots_in = Vec::with_capacity(batches.len());
        for (index, batch) in batches.iter().enumerate() {
            let geometry = self.meshes.geometry(batch.mesh);
            slots_in.push(Slot {
                vertices: self.meshes.buffer().binding(
                    u64::from(geometry.first) * VERTEX_BYTES,
                    u64::from(geometry.count) * VERTEX_BYTES,
                ),
                skins: self.meshes.skin_binding(geometry),
                depth_drawn: self.depth_drawn.buffer().binding(
                    u64::from(bases[index]) * DRAW_BYTES,
                    u64::from(batch.count) * DRAW_BYTES,
                ),
                main_drawn: self.main_drawn.buffer().binding(
                    u64::from(bases[index]) * DRAW_BYTES,
                    u64::from(batch.count) * DRAW_BYTES,
                ),
                shadow_drawn: std::array::from_fn(|cascade| {
                    self.shadow_drawn.buffer().binding(
                        cascade as u64 * u64::from(slots) * DRAW_BYTES
                            + u64::from(bases[index]) * DRAW_BYTES,
                        u64::from(batch.count) * DRAW_BYTES,
                    )
                }),
            });
        }
        let shared = Shared {
            view: self.views.binding(0, VIEW_BYTES),
            motion_view: self.temporality.view(),
            palette: self.palettes.binding(),
            materials: self
                .material_records
                .buffer()
                .binding(0, self.materials.records().len() as u64 * MATERIAL_BYTES),
            light: self.light.binding(0, LIGHT_BYTES),
            cascades: self
                .cascades
                .binding(0, u64::from(CASCADE_COUNT) * CASCADE_BYTES),
            lighting: self.lighting.buffer().binding(0, self.lighting_capacity),
            probes: self.probes.record(probe_count),
            keys: self.transparency.keys(),
            colors: self.transparency.colors(),
            dropped: self.transparency.dropped(),
            settings: self.transparency.settings(),
        };
        let groups = slots_in
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                SlotGroups::build(&pipelines, &shared, slot, index >= depth_batches as usize)
            })
            .collect::<Vec<_>>();
        let textures = self.materials.textures();
        let samplers = self.materials.samplers();
        let depth_tables = table(
            pipelines.depth,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let shadow_tables = table(
            pipelines.shadow,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let tables = table(
            pipelines.opaque,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let transparent_tables = table(
            pipelines.blended,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let motion_tables = table(
            pipelines.motion,
            program::MOTION_TEXTURE,
            program::MOTION_SAMPLER,
            textures,
            samplers,
        );
        let motion_blended_tables = table(
            pipelines.motion_blended,
            program::MOTION_TEXTURE,
            program::MOTION_SAMPLER,
            textures,
            samplers,
        );
        let depth_groups = (0..depth_batches as usize)
            .map(|index| {
                [
                    groups[index]
                        .depth()
                        .expect("an opaque batch binds the depth pass")
                        .clone(),
                    depth_tables.clone(),
                ]
            })
            .collect::<Vec<_>>();
        let mut main_groups = Vec::with_capacity(batches.len());
        let mut motion_groups = Vec::with_capacity(batches.len());
        for (index, slot) in groups.iter().enumerate() {
            if index < depth_batches as usize {
                main_groups.push([slot.shade().clone(), tables.clone(), self.shade.clone()]);
                motion_groups.push([slot.motion().clone(), motion_tables.clone()]);
            } else {
                main_groups.push([
                    slot.shade().clone(),
                    transparent_tables.clone(),
                    self.transparent_shade.clone(),
                ]);
                motion_groups.push([slot.motion().clone(), motion_blended_tables.clone()]);
            }
        }
        let mut cast_groups = Vec::with_capacity(CASCADE_COUNT as usize * depth_batches as usize);
        for cascade in 0..CASCADE_COUNT as usize {
            for slot in groups.iter().take(depth_batches as usize) {
                cast_groups.push([slot.shadow(cascade).clone(), shadow_tables.clone()]);
            }
        }
        let depth = depth_groups
            .iter()
            .enumerate()
            .map(|(index, groups)| {
                Draw::indirect(
                    &self.depth,
                    groups,
                    self.depth_arguments
                        .buffer()
                        .binding(index as u64 * ARGUMENT_BYTES, ARGUMENT_BYTES),
                )
            })
            .collect::<Vec<_>>();
        submission.render(
            Pass::new(hzb.depth(), [1.0, 0.0, 0.0, 1.0]).depth(scene_depth, DepthOps::CLEAR_STORED),
            &depth,
        );
        hzb.build(submission, &self.hzb_base, &self.hzb_reduce);
        submission.dispatch(&self.cull, &main_cull, [total.div_ceil(WORKGROUP), 1, 1]);
        let mut main = main_groups
            .iter()
            .enumerate()
            .map(|(index, groups)| {
                let pipeline = if index < depth_batches as usize {
                    &self.draw
                } else {
                    &self.transparent_draw
                };
                Draw::indirect(
                    pipeline,
                    groups,
                    self.main_arguments
                        .buffer()
                        .binding(index as u64 * ARGUMENT_BYTES, ARGUMENT_BYTES),
                )
            })
            .collect::<Vec<_>>();
        let sky_groups = [self.sky_view.clone(), self.sky_shade.clone()];
        if sky {
            main.push(Draw::new(&self.sky, &sky_groups, SKY_VERTICES));
        }
        if !line_groups.is_empty() {
            main.push(Draw::new(&self.line_pipeline, line_groups, line_vertices));
        }
        let cast = cast_groups
            .iter()
            .enumerate()
            .map(|(index, groups)| {
                let cascade = index / depth_batches as usize;
                let batch = index % depth_batches as usize;
                let column = cascade as u32 % SHADOW_ATLAS_COLUMNS;
                let row = cascade as u32 / SHADOW_ATLAS_COLUMNS;
                Draw::indirect(
                    &self.shadow,
                    groups,
                    self.shadow_arguments.buffer().binding(
                        u64::from(cascade as u32) * self.shadow_span
                            + batch as u64 * ARGUMENT_BYTES,
                        ARGUMENT_BYTES,
                    ),
                )
                .viewport(Viewport::new(
                    column as f32 * SHADOW_SIDE as f32,
                    row as f32 * SHADOW_SIDE as f32,
                    SHADOW_SIDE as f32,
                    SHADOW_SIDE as f32,
                ))
            })
            .collect::<Vec<_>>();
        submission.render(
            Pass::new(&self.shadow_map, [1.0, 0.0, 0.0, 1.0])
                .depth(&self.shadow_depth, DepthOps::CLEAR_STORED),
            &cast,
        );
        if point_light_count > 0 {
            let palette = self.palettes.binding();
            let materials = self
                .material_records
                .buffer()
                .binding(0, self.materials.records().len() as u64 * MATERIAL_BYTES);
            let opaque = depth_batches as usize;
            let mut groups = Vec::with_capacity(point_cells as usize * opaque);
            for cell in 0..point_cells {
                for (index, batch) in batches.iter().take(opaque).enumerate() {
                    let geometry = self.meshes.geometry(batch.mesh);
                    let base = bases[index];
                    groups.push([
                        self.shadow.bind_group(
                            0,
                            &[
                                Binding::buffer(program::SHADOW_PALETTE, palette),
                                Binding::buffer(
                                    program::SHADOW_SKINS,
                                    self.meshes.skin_binding(geometry),
                                ),
                                Binding::buffer(
                                    program::SHADOW_DRAWN,
                                    self.point_shadows.drawn(cell, base, batch.count),
                                ),
                                Binding::buffer(
                                    program::SHADOW_VERTICES,
                                    self.meshes.buffer().binding(
                                        u64::from(geometry.first) * VERTEX_BYTES,
                                        u64::from(geometry.count) * VERTEX_BYTES,
                                    ),
                                ),
                                Binding::buffer(program::SHADOW_MATERIALS, materials),
                            ],
                        ),
                        shadow_tables.clone(),
                    ]);
                }
            }
            let draws = groups
                .iter()
                .enumerate()
                .map(|(index, held)| {
                    let cell = (index / opaque) as u32;
                    let batch = (index % opaque) as u32;
                    Draw::indirect(
                        &self.shadow,
                        held,
                        self.point_shadows.arguments(cell, batch),
                    )
                    .viewport(self.point_shadows.viewport(cell))
                })
                .collect::<Vec<_>>();
            submission.render(
                Pass::new(self.point_shadows.atlas(), self.point_shadows.clear())
                    .depth(self.point_shadows.depth(), DepthOps::CLEAR_STORED),
                &draws,
            );
        }
        if depth_batches < batches.len() as u32 {
            self.transparency.clear_layers(submission);
        } else {
            self.transparency.clear_dropped(submission);
        }
        submission.render(
            Pass::new(self.post.hdr(), [clear[0], clear[1], clear[2], 0.0])
                .depth(scene_depth, DepthOps::KEEP),
            &main,
        );
        let motion = motion_groups
            .iter()
            .enumerate()
            .map(|(index, groups)| {
                let pipeline = if index < depth_batches as usize {
                    &self.temporality.motion()
                } else {
                    &self.temporality.motion_blended()
                };
                Draw::indirect(
                    pipeline,
                    groups,
                    self.main_arguments
                        .buffer()
                        .binding(index as u64 * ARGUMENT_BYTES, ARGUMENT_BYTES),
                )
            })
            .collect::<Vec<_>>();
        submission.render(
            Pass::new(self.temporality.target(), [0.0, 0.0, 0.0, 0.0])
                .depth(scene_depth, DepthOps::KEEP),
            &motion,
        );
        Channels { arguments }
    }
}
