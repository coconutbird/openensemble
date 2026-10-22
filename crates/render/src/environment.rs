//! HDR environment-cubemap loading and GPU upload.
//!
//! Halo Wars PC environment maps are six-face `A16B16G16R16F` DDX/DDS
//! resources. Definitive Edition stores these as legacy DDS files whose
//! numeric D3D9 `FourCC` is `113`. The [`ddx`](pipeline::ddx) reader decodes
//! both containers and splits the faces and mips. This module checks that the
//! texture is a complete HDR cube and uploads it.

use std::ops::Range;

use pipeline::ddx::{
    D3DFMT_A16B16G16R16F, DDS_PIXEL_FORMAT_SIZE, DataFormat, DdsHeader, DdxTexture, ResourceType,
    TextureInfo, TextureSource, mip_extent,
};
use pipeline::source::{AssetSource, StdFileProvider};

use crate::wgpu;

const CUBE_FACE_COUNT: usize = 6;
const CUBE_FACE_COUNT_U32: u32 = 6;
const RGBA16_FLOAT_BYTES_PER_PIXEL: usize = 8;
const RGBA16_FLOAT_BYTES_PER_PIXEL_U32: u32 = 8;

/// Errors produced while resolving or validating an HDR environment cube.
#[derive(Debug, thiserror::Error)]
pub enum EnvironmentMapError {
    /// The requested environment asset was absent from the active stack.
    #[error("environment map not found: {0}")]
    NotFound(String),
    /// The DDX/DDS container could not be parsed or was not a supported cube.
    #[error("invalid environment map '{path}': {reason}")]
    Invalid {
        /// Canonical game asset path.
        path: String,
        /// Validation diagnostic.
        reason: String,
    },
}

/// A normalized six-face HDR cubemap ready for GPU upload.
///
/// Subresources are byte ranges of `data` with face-major indexing:
/// `face * mip_count + mip`.
#[derive(Clone, Debug)]
pub struct EnvironmentMap {
    path: String,
    size: u32,
    mip_count: u32,
    hdr_scale: f32,
    data: Vec<u8>,
    subresources: Vec<Range<usize>>,
}

impl EnvironmentMap {
    /// Resolves and validates a DDX environment map.
    ///
    /// `path` may include or omit the `art\` prefix and `.ddx` suffix.
    ///
    /// # Errors
    ///
    /// Returns [`EnvironmentMapError::NotFound`] when the asset cannot be
    /// resolved, or [`EnvironmentMapError::Invalid`] when its contents are not
    /// a complete supported HDR cubemap.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        path: &str,
    ) -> Result<Self, EnvironmentMapError> {
        let canonical_path = canonical_environment_path(path);
        let bytes = source
            .resolve_with_fallback(&canonical_path, &[".ddx"])
            .ok_or_else(|| EnvironmentMapError::NotFound(canonical_path.clone()))?;
        Self::from_bytes(canonical_path, &bytes)
    }

    /// Validates environment-map bytes from either a DDS or ECF-wrapped DDX.
    ///
    /// # Errors
    ///
    /// Returns [`EnvironmentMapError::Invalid`] when the container metadata,
    /// dimensions, mip chain, or payload layout is invalid.
    pub fn from_bytes(path: impl Into<String>, bytes: &[u8]) -> Result<Self, EnvironmentMapError> {
        let path = path.into();
        parse_cube(&path, bytes).map_err(|reason| EnvironmentMapError::Invalid {
            path: path.clone(),
            reason,
        })
    }

    /// Canonical game path used to resolve this cubemap.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Width and height of mip zero. Environment maps are square.
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Number of complete mip levels stored for every face.
    #[must_use]
    pub fn mip_count(&self) -> u32 {
        self.mip_count
    }

    /// Multiplier applied after the oracle's `sample.a * sample.rgb` decode.
    #[must_use]
    pub fn hdr_scale(&self) -> f32 {
        self.hdr_scale
    }

    /// Uploads the complete mip chain and returns a cube texture view.
    #[must_use]
    pub fn create_texture_view(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
    ) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: self.size,
                height: self.size,
                depth_or_array_layers: CUBE_FACE_COUNT_U32,
            },
            mip_level_count: self.mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let subresource_coordinates = (0..CUBE_FACE_COUNT_U32)
            .flat_map(|face| (0..self.mip_count).map(move |mip| (face, mip)));
        for ((face, mip), subresource) in subresource_coordinates.zip(self.subresources.iter()) {
            let subresource = &self.data[subresource.clone()];
            let mip_size = mip_extent(self.size, mip);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: face,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                subresource,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip_size * RGBA16_FLOAT_BYTES_PER_PIXEL_U32),
                    rows_per_image: Some(mip_size),
                },
                wgpu::Extent3d {
                    width: mip_size,
                    height: mip_size,
                    depth_or_array_layers: 1,
                },
            );
        }
        texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(label),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    pub(crate) fn black() -> Self {
        Self {
            path: "<black environment fallback>".to_owned(),
            size: 1,
            mip_count: 1,
            hdr_scale: 1.0,
            data: vec![0; RGBA16_FLOAT_BYTES_PER_PIXEL * CUBE_FACE_COUNT],
            subresources: (0..CUBE_FACE_COUNT)
                .map(|face| {
                    let start = face * RGBA16_FLOAT_BYTES_PER_PIXEL;
                    start..start + RGBA16_FLOAT_BYTES_PER_PIXEL
                })
                .collect(),
        }
    }
}

