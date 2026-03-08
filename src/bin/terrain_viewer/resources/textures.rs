//! Texture creation for terrain rendering.
//!
//! Creates GPU texture arrays and atlases from loaded terrain data.

use anyhow::Result;
use render::terrain::{generate_mipmaps, mip_level_count};
use render::wgpu;

use crate::types::AlbedoData;
use crate::viewer::TerrainViewer;

impl TerrainViewer {
    /// Creates a 2D texture array from loaded terrain textures.
    pub(crate) fn create_terrain_texture_array(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        albedo: &Option<AlbedoData>,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        // Use terrain textures if available, otherwise fall back to albedo or white
        if !self.terrain_textures.is_empty() {
            // All textures should be same size (e.g., 1024x1024)
            let tex_width = self.terrain_textures[0].width;
            let tex_height = self.terrain_textures[0].height;
            let layer_count = self.terrain_textures.len() as u32;
            let num_mips = mip_level_count(tex_width, tex_height);

            log::info!(
                "Creating terrain texture array: {}x{} x {} layers with {} mip levels",
                tex_width,
                tex_height,
                layer_count,
                num_mips
            );

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Terrain Texture Array"),
                size: wgpu::Extent3d {
                    width: tex_width,
                    height: tex_height,
                    depth_or_array_layers: layer_count,
                },
                mip_level_count: num_mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            // Upload each layer with mipmaps
            for (i, tex) in self.terrain_textures.iter().enumerate() {
                // Generate mipmaps for this texture
                let mips = generate_mipmaps(&tex.pixels, tex_width, tex_height);

                // Upload each mip level
                let mut mip_width = tex_width;
                let mut mip_height = tex_height;
                for (mip_level, mip_data) in mips.iter().enumerate() {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: mip_level as u32,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: i as u32,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        mip_data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * mip_width),
                            rows_per_image: Some(mip_height),
                        },
                        wgpu::Extent3d {
                            width: mip_width,
                            height: mip_height,
                            depth_or_array_layers: 1,
                        },
                    );
                    mip_width = (mip_width / 2).max(1);
                    mip_height = (mip_height / 2).max(1);
                }
            }

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        } else if let Some(a) = albedo {
            // Fallback to albedo texture
            log::info!(
                "Using XTT albedo as terrain array fallback: {}x{}",
                a.width,
                a.height
            );
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Terrain Texture Array (Fallback)"),
                size: wgpu::Extent3d {
                    width: a.width,
                    height: a.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &a.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * a.width),
                    rows_per_image: Some(a.height),
                },
                wgpu::Extent3d {
                    width: a.width,
                    height: a.height,
                    depth_or_array_layers: 1,
                },
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        } else {
            // White fallback
            log::info!("Using white fallback terrain texture");
            let white = vec![255u8; 4];

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Terrain Texture Array (White)"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &white,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        }
    }

    /// Creates a 2D texture array from loaded normal map textures.
    pub(crate) fn create_normal_map_array(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        if !self.normal_textures.is_empty() {
            // All normal maps should be same size as terrain textures
            let tex_width = self.normal_textures[0].width;
            let tex_height = self.normal_textures[0].height;
            let layer_count = self.normal_textures.len() as u32;
            let num_mips = mip_level_count(tex_width, tex_height);

            log::info!(
                "Creating normal map array: {}x{} x {} layers with {} mip levels",
                tex_width,
                tex_height,
                layer_count,
                num_mips
            );

            // Normal maps should NOT be sRGB - they contain linear data
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Normal Map Array"),
                size: wgpu::Extent3d {
                    width: tex_width,
                    height: tex_height,
                    depth_or_array_layers: layer_count,
                },
                mip_level_count: num_mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm, // NOT sRGB for normal maps
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            // Upload each layer with mipmaps
            for (i, tex) in self.normal_textures.iter().enumerate() {
                // Generate mipmaps for this texture
                let mips = generate_mipmaps(&tex.pixels, tex_width, tex_height);

                // Upload each mip level
                let mut mip_width = tex_width;
                let mut mip_height = tex_height;
                for (mip_level, mip_data) in mips.iter().enumerate() {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: mip_level as u32,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: i as u32,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        mip_data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * mip_width),
                            rows_per_image: Some(mip_height),
                        },
                        wgpu::Extent3d {
                            width: mip_width,
                            height: mip_height,
                            depth_or_array_layers: 1,
                        },
                    );
                    mip_width = (mip_width / 2).max(1);
                    mip_height = (mip_height / 2).max(1);
                }
            }

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        } else {
            // Fallback: flat normal (pointing up)
            log::info!("Using flat normal fallback texture");
            // Normal map flat = (0.5, 0.5, 1.0) in tangent space = (128, 128, 255) in 0-255
            let flat_normal = vec![128u8, 128, 255, 255];

            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Normal Map Array (Fallback)"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &flat_normal,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });

            (texture, view)
        }
    }

    /// Creates the alpha atlas texture (1024x1024 RGBA, each chunk is 64x64).
    pub(crate) fn create_alpha_atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        // Per-chunk alpha texture array: 256 slices of 64×64 RGBA
        // Matches original Halo Wars approach where each chunk has its own alpha texture.
        // Each pixel's RGBA channels hold alpha weights for layers 1-4.
        const CHUNK_SIZE: u32 = 64;
        const NUM_CHUNKS: u32 = 256;

        // Each slice is 64×64×4 bytes (RGBA)
        let slice_bytes = (CHUNK_SIZE * CHUNK_SIZE * 4) as usize;
        let mut array_data = vec![0u8; slice_bytes * NUM_CHUNKS as usize];

        if !self.chunk_splat_data.is_empty() {
            log::info!(
                "Creating alpha texture array from {} chunks",
                self.chunk_splat_data.len()
            );

            for (i, chunk) in self.chunk_splat_data.iter().take(5).enumerate() {
                let non_zero: usize = chunk
                    .alpha_maps
                    .iter()
                    .flat_map(|m| m.iter())
                    .filter(|&&v| v > 0)
                    .count();
                log::info!(
                    "  Chunk {}: grid=({},{}), {} alpha maps, {} non-zero values",
                    i,
                    chunk.grid_x,
                    chunk.grid_z,
                    chunk.alpha_maps.len(),
                    non_zero
                );
            }

            for chunk in &self.chunk_splat_data {
                // X-major indexing matching original: idx = gridX * numXChunks + gridZ
                let chunk_idx = (chunk.grid_x * 16 + chunk.grid_z) as usize;
                if chunk_idx >= NUM_CHUNKS as usize {
                    continue;
                }
                let slice_offset = chunk_idx * slice_bytes;

                for y in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let src_idx = (y * CHUNK_SIZE + x) as usize;
                        let dst_idx = slice_offset + (y * CHUNK_SIZE + x) as usize * 4;

                        // R = alpha for layer 1, G = layer 2, B = layer 3, A = layer 4
                        if !chunk.alpha_maps.is_empty() && src_idx < chunk.alpha_maps[0].len() {
                            array_data[dst_idx] = chunk.alpha_maps[0][src_idx];
                        }
                        if chunk.alpha_maps.len() > 1 && src_idx < chunk.alpha_maps[1].len() {
                            array_data[dst_idx + 1] = chunk.alpha_maps[1][src_idx];
                        }
                        if chunk.alpha_maps.len() > 2 && src_idx < chunk.alpha_maps[2].len() {
                            array_data[dst_idx + 2] = chunk.alpha_maps[2][src_idx];
                        }
                        if chunk.alpha_maps.len() > 3 && src_idx < chunk.alpha_maps[3].len() {
                            array_data[dst_idx + 3] = chunk.alpha_maps[3][src_idx];
                        } else {
                            array_data[dst_idx + 3] = 255;
                        }
                    }
                }
            }
        } else {
            log::info!("No splat data, using empty alpha texture array");
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Alpha Texture Array"),
            size: wgpu::Extent3d {
                width: CHUNK_SIZE,
                height: CHUNK_SIZE,
                depth_or_array_layers: NUM_CHUNKS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &array_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * CHUNK_SIZE),
                rows_per_image: Some(CHUNK_SIZE),
            },
            wgpu::Extent3d {
                width: CHUNK_SIZE,
                height: CHUNK_SIZE,
                depth_or_array_layers: NUM_CHUNKS,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        (texture, view)
    }
}
/// Saves the alpha atlas as a PNG for debugging.
fn save_debug_atlas(atlas_data: &[u8], size: u32, path: &str) -> Result<()> {
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let w = BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header()?;
    writer.write_image_data(atlas_data)?;

    Ok(())
}
impl TerrainViewer {
    /// Creates a pre-composited albedo atlas by blending all texture layers on the CPU.
    /// This avoids the per-chunk layer index mismatch problem at chunk boundaries.
    pub(crate) fn create_composited_albedo_atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        // Composited atlas size: 2048x2048 for higher quality (128x128 per chunk)
        // Each chunk gets more pixels for better texture detail
        const ATLAS_SIZE: u32 = 2048;
        const CHUNK_SIZE: u32 = 128; // 2048 / 16 = 128 pixels per chunk
        const ALPHA_CHUNK_SIZE: u32 = 64;

