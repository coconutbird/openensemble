//! HDR environment-cubemap loading and GPU upload.
//!
//! Halo Wars PC environment maps are six-face `A16B16G16R16F` DDX/DDS
//! resources. Definitive Edition stores these as legacy DDS files whose
//! numeric D3D9 `FourCC` is `113`; the general DDX decoder intentionally
//! exposes the payload but does not reinterpret that renderer-specific DDS
//! metadata. This module validates and normalizes both container layouts
//! without changing the format readers.

use pipeline::ddx::{DataFormat, DdxTexture, ResourceType};
use pipeline::source::{AssetSource, StdFileProvider};

use crate::wgpu;

const DDS_MAGIC: &[u8; 4] = b"DDS ";
const DDS_HEADER_SIZE: usize = 128;
const DDS_PIXEL_FORMAT_OFFSET: usize = 76;
const DDS_PIXEL_FORMAT_SIZE: u32 = 32;
const DDS_FOUR_CC_OFFSET: usize = 84;
const DDS_CAPS2_OFFSET: usize = 112;
const D3DFMT_A16B16G16R16F: u32 = 113;
const DDS_CUBEMAP_ALL_FACES: u32 = 0x0000_FE00;
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
/// Subresources use face-major indexing: `face * mip_count + mip`.
#[derive(Clone, Debug)]
pub struct EnvironmentMap {
    path: String,
    size: u32,
    mip_count: u32,
    hdr_scale: f32,
    subresources: Vec<Vec<u8>>,
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
        let result = if bytes.starts_with(DDS_MAGIC) {
            parse_pc_dds(&path, bytes)
        } else {
            parse_ecf_ddx(&path, bytes)
        };
        result.map_err(|reason| EnvironmentMapError::Invalid {
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
            subresources: vec![vec![0; RGBA16_FLOAT_BYTES_PER_PIXEL]; CUBE_FACE_COUNT],
        }
    }
}

fn parse_pc_dds(path: &str, bytes: &[u8]) -> Result<EnvironmentMap, String> {
    if bytes.len() < DDS_HEADER_SIZE {
        return Err(format!(
            "DDS header is truncated: {} bytes, expected at least {DDS_HEADER_SIZE}",
            bytes.len()
        ));
    }
    if read_u32(bytes, 4)? != 124 {
        return Err("DDS header size is not 124 bytes".to_owned());
    }
    if read_u32(bytes, DDS_PIXEL_FORMAT_OFFSET)? != DDS_PIXEL_FORMAT_SIZE {
        return Err("DDS pixel-format size is not 32 bytes".to_owned());
    }
    if read_u32(bytes, DDS_FOUR_CC_OFFSET)? != D3DFMT_A16B16G16R16F {
        return Err("DDS is not D3DFMT_A16B16G16R16F (numeric FourCC 113)".to_owned());
    }
    let caps2 = read_u32(bytes, DDS_CAPS2_OFFSET)?;
    if caps2 & DDS_CUBEMAP_ALL_FACES != DDS_CUBEMAP_ALL_FACES {
        return Err(format!(
            "DDS does not contain all six cubemap faces (caps2=0x{caps2:08x})"
        ));
    }
    let height = read_u32(bytes, 12)?;
    let width = read_u32(bytes, 16)?;
    let mip_count = read_u32(bytes, 28)?.max(1);
    validate_dimensions(width, height, mip_count)?;
    let subresources = split_face_major(&bytes[DDS_HEADER_SIZE..], width, mip_count)?;
    Ok(EnvironmentMap {
        path: path.to_owned(),
        size: width,
        mip_count,
        hdr_scale: 1.0,
        subresources,
    })
}

fn parse_ecf_ddx(path: &str, bytes: &[u8]) -> Result<EnvironmentMap, String> {
    let texture = DdxTexture::from_bytes(bytes).map_err(|error| error.to_string())?;
    if texture.info.resource_type != ResourceType::CubeMap {
        return Err(format!(
            "DDX resource is {:?}, expected CubeMap",
            texture.info.resource_type
        ));
    }
    if texture.info.data_format != DataFormat::A16B16G16R16F {
        return Err(format!(
            "DDX format is {:?}, expected A16B16G16R16F",
            texture.info.data_format
        ));
    }
    validate_dimensions(
        texture.info.width,
        texture.info.height,
        texture.info.num_mip_levels,
    )?;
    let subresources = split_ecf_layout(
        &texture.data,
        texture.info.width,
        texture.info.num_mip_levels,
    )?;
    Ok(EnvironmentMap {
        path: path.to_owned(),
        size: texture.info.width,
        mip_count: texture.info.num_mip_levels,
        hdr_scale: texture.info.hdr_scale,
        subresources,
    })
}

