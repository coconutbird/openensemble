//! Texture creation for terrain rendering.
//!
//! Creates GPU texture arrays and atlases from loaded terrain data.

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
        let scene = self.scene.as_ref().expect("scene must be loaded");
        let terrain_textures = &scene.terrain_textures;
        // Use terrain textures if available, otherwise fall back to albedo or white
        if !terrain_textures.is_empty() {
            // All textures should be same size (e.g., 1024x1024)
            let tex_width = terrain_textures[0].width;
            let tex_height = terrain_textures[0].height;
            let layer_count = terrain_textures.len() as u32;
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
            for (i, tex) in terrain_textures.iter().enumerate() {
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
        let scene = self.scene.as_ref().expect("scene must be loaded");
        let normal_textures = &scene.normal_textures;
        if !normal_textures.is_empty() {
            // All normal maps should be same size as terrain textures
            let tex_width = normal_textures[0].width;
            let tex_height = normal_textures[0].height;
            let layer_count = normal_textures.len() as u32;
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
            for (i, tex) in normal_textures.iter().enumerate() {
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
        let scene = self.scene.as_ref().expect("scene must be loaded");
        let chunk_splat_data = &scene.chunk_splat_data;
        // Per-chunk alpha texture array: 256 slices of 64×64 RGBA
        // Matches original Halo Wars approach where each chunk has its own alpha texture.
        // Each pixel's RGBA channels hold alpha weights for layers 1-4.
        const CHUNK_SIZE: u32 = 64;
        const NUM_CHUNKS: u32 = 256;

        // Each slice is 64×64×4 bytes (RGBA)
        let slice_bytes = (CHUNK_SIZE * CHUNK_SIZE * 4) as usize;
        let mut array_data = vec![0u8; slice_bytes * NUM_CHUNKS as usize];

        if !chunk_splat_data.is_empty() {
            log::info!(
                "Creating alpha texture array from {} chunks",
                chunk_splat_data.len()
            );

            for (i, chunk) in chunk_splat_data.iter().take(5).enumerate() {
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

            for chunk in chunk_splat_data {
                // X-major chunk indexing (game convention): gridX * numZChunks + gridZ
                let chunk_idx = (chunk.grid_x * 16 + chunk.grid_z) as usize;
                if chunk_idx >= NUM_CHUNKS as usize {
                    continue;
                }
                let slice_offset = chunk_idx * slice_bytes;

                for y in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        // Decoded alpha has x=Z, y=X (XTT convention). Transpose so that
                        // texture columns=X, rows=Z — matching sample_uv = (X→U, Z→V).
                        let src_idx = (x * CHUNK_SIZE + y) as usize; // transpose: swap x↔y
                        let dst_idx = slice_offset + (y * CHUNK_SIZE + x) as usize * 4;

                        // R = alpha for layer 1, G = layer 2, B = layer 3, A = layer 4
                        // Unused layers get 0 alpha (not 255!) so they don't blend.
                        for ch in 0..4usize {
                            if chunk.alpha_maps.len() > ch && src_idx < chunk.alpha_maps[ch].len() {
                                array_data[dst_idx + ch] = chunk.alpha_maps[ch][src_idx];
                            }
                            // else: already 0 from vec initialization
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

    /// Creates the high alpha atlas texture for layers 5-7 (overflow layers).
    /// Same format as create_alpha_atlas but stores alpha maps [4], [5], [6] in R, G, B.
    pub(crate) fn create_alpha_atlas_hi(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let scene = self.scene.as_ref().expect("scene must be loaded");
        let chunk_splat_data = &scene.chunk_splat_data;
        const CHUNK_SIZE: u32 = 64;
        const NUM_CHUNKS: u32 = 256;

        let slice_bytes = (CHUNK_SIZE * CHUNK_SIZE * 4) as usize;
        let mut array_data = vec![0u8; slice_bytes * NUM_CHUNKS as usize];

        if !chunk_splat_data.is_empty() {
            for chunk in chunk_splat_data {
                let chunk_idx = (chunk.grid_x * 16 + chunk.grid_z) as usize;
                if chunk_idx >= NUM_CHUNKS as usize {
                    continue;
                }
                let slice_offset = chunk_idx * slice_bytes;

                // Only process chunks that have > 4 alpha maps
                if chunk.alpha_maps.len() <= 4 {
                    continue;
                }

                for y in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let src_idx = (x * CHUNK_SIZE + y) as usize; // transpose
                        let dst_idx = slice_offset + (y * CHUNK_SIZE + x) as usize * 4;

                        // R = alpha for layer 5, G = layer 6, B = layer 7, A = unused
                        for ch in 0..3usize {
                            let map_idx = 4 + ch;
                            if chunk.alpha_maps.len() > map_idx
                                && src_idx < chunk.alpha_maps[map_idx].len()
                            {
                                array_data[dst_idx + ch] = chunk.alpha_maps[map_idx][src_idx];
                            }
                        }
                    }
                }
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Alpha Texture Array Hi"),
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

impl TerrainViewer {
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
