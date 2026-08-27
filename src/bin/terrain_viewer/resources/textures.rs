//! Texture creation for terrain rendering.
//!
//! Creates GPU texture arrays and atlases from loaded terrain data.

use render::terrain::{
    DecalTexture, NormalMapTexture, SpecularMapTexture, TerrainTexture, generate_mipmaps,
    mip_level_count,
};
use render::wgpu;

use crate::types::AlbedoData;
use crate::viewer::TerrainViewer;

const ALPHA_CHUNK_SIZE: u32 = 64;
const ALPHA_CHUNK_SIZE_USIZE: usize = 64;
const ALPHA_CHUNK_COUNT: u32 = 256;
const ALPHA_CHUNK_COUNT_USIZE: usize = 256;
const ALPHA_CHANNEL_COUNT: usize = 4;
const ALPHA_CHANNEL_COUNT_U32: u32 = 4;
const ALPHA_SLICE_BYTES: usize =
    ALPHA_CHUNK_SIZE_USIZE * ALPHA_CHUNK_SIZE_USIZE * ALPHA_CHANNEL_COUNT;

fn create_array_texture(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    layers: u32,
    mip_levels: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: mip_levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn array_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

fn upload_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    pixels: &[u8],
    width: u32,
    height: u32,
    layer: u32,
    mip_level: u32,
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

fn upload_mip_chain(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    pixels: &[u8],
    width: u32,
    height: u32,
    layer: u32,
) {
    let mips = generate_mipmaps(pixels, width, height);
    let mut mip_width = width;
    let mut mip_height = height;

    for (mip_level, mip_data) in mips.iter().enumerate() {
        let mip_level = u32::try_from(mip_level).expect("mipmap count must fit in u32");
        upload_texture(
            queue, texture, mip_data, mip_width, mip_height, layer, mip_level,
        );
        mip_width = (mip_width / 2).max(1);
        mip_height = (mip_height / 2).max(1);
    }
}

fn create_terrain_array_from_layers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    textures: &[TerrainTexture],
) -> (wgpu::Texture, wgpu::TextureView) {
    let width = textures[0].width;
    let height = textures[0].height;
    let layers = u32::try_from(textures.len()).expect("terrain texture count must fit in u32");
    let mip_levels = mip_level_count(width, height);
    log::info!(
        "Creating terrain texture array: {width}x{height} x {layers} layers with {mip_levels} mip levels"
    );

    let texture = create_array_texture(
        device,
        "Terrain Texture Array",
        width,
        height,
        layers,
        mip_levels,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    for (layer, terrain_texture) in textures.iter().enumerate() {
        let layer = u32::try_from(layer).expect("terrain texture index must fit in u32");
        upload_mip_chain(
            queue,
            &texture,
            &terrain_texture.pixels,
            width,
            height,
            layer,
        );
    }

    let view = array_view(&texture);
    (texture, view)
}

fn create_single_terrain_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    albedo: Option<&AlbedoData>,
) -> (wgpu::Texture, wgpu::TextureView) {
    let (label, width, height, pixels): (&str, u32, u32, &[u8]) = albedo.map_or_else(
        || {
            log::info!("Using white fallback terrain texture");
            ("Terrain Texture Array (White)", 1, 1, &[255_u8; 4][..])
        },
        |value| {
            log::info!(
                "Using XTT albedo as terrain array fallback: {}x{}",
                value.width,
                value.height
            );
            (
                "Terrain Texture Array (Fallback)",
                value.width,
                value.height,
                value.pixels.as_slice(),
            )
        },
    );
    let texture = create_array_texture(
        device,
        label,
        width,
        height,
        1,
        1,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    upload_texture(queue, &texture, pixels, width, height, 0, 0);
    let view = array_view(&texture);
    (texture, view)
}

fn create_normal_array_from_layers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    textures: &[NormalMapTexture],
) -> (wgpu::Texture, wgpu::TextureView) {
    let width = textures[0].width;
    let height = textures[0].height;
    let layers = u32::try_from(textures.len()).expect("normal texture count must fit in u32");
    let mip_levels = mip_level_count(width, height);
    log::info!(
        "Creating normal map array: {width}x{height} x {layers} layers with {mip_levels} mip levels"
    );

    let texture = create_array_texture(
        device,
        "Normal Map Array",
        width,
        height,
        layers,
        mip_levels,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    for (layer, normal_texture) in textures.iter().enumerate() {
        let layer = u32::try_from(layer).expect("normal texture index must fit in u32");
        upload_mip_chain(
            queue,
            &texture,
            &normal_texture.pixels,
            width,
            height,
            layer,
        );
    }

    let view = array_view(&texture);
    (texture, view)
}

fn create_flat_normal_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView) {
    log::info!("Using flat normal fallback texture");
    let texture = create_array_texture(
        device,
        "Normal Map Array (Fallback)",
        1,
        1,
        1,
        1,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    upload_texture(queue, &texture, &[128, 128, 255, 255], 1, 1, 0, 0);
    let view = array_view(&texture);
    (texture, view)
}

fn create_specular_array_from_layers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    textures: &[SpecularMapTexture],
) -> (wgpu::Texture, wgpu::TextureView) {
    let width = textures[0].width;
    let height = textures[0].height;
    let layers = u32::try_from(textures.len()).expect("specular texture count must fit in u32");
    let mip_levels = mip_level_count(width, height);
    log::info!(
        "Creating specular map array: {width}x{height} x {layers} layers with {mip_levels} mip levels"
    );

    let texture = create_array_texture(
        device,
        "Specular Map Array",
        width,
        height,
        layers,
        mip_levels,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    for (layer, specular_texture) in textures.iter().enumerate() {
        let layer = u32::try_from(layer).expect("specular texture index must fit in u32");
        upload_mip_chain(
            queue,
            &texture,
            &specular_texture.pixels,
            width,
            height,
            layer,
        );
    }

    let view = array_view(&texture);
    (texture, view)
}

fn create_black_specular_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView) {
    log::info!("Using black specular fallback texture");
    let texture = create_array_texture(
        device,
        "Specular Map Array (Fallback)",
        1,
        1,
        1,
        1,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    upload_texture(queue, &texture, &[0, 0, 0, 255], 1, 1, 0, 0);
    let view = array_view(&texture);
    (texture, view)
}

fn resize_rgba_nearest(
    pixels: &[u8],
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> Vec<u8> {
    if source_width == target_width && source_height == target_height {
        return pixels.to_vec();
    }
    let target_texels = target_width
        .checked_mul(target_height)
        .and_then(|count| usize::try_from(count).ok())
        .expect("resized texture dimensions must fit usize");
    let mut resized = vec![0_u8; target_texels * 4];
    for target_y in 0..target_height {
        let source_y = target_y * source_height / target_height;
        for target_x in 0..target_width {
            let source_x = target_x * source_width / target_width;
            let source_index = usize::try_from((source_y * source_width + source_x) * 4)
                .expect("source texel offset must fit usize");
            let target_index = usize::try_from((target_y * target_width + target_x) * 4)
                .expect("target texel offset must fit usize");
            resized[target_index..target_index + 4]
                .copy_from_slice(&pixels[source_index..source_index + 4]);
        }
    }
    resized
}

fn create_decal_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    textures: &[DecalTexture],
    opacity: bool,
) -> (wgpu::Texture, wgpu::TextureView) {
    let width = textures
        .iter()
        .map(|texture| texture.width)
        .max()
        .unwrap_or(1);
    let height = textures
        .iter()
        .map(|texture| texture.height)
        .max()
        .unwrap_or(1);
    let layers = u32::try_from(textures.len().max(1)).expect("decal count must fit u32");
    let mip_levels = mip_level_count(width, height);
    let (label, format) = if opacity {
        ("Decal Opacity Array", wgpu::TextureFormat::Rgba8Unorm)
    } else {
        ("Decal Diffuse Array", wgpu::TextureFormat::Rgba8UnormSrgb)
    };
    let texture = create_array_texture(device, label, width, height, layers, mip_levels, format);
    if textures.is_empty() {
        let fallback = [0_u8, 0, 0, 255];
        upload_texture(queue, &texture, &fallback, 1, 1, 0, 0);
    } else {
        for (layer, decal) in textures.iter().enumerate() {
            let source = if opacity {
                &decal.opacity_pixels
            } else {
                &decal.diffuse_pixels
            };
            let pixels = resize_rgba_nearest(source, decal.width, decal.height, width, height);
            upload_mip_chain(
                queue,
                &texture,
                &pixels,
                width,
                height,
                u32::try_from(layer).expect("decal layer index must fit u32"),
            );
        }
    }
    let view = array_view(&texture);
    (texture, view)
}

fn alpha_chunk_index(grid_x: i32, grid_z: i32) -> Option<usize> {
    let grid_x = usize::try_from(grid_x).ok()?;
    let grid_z = usize::try_from(grid_z).ok()?;
    grid_x
        .checked_mul(16)?
        .checked_add(grid_z)
        .filter(|&index| index < ALPHA_CHUNK_COUNT_USIZE)
}

fn empty_alpha_array() -> Vec<u8> {
    vec![0; ALPHA_SLICE_BYTES * ALPHA_CHUNK_COUNT_USIZE]
}

fn populate_alpha_data<'a>(
    array_data: &mut [u8],
    chunks: impl IntoIterator<Item = (i32, i32, &'a [Vec<u8>])>,
    first_map: usize,
    channel_count: usize,
) {
    for (grid_x, grid_z, alpha_maps) in chunks {
        let Some(chunk_index) = alpha_chunk_index(grid_x, grid_z) else {
            continue;
        };
        let slice_offset = chunk_index * ALPHA_SLICE_BYTES;

        for row in 0..ALPHA_CHUNK_SIZE_USIZE {
            for column in 0..ALPHA_CHUNK_SIZE_USIZE {
                // XTT stores x=Z and y=X, so transpose while building the texture.
                let source_index = column * ALPHA_CHUNK_SIZE_USIZE + row;
                let target_index =
                    slice_offset + (row * ALPHA_CHUNK_SIZE_USIZE + column) * ALPHA_CHANNEL_COUNT;
                for channel in 0..channel_count {
                    let map_index = first_map + channel;
                    if let Some(alpha_map) = alpha_maps.get(map_index)
                        && let Some(&alpha) = alpha_map.get(source_index)
                    {
                        array_data[target_index + channel] = alpha;
                    }
                }
            }
        }
    }
}

fn create_alpha_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    array_data: &[u8],
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = create_array_texture(
        device,
        label,
        ALPHA_CHUNK_SIZE,
        ALPHA_CHUNK_SIZE,
        ALPHA_CHUNK_COUNT,
        1,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        array_data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(ALPHA_CHANNEL_COUNT_U32 * ALPHA_CHUNK_SIZE),
            rows_per_image: Some(ALPHA_CHUNK_SIZE),
        },
        wgpu::Extent3d {
            width: ALPHA_CHUNK_SIZE,
            height: ALPHA_CHUNK_SIZE,
            depth_or_array_layers: ALPHA_CHUNK_COUNT,
        },
    );
    let view = array_view(&texture);
    (texture, view)
}

