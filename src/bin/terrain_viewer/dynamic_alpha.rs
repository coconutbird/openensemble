//! GPU upload wrapper for animation-driven terrain visibility.

use glam::Vec2;
use render::terrain::DynamicTerrainAlphaMask;
use render::ugx::{AnimationTerrainAlpha, AnimationTerrainAlphaShape};
use render::wgpu;

pub(crate) struct DynamicTerrainAlphaTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    mask: DynamicTerrainAlphaMask,
    world_min: Vec2,
    world_max: Vec2,
}

impl DynamicTerrainAlphaTexture {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_dimension: u32,
        world_min: Vec2,
        world_max: Vec2,
    ) -> Self {
        let mask = DynamicTerrainAlphaMask::new(terrain_dimension)
            .expect("dynamic terrain alpha dimensions must fit host memory");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Dynamic Terrain Alpha Bitmask"),
            size: wgpu::Extent3d {
                width: mask.words_per_row(),
                height: mask.dimension(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let result = Self {
            texture,
            view,
            mask,
            world_min,
            world_max,
        };
        result.upload(queue);
        result
    }

    pub(crate) const fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub(crate) fn apply(&mut self, queue: &wgpu::Queue, event: AnimationTerrainAlpha) {
        let transform = event.transform();
        let center = Vec2::new(transform.w_axis.x, transform.w_axis.z);
        let changed = match event.shape() {
            AnimationTerrainAlphaShape::Rectangle {
                half_extent_x,
                half_extent_z,
            } => self.mask.set_oriented_rectangle_world(
                center,
                [
                    Vec2::new(transform.x_axis.x, transform.x_axis.z),
                    Vec2::new(transform.z_axis.x, transform.z_axis.z),
                ],
                Vec2::new(half_extent_x, half_extent_z),
                [self.world_min, self.world_max],
                event.enabled(),
            ),
            AnimationTerrainAlphaShape::Circle { radius } => self.mask.set_circle_world(
                center,
                radius,
                self.world_min,
                self.world_max,
                event.enabled(),
            ),
        };
        if changed {
            self.upload(queue);
        }
    }

    fn upload(&self, queue: &wgpu::Queue) {
        queue.write_texture(
            self.texture.as_image_copy(),
            bytemuck::cast_slice(self.mask.words()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.mask.words_per_row() * 4),
                rows_per_image: Some(self.mask.dimension()),
            },
            wgpu::Extent3d {
                width: self.mask.words_per_row(),
                height: self.mask.dimension(),
                depth_or_array_layers: 1,
            },
        );
    }
}
