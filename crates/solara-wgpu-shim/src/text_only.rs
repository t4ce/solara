//! Audited WGPU call surface used by the `text-only` renderer.
//!
//! This inventory describes calls made by the shim. It does not restrict the
//! upstream `wgpu` and `wgpu_text` re-exports.

/// GPU labels emitted by the text-only command encoder and render pass.
pub const TAG: &str = "solara-wgpu:text-only";

/// Exact WGPU and `wgpu_text` operations used by the text-only path.
pub const API_SUBSET: &[&str] = &[
    "wgpu::Adapter::request_device",
    "wgpu::CommandEncoder::begin_render_pass",
    "wgpu::CommandEncoder::finish",
    "wgpu::Device::create_command_encoder",
    "wgpu::Instance::create_surface",
    "wgpu::Instance::new",
    "wgpu::Instance::request_adapter",
    "wgpu::InstanceDescriptor::new_with_display_handle_from_env",
    "wgpu::Queue::present",
    "wgpu::Queue::submit",
    "wgpu::Surface::configure",
    "wgpu::Surface::get_capabilities",
    "wgpu::Surface::get_current_texture",
    "wgpu::Texture::create_view",
    "wgpu_text::BrushBuilder::build",
    "wgpu_text::TextBrush::draw",
    "wgpu_text::TextBrush::queue",
    "wgpu_text::TextBrush::resize_view",
];

/// Shape-pipeline operations deliberately absent from the text-only path.
pub const EXCLUDED_SHAPE_API: &[&str] = &[
    "wgpu::Device::create_bind_group",
    "wgpu::Device::create_bind_group_layout",
    "wgpu::Device::create_pipeline_layout",
    "wgpu::Device::create_render_pipeline",
    "wgpu::Device::create_shader_module",
    "wgpu::DeviceExt::create_buffer_init",
    "wgpu::RenderPass::draw",
    "wgpu::RenderPass::set_bind_group",
    "wgpu::RenderPass::set_pipeline",
    "wgpu::RenderPass::set_vertex_buffer",
];

#[cfg(test)]
mod tests {
    use super::{API_SUBSET, EXCLUDED_SHAPE_API, TAG};

    const SHIM_SOURCE: &str = include_str!("lib.rs");
    const SOURCE_TAG: &str = "// TEXT_ONLY_WGPU_API: ";

    #[test]
    fn api_inventory_matches_every_tagged_text_only_call() {
        let mut tagged = SHIM_SOURCE
            .lines()
            .filter_map(|line| line.trim().strip_prefix(SOURCE_TAG))
            .collect::<Vec<_>>();
        tagged.sort_unstable();
        tagged.dedup();

        let mut expected = API_SUBSET.to_vec();
        expected.sort_unstable();

        println!("{TAG}");
        for operation in &tagged {
            println!("  {operation}");
        }

        assert_eq!(tagged, expected);
    }

    #[test]
    fn api_inventory_is_shape_free() {
        for operation in EXCLUDED_SHAPE_API {
            assert!(
                !API_SUBSET.contains(operation),
                "{operation} leaked into {TAG}"
            );
        }
    }
}