fn validate_dimensions(width: u32, height: u32, mip_count: u32) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("cubemap dimensions must be nonzero".to_owned());
    }
    if width != height {
        return Err(format!("cubemap is not square: {width}x{height}"));
    }
    let maximum_mips = u32::BITS - width.leading_zeros();
    if mip_count == 0 || mip_count > maximum_mips {
        return Err(format!(
            "cubemap has {mip_count} mip levels; {width}x{height} permits at most {maximum_mips}"
        ));
    }
    Ok(())
}

fn split_face_major(data: &[u8], size: u32, mip_count: u32) -> Result<Vec<Vec<u8>>, String> {
    let mut cursor = 0;
    let mut result = Vec::with_capacity(
        CUBE_FACE_COUNT * usize::try_from(mip_count).map_err(|_| "mip count is too large")?,
    );
    for _face in 0..CUBE_FACE_COUNT {
        for mip in 0..mip_count {
            result.push(take_subresource(data, &mut cursor, size, mip)?.to_vec());
        }
    }
    validate_consumed(data, cursor)?;
    Ok(result)
}

fn split_ecf_layout(data: &[u8], size: u32, mip_count: u32) -> Result<Vec<Vec<u8>>, String> {
    let count = CUBE_FACE_COUNT
        .checked_mul(usize::try_from(mip_count).map_err(|_| "mip count is too large")?)
        .ok_or("cubemap subresource count overflow")?;
    let mut result = vec![Vec::new(); count];
    let mut cursor = 0;
    for face in 0..CUBE_FACE_COUNT {
        result[face * usize::try_from(mip_count).map_err(|_| "mip count is too large")?] =
            take_subresource(data, &mut cursor, size, 0)?.to_vec();
    }
    for face in 0..CUBE_FACE_COUNT {
        for mip in 1..mip_count {
            let index = face * usize::try_from(mip_count).map_err(|_| "mip count is too large")?
                + usize::try_from(mip).map_err(|_| "mip level is too large")?;
            result[index] = take_subresource(data, &mut cursor, size, mip)?.to_vec();
        }
    }
    validate_consumed(data, cursor)?;
    Ok(result)
}

fn take_subresource<'data>(
    data: &'data [u8],
    cursor: &mut usize,
    size: u32,
    mip: u32,
) -> Result<&'data [u8], String> {
    let extent = usize::try_from(mip_extent(size, mip)).map_err(|_| "mip extent is too large")?;
    let byte_len = extent
        .checked_mul(extent)
        .and_then(|pixels| pixels.checked_mul(RGBA16_FLOAT_BYTES_PER_PIXEL))
        .ok_or("cubemap mip byte count overflow")?;
    let end = cursor
        .checked_add(byte_len)
        .ok_or("cubemap payload offset overflow")?;
    let subresource = data.get(*cursor..end).ok_or_else(|| {
        format!(
            "cubemap payload is truncated at byte {cursor}: need {byte_len}, have {}",
            data.len().saturating_sub(*cursor)
        )
    })?;
    *cursor = end;
    Ok(subresource)
}

fn validate_consumed(data: &[u8], cursor: usize) -> Result<(), String> {
    if cursor != data.len() {
        return Err(format!(
            "cubemap payload has {} trailing bytes",
            data.len().saturating_sub(cursor)
        ));
    }
    Ok(())
}

fn mip_extent(size: u32, mip: u32) -> u32 {
    size.checked_shr(mip).unwrap_or(0).max(1)
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, String> {
    let end = offset.checked_add(4).ok_or("DDS field offset overflow")?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| format!("DDS field at byte {offset} is truncated"))?;
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| "DDS field is not four bytes")?,
    ))
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
    use super::{
        CUBE_FACE_COUNT, D3DFMT_A16B16G16R16F, DDS_CUBEMAP_ALL_FACES, DDS_PIXEL_FORMAT_SIZE,
        EnvironmentMap, RGBA16_FLOAT_BYTES_PER_PIXEL, canonical_environment_path,
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
        push_u32(&mut bytes, DDS_CUBEMAP_ALL_FACES);
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
        assert_eq!(cube.subresources[0], vec![0; 32]);
        assert_eq!(cube.subresources[1], vec![16; 8]);
        assert_eq!(cube.subresources[10], vec![5; 32]);
        assert_eq!(cube.subresources[11], vec![21; 8]);
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