impl TerrainViewer {
    /// Creates a 2D texture array from loaded terrain textures.
    pub(crate) fn create_terrain_texture_array(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        albedo: Option<&AlbedoData>,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let scene = self.scene.as_ref().expect("scene must be loaded");
        if scene.terrain_textures.is_empty() {
            create_single_terrain_array(device, queue, albedo)
        } else {
            create_terrain_array_from_layers(device, queue, &scene.terrain_textures)
        }
    }

    /// Creates a 2D texture array from loaded normal map textures.
    pub(crate) fn create_normal_map_array(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let scene = self.scene.as_ref().expect("scene must be loaded");
        if scene.normal_textures.is_empty() {
            create_flat_normal_array(device, queue)
        } else {
            create_normal_array_from_layers(device, queue, &scene.normal_textures)
        }
    }

    /// Creates a 2D texture array from loaded specular map textures.
    pub(crate) fn create_specular_map_array(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let scene = self.scene.as_ref().expect("scene must be loaded");
        if scene.specular_textures.is_empty() {
            create_black_specular_array(device, queue)
        } else {
            create_specular_array_from_layers(device, queue, &scene.specular_textures)
        }
    }

    /// Creates the alpha atlas texture (256 slices of 64x64 RGBA pixels).
    pub(crate) fn create_alpha_atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let chunks = &self
            .scene
            .as_ref()
            .expect("scene must be loaded")
            .chunk_splat_data;
        if chunks.is_empty() {
            log::info!("No splat data, using empty alpha texture array");
        } else {
            log::info!("Creating alpha texture array from {} chunks", chunks.len());
            for (index, chunk) in chunks.iter().take(5).enumerate() {
                let non_zero = chunk
                    .alpha_maps
                    .iter()
                    .flatten()
                    .filter(|&&value| value > 0)
                    .count();
                log::info!(
                    "  Chunk {index}: grid=({},{}), {} alpha maps, {non_zero} non-zero values",
                    chunk.grid_x,
                    chunk.grid_z,
                    chunk.alpha_maps.len()
                );
            }
        }