fn parse_cube(path: &str, bytes: &[u8]) -> Result<EnvironmentMap, String> {
    let texture = DdxTexture::from_bytes(bytes).map_err(|error| error.to_string())?;
    let mut layout = texture.subresource_layout();
    match &texture.source {
        TextureSource::Dds(header) => {
            validate_pc_dds(header)?;
            // The numeric FourCC and the stored mip count are authoritative,
            // also when DDPF_FOURCC or DDSD_MIPMAPCOUNT is clear.
            layout.data_format = DataFormat::A16B16G16R16F;
            layout.faces = CUBE_FACE_COUNT_U32;
            layout.mip_levels = header.mip_map_count.max(1);
        }
        TextureSource::Ddx(_) => validate_ecf_ddx(&texture.info)?,
    }
    validate_dimensions(layout.width, layout.height, layout.mip_levels)?;
    let subresources = layout
        .ranges(texture.data.len())
        .map_err(|error| error.to_string())?;
    Ok(EnvironmentMap {
        path: path.to_owned(),
        size: layout.width,
        mip_count: layout.mip_levels,
        hdr_scale: texture.info.hdr_scale,
        data: texture.data,
        subresources,
    })
}

fn validate_pc_dds(header: &DdsHeader) -> Result<(), String> {
    if header.pixel_format.size != DDS_PIXEL_FORMAT_SIZE {
        return Err("DDS pixel-format size is not 32 bytes".to_owned());
    }
    if header.pixel_format.four_cc_code() != D3DFMT_A16B16G16R16F {
        return Err("DDS is not D3DFMT_A16B16G16R16F (numeric FourCC 113)".to_owned());
    }
    if !header.is_complete_cubemap() {
        return Err(format!(
            "DDS does not contain all six cubemap faces (caps2=0x{:08x})",
            header.caps2
        ));
    }
    Ok(())
}

fn validate_ecf_ddx(info: &TextureInfo) -> Result<(), String> {
    if info.resource_type != ResourceType::CubeMap {
        return Err(format!(
            "DDX resource is {:?}, expected CubeMap",
            info.resource_type
        ));
    }
    if info.data_format != DataFormat::A16B16G16R16F {
        return Err(format!(
            "DDX format is {:?}, expected A16B16G16R16F",
            info.data_format
        ));
    }
    Ok(())
}

fn validate_dimensions(width: u32, height: u32, mip_count: u32) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("cubemap dimensions must be nonzero".to_owned());
    }
    if width != height {
        return Err(format!("cubemap is not square: {width}x{height}"));
    }
    let maximum_mips = width.bit_width();
    if mip_count == 0 || mip_count > maximum_mips {
        return Err(format!(
            "cubemap has {mip_count} mip levels; {width}x{height} permits at most {maximum_mips}"
        ));
    }
    Ok(())
}

fn canonical_environment_path(path: &str) -> String {
    let normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}

#[cfg(test)]
mod tests {
    use pipeline::ddx::DDSCAPS2_CUBEMAP_ALL_FACES;

    use super::{
        CUBE_FACE_COUNT, D3DFMT_A16B16G16R16F, DDS_PIXEL_FORMAT_SIZE, EnvironmentMap,
        RGBA16_FLOAT_BYTES_PER_PIXEL, canonical_environment_path,
    };

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn two_by_two_cube() -> Vec<u8> {
        let mut bytes = Vec::with_capacity(128 + 240);
        bytes.extend_from_slice(b"DDS ");
        push_u32(&mut bytes, 124);
        push_u32(&mut bytes, 0x0002_100F);
        push_u32(&mut bytes, 2);
        push_u32(&mut bytes, 2);
        push_u32(&mut bytes, 16);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 2);
        for _ in 0..11 {
            push_u32(&mut bytes, 0);
        }
        push_u32(&mut bytes, DDS_PIXEL_FORMAT_SIZE);
        push_u32(&mut bytes, 4);
        push_u32(&mut bytes, D3DFMT_A16B16G16R16F);
        for _ in 0..5 {
            push_u32(&mut bytes, 0);
        }
        push_u32(&mut bytes, 0x0040_1008);
        push_u32(&mut bytes, DDSCAPS2_CUBEMAP_ALL_FACES);
        for _ in 0..3 {
            push_u32(&mut bytes, 0);
        }
        assert_eq!(bytes.len(), 128);
        for face in 0..CUBE_FACE_COUNT {
            bytes.extend(std::iter::repeat_n(
                u8::try_from(face).unwrap(),
                4 * RGBA16_FLOAT_BYTES_PER_PIXEL,
            ));
            bytes.extend(std::iter::repeat_n(
                u8::try_from(face + 16).unwrap(),
                RGBA16_FLOAT_BYTES_PER_PIXEL,
            ));
        }
        bytes
    }

    #[test]
    fn pc_numeric_fourcc_cube_is_split_face_then_mip() {
        let cube = EnvironmentMap::from_bytes("art\\test.ddx", &two_by_two_cube())
            .expect("synthetic cubemap must parse");
        assert_eq!(cube.size(), 2);
        assert_eq!(cube.mip_count(), 2);
        assert_eq!(cube.subresources.len(), 12);
        let subresource = |index: usize| &cube.data[cube.subresources[index].clone()];
        assert_eq!(subresource(0), [0; 32]);
        assert_eq!(subresource(1), [16; 8]);
        assert_eq!(subresource(10), [5; 32]);
        assert_eq!(subresource(11), [21; 8]);
    }

    #[test]
    fn environment_paths_are_resolved_from_art() {
        assert_eq!(
            canonical_environment_path("environment/sky/cloudy_en.ddx"),
            "art\\environment\\sky\\cloudy_en.ddx"
        );
        assert_eq!(
            canonical_environment_path("art\\environment\\sky\\cloudy_en"),
            "art\\environment\\sky\\cloudy_en"
        );
    }
}
