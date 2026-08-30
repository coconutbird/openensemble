use super::*;

#[test]
fn xsd_height_queries_reproduce_retail_blocked_grid_and_clamping() {
    let header = header(XsdEndian::Little, 9, 16, 2.0);
    let mut heights = vec![0_u8; 16 * 16 * 2];
    write_height(&mut heights, XsdEndian::Little, 16, 3, 5, 42.5);
    write_height(&mut heights, XsdEndian::Little, 16, 8, 8, -7.25);
    let terrain = TerrainSimulation::from_chunks(&header, &heights).unwrap();

    let interpolated = terrain
        .height(Vec3::new(6.9, 999.0, 10.1), false)
        .expect("interpolated terrain height");
    assert!((interpolated - 22.206_25).abs() < 0.000_1);
    assert_eq!(terrain.height(Vec3::new(99.0, 0.0, 99.0), false), None);
    assert_eq!(
        terrain.height(Vec3::new(99.0, 0.0, 99.0), true),
        Some(-7.25)
    );
}

#[test]
fn xsd_height_parser_accepts_retail_big_endian_chunks() {
    let header = header(XsdEndian::Big, 8, 8, 1.0);
    let mut heights = vec![0_u8; 8 * 8 * 2];
    write_height(&mut heights, XsdEndian::Big, 8, 7, 2, 6.5);
    let terrain = TerrainSimulation::from_chunks(&header, &heights).unwrap();

    assert_eq!(terrain.height(Vec3::new(7.0, 0.0, 2.0), false), Some(6.5));
}

#[test]
fn logical_fingerprint_is_independent_of_xsd_byte_order() {
    let little_header = header(XsdEndian::Little, 8, 8, 2.0);
    let big_header = header(XsdEndian::Big, 8, 8, 2.0);
    let mut little_heights = vec![0_u8; 8 * 8 * 2];
    let mut big_heights = vec![0_u8; 8 * 8 * 2];
    write_height(&mut little_heights, XsdEndian::Little, 8, 3, 4, 11.5);
    write_height(&mut big_heights, XsdEndian::Big, 8, 3, 4, 11.5);

    let little = TerrainSimulation::from_chunks(&little_header, &little_heights).unwrap();
    let big = TerrainSimulation::from_chunks(&big_header, &big_heights).unwrap();

    assert_eq!(little.fingerprint, big.fingerprint);
}

#[test]
fn xsd_surface_queries_use_retail_row_major_clamping() {
    let mut little_header = header(XsdEndian::Little, 8, 8, 1.0);
    write_i32(&mut little_header[4..8], XsdEndian::Little, 4);
    write_f32(&mut little_header[8..12], XsdEndian::Little, 2.0);
    let heights = vec![0_u8; 8 * 8 * 2];
    let tile_types = (0_u8..16).collect::<Vec<_>>();
    let terrain =
        TerrainSimulation::from_chunks_with_tile_types(&little_header, &heights, Some(&tile_types))
            .unwrap();

    assert_eq!(terrain.surface_type(Vec3::new(5.9, 20.0, 3.9)), Some(6));
    assert_eq!(terrain.surface_type(Vec3::new(-1.9, 0.0, -1.9)), Some(0));
    assert_eq!(terrain.surface_type(Vec3::new(500.0, 0.0, 500.0)), Some(15));
    assert_eq!(terrain.surface_type(Vec3::splat(f32::NAN)), None);
}

#[test]
fn xsd_surface_query_is_absent_when_synthetic_chunk_is_omitted() {
    let header = header(XsdEndian::Little, 8, 8, 1.0);
    let heights = vec![0_u8; 8 * 8 * 2];
    let terrain = TerrainSimulation::from_chunks(&header, &heights).unwrap();

    assert_eq!(terrain.surface_type(Vec3::ZERO), None);
}

#[test]
fn projectile_segment_finds_the_retail_triangle_surface() {
    let header = header(XsdEndian::Little, 8, 8, 2.0);
    let mut heights = vec![0_u8; 8 * 8 * 2];
    write_height(&mut heights, XsdEndian::Little, 8, 1, 1, 2.0);
    let terrain = TerrainSimulation::from_chunks(&header, &heights).unwrap();

    let impact = terrain
        .projectile_segment_intersection(Vec3::new(2.0, 4.0, 2.0), Vec3::new(2.0, -1.0, 2.0))
        .expect("vertical segment should strike the raised vertex");
    assert!((impact.y - 2.0).abs() < 0.000_1);
}

#[test]
fn projectile_fast_check_matches_retail_clearance_threshold() {
    let header = header(XsdEndian::Little, 8, 8, 1.0);
    let heights = vec![0_u8; 8 * 8 * 2];
    let terrain = TerrainSimulation::from_chunks(&header, &heights).unwrap();

    assert!(
        terrain
            .projectile_segment_intersection(Vec3::new(1.0, 0.5, 1.0), Vec3::new(2.0, 0.5, 1.0),)
            .is_none()
    );
    assert_eq!(
        terrain
            .projectile_segment_intersection(Vec3::new(1.0, 0.2, 1.0), Vec3::new(2.0, 0.1, 1.0),),
        Some(Vec3::new(2.0, 0.0, 1.0))
    );
}

fn header(endian: XsdEndian, height_axis: i32, cache_axis: i32, scale: f32) -> Vec<u8> {
    let mut bytes = vec![0_u8; XSD_HEADER_SIZE];
    write_i32(&mut bytes[0..4], endian, XSD_VERSION);
    write_i32(&mut bytes[4..8], endian, 8);
    write_f32(&mut bytes[8..12], endian, 1.0);
    write_f32(&mut bytes[12..16], endian, 1.0);
    write_i32(&mut bytes[16..20], endian, height_axis);
    write_i32(&mut bytes[20..24], endian, cache_axis);
    write_f32(&mut bytes[24..28], endian, scale);
    write_i32(&mut bytes[28..32], endian, 1);
    bytes
}

fn write_height(
    bytes: &mut [u8],
    endian: XsdEndian,
    cache_axis: usize,
    x: usize,
    z: usize,
    height: f32,
) {
    let blocks_per_axis = cache_axis / HEIGHT_BLOCK_AXIS;
    let index = (z / HEIGHT_BLOCK_AXIS) * blocks_per_axis * HEIGHT_BLOCK_SIZE
        + (x / HEIGHT_BLOCK_AXIS) * HEIGHT_BLOCK_SIZE
        + (z % HEIGHT_BLOCK_AXIS) * HEIGHT_BLOCK_AXIS
        + x % HEIGHT_BLOCK_AXIS;
    let sample = &mut bytes[index * 2..index * 2 + 2];
    match endian {
        XsdEndian::Big => BigEndian::write_u16(sample, f16::from_f32(height).to_bits()),
        XsdEndian::Little => LittleEndian::write_u16(sample, f16::from_f32(height).to_bits()),
    }
}

fn write_i32(bytes: &mut [u8], endian: XsdEndian, value: i32) {
    match endian {
        XsdEndian::Big => BigEndian::write_i32(bytes, value),
        XsdEndian::Little => LittleEndian::write_i32(bytes, value),
    }
}

fn write_f32(bytes: &mut [u8], endian: XsdEndian, value: f32) {
    match endian {
        XsdEndian::Big => BigEndian::write_f32(bytes, value),
        XsdEndian::Little => LittleEndian::write_f32(bytes, value),
    }
}
