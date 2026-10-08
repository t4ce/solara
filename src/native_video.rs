//! One DOM video owns a UI4 texture surface. The kernel performs URL fetch,
//! demux, hardware decode and RGBA conversion; only leased texture IDs reach us.
use trueos::{
    ui4_scene::{Damage, Frame, MenuEntry},
    vgpu::*,
    vmedia::{Video, VideoFrame, VideoPoll},
};

pub(crate) struct VideoSurface {
    stream: Video,
    current: Option<VideoFrame>,
    frame: Frame,
    device: Device,
    queue: Queue,
    mesh: RetainedMesh,
    vertices: Buffer,
    indices: Buffer,
    rect: [i32; 4],
    first_frame: bool,
    pub(crate) closed: bool,
    paused: bool,
    dirty: bool,
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
        let frame = match Frame::open_streaming(rect[0], rect[1], rect[2] as u32, rect[3] as u32) {
            Ok(v) => v,
            Err(_) => {
                let _ = device.destroy_retained_mesh(mesh);
                let _ = device.destroy_buffer(indices);
                let _ = device.destroy_buffer(vertices);
                return Err(ERR_IO);
            }
        };
        let mut surface = Self {
            stream,
            current: None,
            frame,
            device,
            queue,
            mesh,
            vertices,
            indices,
            rect,
            first_frame: true,
            closed: false,
            paused: false,
            dirty: true,
        };
        surface
            .frame
            .register_dynamic_context_menu()
            .map_err(|_| ERR_IO)?;
        Ok(surface)
    }
    pub(crate) fn tick(&mut self, rect: [i32; 4]) -> Result<(), i32> {
        while let Some(event) = self
            .frame
            .take_dynamic_context_menu_event()
            .map_err(|_| ERR_IO)?
        {
            let entries = [
                MenuEntry::new(
                    if self.paused { "Play" } else { "Pause" },
                    |s: &mut [bool; 2]| s[0] = true,
                ),
                MenuEntry::new("Close video", |s: &mut [bool; 2]| s[1] = true),
            ];
            if event.closed.is_none() {
                self.frame
                    .resolve_context_menu(event.serial, &entries)
                    .map_err(|_| ERR_IO)?;
            } else {
                let mut action = [false; 2];
                event.dispatch(&entries, &mut action);
                if action[0] {
                    self.paused = !self.paused;
                    self.stream.set_paused(self.paused)?;
                }
                if action[1] {
                    self.closed = true;
                    return Ok(());
                }
            }
        }
        let moved = self.rect != rect;
        if moved {
            if self.rect[..2] != rect[..2] {
                self.frame
                    .set_position(rect[0], rect[1])
                    .map_err(|_| ERR_IO)?;
            }
            if self.rect[2..] != rect[2..] {
                self.frame
                    .resize(rect[2] as u32, rect[3] as u32)
                    .map_err(|_| ERR_IO)?;
            }
            self.rect = rect;
        }
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
        submit.frame.clear_rgba8_srgb = u32::from_le_bytes([0, 0, 0, 255]);
        submit.frame.seed_count = 1;
        submit.frame.seeds[0] = RetainedTransformSeed {
            scale: [
                width as f32 * scale / rect[2] as f32,
                height as f32 * scale / rect[3] as f32,
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
        self.frame.begin_gpu_frame().map_err(|e| match e {
            trueos::ui4_scene::Error::Busy => ERR_BUSY,
            _ => ERR_IO,
        })?;
        let surface = self.device.acquire_ui4_surface(self.frame.window_id())?;
        let point = self
            .device
            .submit_retained_textured_frame_v1(self.queue, surface, self.mesh, submit)?;
        // Retire the GPU read before polling/replacing the lease on next tick.
        self.device.wait(self.queue, point.value)?;
        self.frame
            .publish(Damage::full(rect[2] as u32, rect[3] as u32))
            .map_err(|_| ERR_IO)?;
        self.dirty = false;
        if self.first_frame {
            self.first_frame = false;
            crate::parser_probe::report_info(format_args!(
                "solara: DOM video texture active window={} extent={}x{}",
                self.frame.window_id(),
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
