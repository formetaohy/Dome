use super::*;

impl Renderer {
    pub(super) fn absorb_damage(&mut self, world: &mut World, collection: &Collection) {
        self.damage.absorb(
            world,
            (&collection.entities, &collection.spheres),
            &self.spheres,
        );
        self.spheres.clear();
        self.spheres.extend(
            collection
                .entities
                .iter()
                .copied()
                .zip(collection.spheres.iter().copied()),
        );
    }

    pub(super) fn stale_probe(&self, world: &World) -> Option<Probe> {
        world.iter::<CaptureProbe>().find_map(|(entity, probe)| {
            let transform = *world.get::<Transform>(entity)?;
            let environment = world.get::<Environment>(entity)?.clone();
            (probe.stale(transform) || self.damage.pending(entity)).then_some(Probe {
                entity,
                transform,
                probe: *probe,
                environment,
            })
        })
    }

    pub(super) fn capture(
        &mut self,
        probe: Option<Probe>,
        inputs: &CaptureInputs<'_>,
        travel: &Travel,
        rotation: Mat4,
        intensity: f32,
    ) {
        let Some(probe) = probe else {
            return;
        };
        let radius = probe.transform.scale.length();
        let shadow = shadow::probe(
            probe.transform.translation,
            radius,
            travel,
            inputs.spheres,
            radius,
        );
        self.captures.prepare(&CaptureFrame {
            device: self.device.clone(),
            queue: self.queue.clone(),
            eye: probe.transform.translation,
            basis: probe.transform.rotation.normalize(),
            rotation,
            intensity,
            shadow,
            palette_base: self.palettes.posed(),
            batches: inputs.batches,
            arguments: inputs.arguments,
            instances: inputs.instances,
            slots: inputs.slots,
            points: inputs.points,
        });
        let mut submission = Submission::new(&self.device, "dome capture");
        let instances = self
            .instance_buffer
            .buffer()
            .binding(0, u64::from(inputs.instances) * INSTANCE_BYTES);
        if inputs.opaque > 0 {
            let hzb = self
                .hzb
                .as_ref()
                .expect("a captured scene culls against the depth pyramid of its frame");
            let culled = inputs.instances.div_ceil(WORKGROUP);
            for view in 0..self.captures.view_count() {
                let group = hzb.cull(
                    &self.cull,
                    &self.hzb_sampler,
                    self.captures.view(view),
                    instances,
                    self.captures.visible_view(view),
                    self.captures.argument_view(view),
                );
                submission.dispatch(&self.cull, &group, [culled, 1, 1]);
            }
        }
        if inputs.opaque > 0 && !inputs.points.is_empty() {
            let (offset, bytes) = self.captures.cluster_region();
            submission.clear(self.captures.lighting.buffer(), offset, bytes);
            let points = self.captures.point_lights();
            for face in 0..crate::capture::FACES {
                let group = self.light_clusters.bind_group(
                    0,
                    &[
                        Binding::buffer(program::CLUSTER_VIEW, self.captures.view(face)),
                        Binding::buffer(program::CLUSTER_POINTS, points),
                        Binding::buffer(program::CLUSTER_GRID, self.captures.clusters(face)),
                    ],
                );
                submission.dispatch(
                    &self.light_clusters,
                    &group,
                    [
                        self.captures.cluster_columns() * self.captures.cluster_rows(),
                        LIGHT_SLICES,
                        1,
                    ],
                );
            }
        }
        let capture_shade = self.shading.group(
            &self.capture_draw,
            (&self.captures.shadow, &self.captures.shadow_sampler),
            (self.point_shadows.atlas(), self.point_shadows.sampler()),
            (self.probes.textures(), self.probes.samplers()),
        );
        let pipelines = Pipelines {
            depth: &self.depth,
            opaque: &self.draw,
            capture: &self.capture_draw,
            blended: &self.transparent_draw,
            motion: self.temporality.motion(),
            shadow: &self.shadow,
        };
        let textures = self.materials.textures();
        let samplers = self.materials.samplers();
        let shadow_tables = table(
            pipelines.shadow,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let tables = table(
            pipelines.capture,
            program::SHADE_TEXTURE,
            program::SHADE_SAMPLER,
            textures,
            samplers,
        );
        let materials = self
            .material_records
            .buffer()
            .binding(0, self.materials.records().len() as u64 * MATERIAL_BYTES);
        let palette = self.palettes.binding();
        if inputs.opaque > 0 {
            let mut cast_groups = Vec::with_capacity(inputs.opaque as usize);
            for (index, batch) in inputs
                .batches
                .iter()
                .take(inputs.opaque as usize)
                .enumerate()
            {
                let geometry = self.meshes.geometry(batch.mesh);
                let group = self.shadow.bind_group(
                    0,
                    &[
                        Binding::buffer(program::SHADOW_PALETTE, palette),
                        Binding::buffer(program::SHADOW_SKINS, self.meshes.skin_binding(geometry)),
                        Binding::buffer(
                            program::SHADOW_VISIBLE,
                            self.captures
                                .shadow_visible(inputs.bases[index], batch.count),
                        ),
                        Binding::buffer(
                            program::SHADOW_VERTICES,
                            self.meshes.buffer().binding(
                                u64::from(geometry.first) * VERTEX_BYTES,
                                u64::from(geometry.count) * VERTEX_BYTES,
                            ),
                        ),
                        Binding::buffer(program::SHADOW_MATERIALS, materials),
                        Binding::buffer(
                            program::SHADOW_VIEW,
                            self.captures.view(crate::capture::SHADOW_VIEW),
                        ),
                        Binding::buffer(program::SHADOW_INSTANCES, instances),
                    ],
                );
                cast_groups.push([group, shadow_tables.clone()]);
            }
            let casters = (0..inputs.opaque)
                .map(|index| {
                    Draw::indirect(
                        &self.shadow,
                        &cast_groups[index as usize],
                        self.captures.shadow_arguments(index),
                    )
                })
                .collect::<Vec<_>>();
            submission.render(Pass::depth_only(&self.captures.shadow), &casters);
        }
        let mut skies = Vec::with_capacity(crate::capture::FACES as usize);
        for face in 0..crate::capture::FACES {
            skies.push([
                self.sky.bind_group(
                    0,
                    &[Binding::buffer(program::SKY_VIEW, self.captures.sky(face))],
                ),
                self.sky_shade.clone(),
            ]);
        }
        let mut batches =
            Vec::with_capacity(crate::capture::FACES as usize * inputs.opaque as usize);
        if inputs.opaque > 0 {
            let lights = self.captures.lighting();
            let point_lights = self.captures.point_lights();
            for face in 0..crate::capture::FACES {
                for (index, batch) in inputs
                    .batches
                    .iter()
                    .take(inputs.opaque as usize)
                    .enumerate()
                {
                    let geometry = self.meshes.geometry(batch.mesh);
                    let shade = self.capture_draw.bind_group(
                        0,
                        &[
                            Binding::buffer(
                                program::DRAW_LIGHT,
                                self.light.binding(0, LIGHT_BYTES),
                            ),
                            Binding::buffer(program::DRAW_CASCADES, self.captures.cascades()),
                            Binding::buffer(
                                program::DRAW_VISIBLE,
                                self.captures
                                    .visible(face, inputs.bases[index], batch.count),
                            ),
                            Binding::buffer(program::DRAW_MATERIALS, materials),
                            Binding::buffer(program::DRAW_VIEW, self.captures.view(face)),
                            Binding::buffer(program::DRAW_PALETTE, palette),
                            Binding::buffer(
                                program::DRAW_SKINS,
                                self.meshes.skin_binding(geometry),
                            ),
                            Binding::buffer(program::DRAW_FRAGMENTS, self.transparency.keys()),
                            Binding::buffer(program::DRAW_COLORS, self.transparency.colors()),
                            Binding::buffer(program::DRAW_DROPPED, self.transparency.dropped()),
                            Binding::buffer(program::DRAW_SETTINGS, self.transparency.settings()),
                            Binding::buffer(
                                program::DRAW_PROBES,
                                self.probes.record(inputs.probe_count),
                            ),
                            Binding::buffer(
                                program::DRAW_VERTICES,
                                self.meshes.buffer().binding(
                                    u64::from(geometry.first) * VERTEX_BYTES,
                                    u64::from(geometry.count) * VERTEX_BYTES,
                                ),
                            ),
                            Binding::buffer(program::DRAW_LIGHTS, lights),
                            Binding::buffer(program::DRAW_POINT_LIGHTS, point_lights),
                            Binding::buffer(program::DRAW_INSTANCES, instances),
                        ],
                    );
                    batches.push([shade, tables.clone(), capture_shade.clone()]);
                }
            }
        }
        let mut draws =
            Vec::with_capacity(crate::capture::FACES as usize * (1 + inputs.opaque as usize));
        let mut batch = 0;
        for face in 0..crate::capture::FACES {
            let viewport = Viewport::new(
                (face * crate::capture::PROBE_RESOLUTION) as f32,
                0.0,
                crate::capture::PROBE_RESOLUTION as f32,
                crate::capture::PROBE_RESOLUTION as f32,
            );
            draws
                .push(Draw::new(&self.sky, &skies[face as usize], SKY_VERTICES).viewport(viewport));
            for index in 0..inputs.opaque {
                draws.push(
                    Draw::indirect(
                        &self.capture_draw,
                        &batches[batch],
                        self.captures.arguments(face, index),
                    )
                    .viewport(viewport),
                );
                batch += 1;
            }
        }
        submission.render(
            Pass::new(&self.captures.atlas, [0.0, 0.0, 0.0, 1.0])
                .depth(&self.captures.depth, DepthOps::CLEAR),
            &draws,
        );
        let convert = [
            self.captures.convert.bind_group(
                0,
                &[Binding::buffer(
                    program::CAPTURE_SETUP,
                    self.captures.setup(),
                )],
            ),
            self.captures.convert.bind_group(
                1,
                &[
                    Binding::texture(program::CAPTURE_ATLAS, &self.captures.atlas),
                    Binding::sampler(program::CAPTURE_SAMPLER, &self.captures.sampler),
                ],
            ),
        ];
        let panorama = self.captures.panorama(&self.device);
        submission.render(
            Pass::new(&panorama, [0.0, 0.0, 0.0, 1.0]),
            &[Draw::new(&self.captures.convert, &convert, 3)],
        );
        self.submitted = Some(submission.submit(&self.queue));
        let sampler = self.shading.sampler().clone();
        let request = self.bake(&panorama, &sampler);
        let mut marker = probe.probe;
        marker.captured(probe.transform);
        self.damage.covered(probe.entity);
        self.captures.begin(crate::capture::Pending {
            entity: probe.entity,
            probe: marker,
            intensity: probe.environment.intensity,
            background: probe.environment.background,
            blend: probe.environment.blend,
            panorama,
            request,
        });
    }

    pub(super) fn settle_captures(&mut self) {
        let Some(request) = self.captures.pending().map(|pending| pending.request) else {
            return;
        };
        let Some(baked) = self.take_baked(request) else {
            return;
        };
        self.captures.settle(baked, self.shading.sampler());
    }
}