        let mut data = empty_alpha_array();
        populate_alpha_data(
            &mut data,
            chunks
                .iter()
                .map(|chunk| (chunk.grid_x, chunk.grid_z, chunk.alpha_maps.as_slice())),
            0,
            4,
        );
        create_alpha_texture(device, queue, "Alpha Texture Array", &data)
    }

    /// Creates the high alpha atlas for overflow layers 5 through 7.
    pub(crate) fn create_alpha_atlas_hi(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let chunks = &self
            .scene
            .as_ref()
            .expect("scene must be loaded")
            .chunk_splat_data;
        let mut data = empty_alpha_array();
        populate_alpha_data(
            &mut data,
            chunks
                .iter()
                .map(|chunk| (chunk.grid_x, chunk.grid_z, chunk.alpha_maps.as_slice())),
            4,
            3,
        );
        create_alpha_texture(device, queue, "Alpha Texture Array Hi", &data)
    }

    /// Creates the decal alpha atlas texture.
    pub(crate) fn create_decal_alpha_atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let chunks = &self
            .scene
            .as_ref()
            .expect("scene must be loaded")
            .chunk_decal_data;
        if chunks.is_empty() {
            log::info!("No decal data, using empty decal alpha atlas");
        } else {
            log::info!(
                "Creating decal alpha atlas from {} chunks with decals",
                chunks.len()
            );
        }

        let mut data = empty_alpha_array();
        populate_alpha_data(
            &mut data,
            chunks
                .iter()
                .map(|chunk| (chunk.grid_x, chunk.grid_z, chunk.alpha_maps.as_slice())),
            0,
            4,
        );
        create_alpha_texture(device, queue, "Decal Alpha Atlas", &data)
    }

    /// Creates the high decal alpha atlas for decal layers 4 through 7.
    pub(crate) fn create_decal_alpha_atlas_hi(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let chunks = &self
            .scene
            .as_ref()
            .expect("scene must be loaded")
            .chunk_decal_data;
        let mut data = empty_alpha_array();
        populate_alpha_data(
            &mut data,
            chunks
                .iter()
                .map(|chunk| (chunk.grid_x, chunk.grid_z, chunk.alpha_maps.as_slice())),
            4,
            4,
        );
        create_alpha_texture(device, queue, "Decal Alpha Atlas Hi", &data)
    }

    /// Uploads the actual decal diffuse and opacity resources as aligned arrays.
    pub(crate) fn create_decal_texture_arrays(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::TextureView, wgpu::TextureView) {
        let decals = &self
            .scene
            .as_ref()
            .expect("scene must be loaded")
            .decal_textures;
        let (_, diffuse) = create_decal_array(device, queue, decals, false);
        let (_, opacity) = create_decal_array(device, queue, decals, true);
        (diffuse, opacity)
    }
}
