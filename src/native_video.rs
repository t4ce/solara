//! One DOM video shades the foreground of its page's paired UI4 surface.
//! The kernel performs URL fetch,
//! demux, hardware decode and RGBA conversion; only leased texture IDs reach us.
use trueos::{
    ui4_scene::{Damage, Frame},
    vgpu::*,
    vmedia::{Video, VideoFrame, VideoPoll},
};

pub(crate) struct VideoSurface {
    stream: Video,
    current: Option<VideoFrame>,
    device: Device,
    queue: Queue,
    mesh: RetainedMesh,
    vertices: Buffer,
    indices: Buffer,
    rect: [i32; 4],
    first_frame: bool,
    pub(crate) paused: bool,
    dirty: bool,
    viewport: [u32; 2],
}
impl VideoSurface {
    pub(crate) fn open(
        device: Device,
        queue: Queue,
        url: &str,
        rect: [i32; 4],
    ) -> Result<Self, i32> {
        // Reserve the shared bounded video pool before creating the surface.
        let stream = Video::open_url(device, url, false)?;
        let vertices_data: [[f32; 8]; 4] = [
            [-1., -1., 0., 0., 0., -1., 0., 1.],
            [-1., 1., 0., 0., 0., -1., 0., 0.],
            [1., 1., 0., 0., 0., -1., 1., 0.],
            [1., -1., 0., 0., 0., -1., 1., 1.],
        ];
        let vb: Vec<u8> = vertices_data
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let ib: Vec<u8> = [0u32, 1, 2, 0, 2, 3]
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect();
        let vertices =
            device.create_buffer(vb.len(), BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_VERTEX)?;
        let indices =
            match device.create_buffer(ib.len(), BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_INDEX) {
                Ok(v) => v,
                Err(e) => {
                    let _ = device.destroy_buffer(vertices);
                    return Err(e);
                }
            };
        let prepared = (|| {
            if device.write_buffer(vertices, 0, &vb)? != vb.len()
                || device.write_buffer(indices, 0, &ib)? != ib.len()
            {
                return Err(ERR_IO);
            }
            device.create_retained_mesh(
                vertices,
                indices,
                RetainedMeshDescriptor {
                    vertex_count: 4,
                    index_count: 6,
                    vertex_layout: RETAINED_VERTEX_LAYOUT_POS_NORMAL_UV,
                    topology: PRIMITIVE_TOPOLOGY_TRIANGLE_LIST | RETAINED_MESH_FLAG_DOUBLE_SIDED,
                    ..Default::default()
                },
            )
        })();
        let mesh = match prepared {
            Ok(v) => v,
            Err(e) => {
                let _ = device.destroy_buffer(indices);
                let _ = device.destroy_buffer(vertices);
                return Err(e);
            }
        };
        Ok(Self {
            stream,
            current: None,
            device,
            queue,
            mesh,
            vertices,
            indices,
            rect,
            first_frame: true,
            paused: false,
            dirty: true,
            viewport: [0, 0],
        })
    }
    pub(crate) fn toggle_pause(&mut self) -> Result<(), i32> {
        self.paused = !self.paused;
        self.stream.set_paused(self.paused)
    }
    pub(crate) fn has_frame(&self) -> bool {
        self.current.is_some()
    }
    pub(crate) fn tick(&mut self, target: &mut Frame, rect: [i32; 4]) -> Result<(), i32> {
        let viewport = [target.width(), target.height()];
        let moved = self.rect != rect || self.viewport != viewport;
        self.rect = rect;
        self.viewport = viewport;
        let changed = match self.stream.poll()? {
            VideoPoll::Frame(frame) => {
                self.current = Some(frame);
                true
            }
            VideoPoll::Pending | VideoPoll::End => false,
        };
        let Some(frame) = &self.current else {
            return Ok(());
        };
        self.dirty |= changed || moved;
        if !self.dirty {
            return Ok(());
        }
        let [width, height] = frame.extent();
        let scale = (rect[2] as f32 / width as f32).min(rect[3] as f32 / height as f32);
        let identity = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        let mut submit = RetainedTexturedFrameV1::default();
        submit.frame.camera = RetainedCamera {
            view: identity,
            projection: identity,
            view_projection: identity,
            inverse_view_projection: identity,
            previous_view_projection: identity,
            position_near: [0., 0., -2., 0.1],
            forward_far: [0., 0., 1., 100.],
            ..Default::default()
        };
        submit.frame.clear_rgba8_srgb = 0;
        submit.frame.seed_count = 1;
        submit.frame.seeds[0] = RetainedTransformSeed {
            translation: [
                (2 * rect[0] + rect[2]) as f32 / viewport[0] as f32 - 1.,
                1. - (2 * rect[1] + rect[3]) as f32 / viewport[1] as f32,
                0.,
            ],
            scale: [
                width as f32 * scale / viewport[0] as f32,
                height as f32 * scale / viewport[1] as f32,
                1.,
            ],
            rotation: [0., 0., 0., 1.],
            local_radius: 1.5,
            ..Default::default()
        };
        submit.ranges[0] = RetainedDrawRange {
            first_index: 0,
            index_count: 6,
        };
        submit.textures[0] = frame.texture_id().raw();
        target.begin_gpu_frame().map_err(|e| match e {
            trueos::ui4_scene::Error::Busy => ERR_BUSY,
            _ => ERR_IO,
        })?;
        let surface = self.device.acquire_ui4_surface(target.window_id())?;
        let point = self
            .device
            .submit_retained_textured_frame_v1(self.queue, surface, self.mesh, submit)?;
        // Retire the GPU read before polling/replacing the lease on next tick.
        self.device.wait(self.queue, point.value)?;
        target
            .publish(Damage::full(viewport[0], viewport[1]))
            .map_err(|_| ERR_IO)?;
        self.dirty = false;
        if self.first_frame {
            self.first_frame = false;
            crate::parser_probe::report_info(format_args!(
                "solara: DOM video texture active window={} extent={}x{}",
                target.window_id(),
                width,
                height
            ));
        }
        Ok(())
    }
}
impl Drop for VideoSurface {
    fn drop(&mut self) {
        self.current = None;
        // All submits are retired before handles/leases are released.
        let _ = self.device.destroy_retained_mesh(self.mesh);
        let _ = self.device.destroy_buffer(self.indices);
        let _ = self.device.destroy_buffer(self.vertices);
    }
}