        let mut atlas_data = vec![128u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize]; // Gray default

        if !self.chunk_splat_data.is_empty() && !self.terrain_textures.is_empty() {
            log::info!(
                "Creating composited albedo atlas: {}x{} ({} chunks, {} textures)",
                ATLAS_SIZE,
                ATLAS_SIZE,
                self.chunk_splat_data.len(),
                self.terrain_textures.len()
            );

            // Debug: log texture names in order
            for (i, tex) in self.terrain_textures.iter().enumerate() {
                log::info!("  CPU blend texture[{}] = {}", i, tex.name);
            }

            // Debug: log first few chunks' layer IDs
            for (i, chunk) in self.chunk_splat_data.iter().take(5).enumerate() {
                log::info!(
                    "  CPU blend chunk[{}] grid=({},{}) layers={:?}",
                    i,
                    chunk.grid_x,
                    chunk.grid_z,
                    chunk.layer_texture_ids
                );
            }

            let tex_width = self.terrain_textures[0].width;
            let tex_height = self.terrain_textures[0].height;

            for chunk in &self.chunk_splat_data {
                let chunk_x = chunk.grid_x as u32;
                let chunk_z = chunk.grid_z as u32;

                // For each pixel in this chunk's output region
                for py in 0..CHUNK_SIZE {
                    for px in 0..CHUNK_SIZE {
                        // Output position in atlas
                        let atlas_x = chunk_x * CHUNK_SIZE + px;
                        let atlas_z = chunk_z * CHUNK_SIZE + py;
                        let atlas_idx = ((atlas_z * ATLAS_SIZE + atlas_x) * 4) as usize;

                        // Calculate UV within the chunk (0-1)
                        let in_chunk_u = px as f32 / CHUNK_SIZE as f32;
                        let in_chunk_v = py as f32 / CHUNK_SIZE as f32;

                        // Sample alpha from the chunk's alpha maps
                        // Alpha maps are 64x64 per chunk
                        let alpha_x = ((in_chunk_u * ALPHA_CHUNK_SIZE as f32) as u32)
                            .min(ALPHA_CHUNK_SIZE - 1);
                        let alpha_y = ((in_chunk_v * ALPHA_CHUNK_SIZE as f32) as u32)
                            .min(ALPHA_CHUNK_SIZE - 1);
                        let alpha_idx = (alpha_y * ALPHA_CHUNK_SIZE + alpha_x) as usize;

                        // Get alpha values for each layer
                        let _alpha0: f32 = 1.0; // Base layer always 100%
                        let alpha1 = if !chunk.alpha_maps.is_empty()
                            && alpha_idx < chunk.alpha_maps[0].len()
                        {
                            chunk.alpha_maps[0][alpha_idx] as f32 / 255.0
                        } else {
                            0.0
                        };
                        let alpha2 = if chunk.alpha_maps.len() > 1
                            && alpha_idx < chunk.alpha_maps[1].len()
                        {
                            chunk.alpha_maps[1][alpha_idx] as f32 / 255.0
                        } else {
                            0.0
                        };
                        let alpha3 = if chunk.alpha_maps.len() > 2
                            && alpha_idx < chunk.alpha_maps[2].len()
                        {
                            chunk.alpha_maps[2][alpha_idx] as f32 / 255.0
                        } else {
                            0.0
                        };

                        // Calculate tiled UV for texture sampling (16x tiling for terrain textures)
                        let global_u = (chunk.grid_x as f32 + in_chunk_u) / 16.0;
                        let global_v = (chunk.grid_z as f32 + in_chunk_v) / 16.0;
                        let tiled_u = (global_u * 16.0).fract();
                        let tiled_v = (global_v * 16.0).fract();

                        // Sample position in source textures
                        let tex_x = ((tiled_u * tex_width as f32) as u32).min(tex_width - 1);
                        let tex_y = ((tiled_v * tex_height as f32) as u32).min(tex_height - 1);
                        let tex_idx = ((tex_y * tex_width + tex_x) * 4) as usize;

                        // Get layer texture IDs for this chunk
                        // Layer IDs are direct indices into active_textures/terrain_textures
                        let layer0_id = chunk.layer_texture_ids.first().copied().unwrap_or(0);
                        let layer1_id = chunk.layer_texture_ids.get(1).copied().unwrap_or(0);
                        let layer2_id = chunk.layer_texture_ids.get(2).copied().unwrap_or(0);
                        let layer3_id = chunk.layer_texture_ids.get(3).copied().unwrap_or(0);

                        // Sample base layer
                        let mut r: f32;
                        let mut g: f32;
                        let mut b: f32;

                        let layer0_idx = layer0_id as usize;
                        if layer0_idx < self.terrain_textures.len()
                            && tex_idx + 3 < self.terrain_textures[layer0_idx].pixels.len()
                        {
                            r = self.terrain_textures[layer0_idx].pixels[tex_idx] as f32;
                            g = self.terrain_textures[layer0_idx].pixels[tex_idx + 1] as f32;
                            b = self.terrain_textures[layer0_idx].pixels[tex_idx + 2] as f32;
                        } else {
                            r = 128.0;
                            g = 128.0;
                            b = 128.0;
                        }

                        // Blend layer 1 - ONLY if layer ID is non-zero (ID=0 means padding)
                        let layer1_idx = layer1_id as usize;
                        if layer1_id > 0
                            && alpha1 > 0.0
                            && layer1_idx < self.terrain_textures.len()
                            && tex_idx + 3 < self.terrain_textures[layer1_idx].pixels.len()
                        {
                            let lr = self.terrain_textures[layer1_idx].pixels[tex_idx] as f32;
                            let lg = self.terrain_textures[layer1_idx].pixels[tex_idx + 1] as f32;
                            let lb = self.terrain_textures[layer1_idx].pixels[tex_idx + 2] as f32;
                            r = r * (1.0 - alpha1) + lr * alpha1;
                            g = g * (1.0 - alpha1) + lg * alpha1;
                            b = b * (1.0 - alpha1) + lb * alpha1;
                        }

                        // Blend layer 2 - ONLY if layer ID is non-zero
                        let layer2_idx = layer2_id as usize;
                        if layer2_id > 0
                            && alpha2 > 0.0
                            && layer2_idx < self.terrain_textures.len()
                            && tex_idx + 3 < self.terrain_textures[layer2_idx].pixels.len()
                        {
                            let lr = self.terrain_textures[layer2_idx].pixels[tex_idx] as f32;
                            let lg = self.terrain_textures[layer2_idx].pixels[tex_idx + 1] as f32;
                            let lb = self.terrain_textures[layer2_idx].pixels[tex_idx + 2] as f32;
                            r = r * (1.0 - alpha2) + lr * alpha2;
                            g = g * (1.0 - alpha2) + lg * alpha2;
                            b = b * (1.0 - alpha2) + lb * alpha2;
                        }

                        // Blend layer 3 - ONLY if layer ID is non-zero
                        let layer3_idx = layer3_id as usize;
                        if layer3_id > 0
                            && alpha3 > 0.0
                            && layer3_idx < self.terrain_textures.len()
                            && tex_idx + 3 < self.terrain_textures[layer3_idx].pixels.len()
                        {
                            let lr = self.terrain_textures[layer3_idx].pixels[tex_idx] as f32;
                            let lg = self.terrain_textures[layer3_idx].pixels[tex_idx + 1] as f32;
                            let lb = self.terrain_textures[layer3_idx].pixels[tex_idx + 2] as f32;
                            r = r * (1.0 - alpha3) + lr * alpha3;
                            g = g * (1.0 - alpha3) + lg * alpha3;
                            b = b * (1.0 - alpha3) + lb * alpha3;
                        }

                        atlas_data[atlas_idx] = r.clamp(0.0, 255.0) as u8;
                        atlas_data[atlas_idx + 1] = g.clamp(0.0, 255.0) as u8;
                        atlas_data[atlas_idx + 2] = b.clamp(0.0, 255.0) as u8;
                        atlas_data[atlas_idx + 3] = 255;
                    }
                }
            }

            // Save debug image
            if let Err(e) = save_debug_atlas(&atlas_data, ATLAS_SIZE, "/tmp/composited_albedo.png")
            {
                log::warn!("Failed to save composited albedo debug: {}", e);
            } else {
                log::info!("Saved composited albedo to /tmp/composited_albedo.png");
            }
        } else {
            log::info!("No splat data or textures, using gray composited atlas");
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Composited Albedo Atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * ATLAS_SIZE),
                rows_per_image: Some(ATLAS_SIZE),
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Creates the XTT albedo texture (original pre-composited from game export).
    /// This is the unique texture that the game's export tools created with proper blending.
    pub(crate) fn create_xtt_albedo_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        albedo: &Option<AlbedoData>,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let (tex_width, tex_height, tex_data) = if let Some(a) = albedo {
            log::info!("Creating XTT albedo texture: {}x{}", a.width, a.height);
            (a.width, a.height, a.pixels.clone())
        } else {
            log::info!("No XTT albedo, using gray fallback");
            (1, 1, vec![128u8, 128, 128, 255])
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("XTT Albedo Texture"),
            size: wgpu::Extent3d {
                width: tex_width,
                height: tex_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tex_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * tex_width),
                rows_per_image: Some(tex_height),
            },
            wgpu::Extent3d {
                width: tex_width,
                height: tex_height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }
}
