use super::*;
use crate::renderer::scene::Upload;

impl Renderer {
    pub(super) fn ensure_hzb(&mut self, width: u32, height: u32, vertical: f32) {
        if self
            .hzb
            .as_ref()
            .is_some_and(|hzb| hzb.holds(width, height))
        {
            return;
        }
        self.hzb = Some(Hzb::new(
            &self.device,
            &self.queue,
            &self.hzb_base,
            &self.hzb_reduce,
            &self.hzb_sampler,
            width,
            height,
            vertical,
        ));
    }

    pub(super) fn upload(&mut self, uploads: &[Upload], arguments: &[u32], slots: u32) {
        for upload in uploads {
            self.instance_buffer.buffer().write_at(
                &self.queue,
                upload.offset,
                bytemuck::cast_slice(&upload.records),
            );
        }
        self.depth_drawn
            .fit(&self.device, u64::from(slots) * DRAW_BYTES);
        self.depth_arguments
            .fit(&self.device, arguments.len() as u64 * WORD_BYTES);
        self.depth_arguments
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(arguments));
        self.main_drawn
            .fit(&self.device, u64::from(slots) * DRAW_BYTES);
        self.main_arguments
            .fit(&self.device, arguments.len() as u64 * WORD_BYTES);
        self.main_arguments
            .buffer()
            .write(&self.queue, bytemuck::cast_slice(arguments));
        self.shadow_drawn.fit(
            &self.device,
            u64::from(CASCADE_COUNT) * u64::from(slots) * DRAW_BYTES,
        );
        let argument_bytes = arguments.len() as u64 * WORD_BYTES;
        let alignment = self
            .device
            .limits()
            .min_storage_buffer_offset_alignment
            .max(WORD_BYTES);
        let span = argument_bytes.next_multiple_of(alignment);
        self.shadow_span = span;
        self.shadow_arguments
            .fit(&self.device, u64::from(CASCADE_COUNT) * span);
        let mut shadow = vec![0u8; (u64::from(CASCADE_COUNT) * span) as usize];
        let source = bytemuck::cast_slice(arguments);
        for cascade in 0..CASCADE_COUNT {
            let start = (u64::from(cascade) * span) as usize;
            shadow[start..start + source.len()].copy_from_slice(source);
        }
        self.shadow_arguments.buffer().write(&self.queue, &shadow);
    }
}
