//! Coverage for every shipped UAX curve encoding used by the sampler.

use std::collections::BTreeSet;

use pipeline::uax::Reader as UaxReader;
use pipeline::uax::types::{Animation, CurveData, CurvePayload};

use super::{AnimationSample, sample_curve};

const IDENTITY_3: [f32; 3] = [0.0; 3];
const IDENTITY_4: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const IDENTITY_9: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

#[test]
fn animation_sample_uses_retail_local_clock_and_frame_timing() {
    let animation = Animation {
        name: None,
        duration: 2.0,
        time_step: 0.25,
        oversampling: 1.0,
        track_groups: Vec::new(),
        default_loop_count: 0,
        flags: 0,
    };
    let sample = AnimationSample::from_normalized(&animation, 0.375);
    assert_eq!(sample.local_clock.to_bits(), 0.75_f32.to_bits());
    assert_eq!(sample.frame_index, 3);
}

#[test]
fn keyframes_use_the_retail_frame_index_without_interpolation() {
    let curve = CurveData {
        format: 0,
        degree: 1,
        payload: CurvePayload::DaKeyframes32f {
            dimension: 3,
            controls: vec![0.0, 10.0, 20.0, 20.0, 30.0, 40.0],
        },
    };
    assert_eq!(sample(&curve, 3, &IDENTITY_3, 0.25, 0), [0.0, 10.0, 20.0]);
    assert_eq!(sample(&curve, 3, &IDENTITY_3, 0.25, 1), [20.0, 30.0, 40.0]);
}

#[test]
fn identity_curves_use_the_component_identity() {
    let curve = CurveData {
        format: 2,
        degree: 0,
        payload: CurvePayload::Identity { dimension: 3 },
    };
    assert_eq!(sample(&curve, 3, &[4.0, 5.0, 6.0], 0.0, 0), [4.0, 5.0, 6.0]);
}

#[test]
fn null_identity_curves_use_the_requested_component_identity() {
    let curve = CurveData {
        format: 0,
        degree: 0,
        payload: CurvePayload::Identity { dimension: 0 },
    };
    assert_eq!(sample(&curve, 3, &[4.0, 5.0, 6.0], 0.0, 0), [4.0, 5.0, 6.0]);
}

#[test]
fn arbitrary_u16_curve_decodes_scale_offsets_and_interpolates() {
    let curve = CurveData {
        format: 6,
        degree: 1,
        payload: CurvePayload::DaK16uC16u {
            one_over_knot_scale_trunc: 0x3f80,
            control_scale_offsets: vec![2.0, 3.0, 4.0, -1.0, -2.0, -3.0],
            knots_controls: vec![0, 10, 10, 20, 30, 30, 40, 50],
        },
    };
    assert_close(
        &sample(&curve, 3, &IDENTITY_3, 5.0, 0),
        &[39.0, 88.0, 157.0],
    );
}

#[test]
fn arbitrary_u8_curve_decodes_scale_offsets_and_interpolates() {
    let curve = CurveData {
        format: 7,
        degree: 1,
        payload: CurvePayload::DaK8uC8u {
            one_over_knot_scale_trunc: 0x3f80,
            control_scale_offsets: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            knots_controls: vec![0, 10, 1, 2, 3, 5, 6, 7],
        },
    };
    assert_close(&sample(&curve, 3, &IDENTITY_3, 5.0, 0), &[7.0, 13.0, 21.0]);
}

#[test]
fn quantized_quaternion_formats_reconstruct_the_missing_component() {
    let curves = [
        CurveData {
            format: 8,
            degree: 1,
            payload: CurvePayload::D4nK16uC15u {
                scale_offset_table_entries: 0,
                one_over_knot_scale: 1.0,
                knots_controls: vec![0, 10, 0x4000, 0xc000, 0xc000, 0x4000, 0xc000, 0xc000],
            },
        },
        CurveData {
            format: 9,
            degree: 1,
            payload: CurvePayload::D4nK8uC7u {
                scale_offset_table_entries: 0,
                one_over_knot_scale: 1.0,
                knots_controls: vec![0, 10, 0x40, 0xc0, 0xc0, 0x40, 0xc0, 0xc0],
            },
        },
    ];
    for curve in curves {
        let actual = sample_normalized(&curve, 5.0);
        assert!(actual[..3].iter().all(|component| component.abs() < 0.006));
        assert!(actual[3] > 0.999);
        assert_close(
            &[actual.iter().map(|value| value * value).sum::<f32>()],
            &[1.0],
        );
    }
}

#[test]
fn fixed_vec3_quantized_formats_use_controls_after_the_knot_block() {
    let curves = [
        CurveData {
            format: 10,
            degree: 1,
            payload: CurvePayload::D3K16uC16u {
                one_over_knot_scale_trunc: 0x3f80,
                control_scales: [1.0, 2.0, 3.0],
                control_offsets: [4.0, 5.0, 6.0],
                knots_controls: vec![0, 10, 1, 2, 3, 5, 6, 7],
            },
        },
        CurveData {
            format: 11,
            degree: 1,
            payload: CurvePayload::D3K8uC8u {
                one_over_knot_scale_trunc: 0x3f80,
                control_scales: [1.0, 2.0, 3.0],
                control_offsets: [4.0, 5.0, 6.0],
                knots_controls: vec![0, 10, 1, 2, 3, 5, 6, 7],
            },
        },
    ];
    for curve in curves {
        assert_close(&sample(&curve, 3, &IDENTITY_3, 5.0, 0), &[7.0, 13.0, 21.0]);
    }
}

#[test]
fn d9_i1_curve_expands_uniform_scale_to_a_matrix() {
    let curve = CurveData {
        format: 12,
        degree: 1,
        payload: CurvePayload::D9I1K16uC16u {
            one_over_knot_scale_trunc: 0x3f80,
            control_scale: 2.0,
            control_offset: 1.0,
            knots_controls: vec![0, 10, 2, 4],
        },
    };
    assert_close(
        &sample(&curve, 9, &IDENTITY_9, 5.0, 0),
        &[7.0, 0.0, 0.0, 0.0, 7.0, 0.0, 0.0, 0.0, 7.0],
    );
}

#[test]
fn d9_i3_curve_expands_three_scales_to_the_diagonal() {
    let curve = CurveData {
        format: 15,
        degree: 1,
        payload: CurvePayload::D9I3K8uC8u {
            one_over_knot_scale_trunc: 0x3f80,
            control_scales: [2.0; 3],
            control_offsets: [1.0; 3],
            knots_controls: vec![0, 10, 1, 2, 3, 4, 5, 6],
        },
    };
    assert_close(
        &sample(&curve, 9, &IDENTITY_9, 5.0, 0),
        &[6.0, 0.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 10.0],
    );
}

#[test]
fn remaining_compact_matrix_formats_expand_to_the_diagonal() {
    let diagonal = CurveData {
        format: 13,
        degree: 1,
        payload: CurvePayload::D9I3K16uC16u {
            one_over_knot_scale_trunc: 0x3f80,
            control_scales: [2.0; 3],
            control_offsets: [1.0; 3],
            knots_controls: vec![0, 10, 1, 2, 3, 4, 5, 6],
        },
    };
    assert_close(
        &sample(&diagonal, 9, &IDENTITY_9, 5.0, 0),
        &[6.0, 0.0, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 10.0],
    );

    let uniform = CurveData {
        format: 14,
        degree: 1,
        payload: CurvePayload::D9I1K8uC8u {
            one_over_knot_scale_trunc: 0x3f80,
            control_scale: 2.0,
            control_offset: 1.0,
            knots_controls: vec![0, 10, 2, 4],
        },
    };
    assert_close(
        &sample(&uniform, 9, &IDENTITY_9, 5.0, 0),
        &[7.0, 0.0, 0.0, 0.0, 7.0, 0.0, 0.0, 0.0, 7.0],
    );
}

#[test]
fn scalar_parameter_vec3_formats_expand_all_components() {
    let curves = [
        CurveData {
            format: 16,
            degree: 1,
            payload: CurvePayload::D3I1K32fC32f {
                padding: 0,
                control_scales: [2.0, 3.0, 4.0],
                control_offsets: [-1.0, -2.0, -3.0],
                knots_controls: vec![0.0, 10.0, 2.0, 4.0],
            },
        },
        CurveData {
            format: 17,
            degree: 1,
            payload: CurvePayload::D3I1K16uC16u {
                one_over_knot_scale_trunc: 0x3f80,
                control_scales: [2.0, 3.0, 4.0],
                control_offsets: [-1.0, -2.0, -3.0],
                knots_controls: vec![0, 10, 2, 4],
            },
        },
        CurveData {
            format: 18,
            degree: 1,
            payload: CurvePayload::D3I1K8uC8u {
                one_over_knot_scale_trunc: 0x3f80,
                control_scales: [2.0, 3.0, 4.0],
                control_offsets: [-1.0, -2.0, -3.0],
                knots_controls: vec![0, 10, 2, 4],
            },
        },
    ];
    for curve in curves {
        assert_close(&sample(&curve, 3, &IDENTITY_3, 5.0, 0), &[5.0, 7.0, 9.0]);
    }
}

#[test]
fn cubic_curve_uses_granny_bspline_coefficients() {
    let curve = CurveData {
        format: 1,
        degree: 3,
        payload: CurvePayload::DaK32fC32f {
            padding: 0,
            knots: vec![0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0],
            controls: vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0],
        },
    };
    assert_close(&sample(&curve, 1, &[0.0], 0.5, 0), &[11.875]);
}

#[test]
fn raw_single_knot_curve_uses_its_constant_control() {
    let curve = CurveData {
        format: 1,
        degree: 3,
        payload: CurvePayload::DaK32fC32f {
            padding: 0,
            knots: vec![0.0],
            controls: vec![4.0, 5.0, 6.0],
        },
    };
    assert_eq!(sample(&curve, 3, &IDENTITY_3, 0.5, 0), [4.0, 5.0, 6.0]);
}

#[test]
fn low_end_spline_window_replicates_the_first_control() {
    let curve = CurveData {
        format: 1,
        degree: 2,
        payload: CurvePayload::DaK32fC32f {
            padding: 0,
            knots: vec![0.0, 1.0, 2.0],
            controls: vec![10.0, 20.0, 30.0],
        },
    };
    assert_close(&sample(&curve, 1, &[0.0], 0.5, 0), &[11.25]);
}

#[test]
fn quaternion_spline_preserves_hemisphere_continuity() {
    let curve = CurveData {
        format: 1,
        degree: 2,
        payload: CurvePayload::DaK32fC32f {
            padding: 0,
            knots: vec![0.0, 1.0, 2.0],
            controls: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0],
        },
    };
    let actual = sample_curve(
        &curve,
        4,
        &[0.0, 0.0, 0.0, 1.0],
        AnimationSample::new(0.5, 0),
        true,
    )
    .unwrap();
    assert_close(&actual, &[0.0, 0.0, 0.0, 1.0]);
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn samples_every_installed_hw1_transform_curve() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let (_, mut source) = pipeline::hw1::World::load(&game_dir).expect("load game archives");
    let paths = source
        .files_per_archive()
        .into_iter()
        .flat_map(|(_, paths)| paths.into_iter())
        .filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("uax"))
        })
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut sampled = 0_usize;
    let mut animations = 0_usize;
    let mut formats = BTreeSet::new();
    let mut failures = Vec::new();
    for path in paths {
        let Some(bytes) = source.resolve_with_fallback(&path, &[]) else {
            failures.push(format!("{path}: missing from its source archive"));
            continue;
        };
        let animation = match UaxReader::read(&bytes) {
            Ok(animation) => animation,
            Err(error) => {
                failures.push(format!("{path}: {error}"));
                continue;
            }
        };
        animations += 1;
        for position in [0.0, 0.5, 1.0] {
            let sample = AnimationSample::from_normalized(&animation, position);
            for track in animation
                .track_groups
                .iter()
                .flat_map(|group| &group.transform_tracks)
            {
                let name = track.name.as_deref().unwrap_or("<unnamed>");
                let components = [
                    ("position", &track.position, &IDENTITY_3[..], false),
                    ("orientation", &track.orientation, &IDENTITY_4[..], true),
                    ("scale/shear", &track.scale_shear, &IDENTITY_9[..], false),
                ];
                for (component, curve, identity, normalized) in components {
                    formats.insert(curve.format);
                    sampled += 1;
                    if let Err(error) =
                        sample_curve(curve, identity.len(), identity, sample, normalized)
                    {
                        failures.push(format!(
                            "{path} :: {name} :: {component} @ {position}: {error}"
                        ));
                    }
                }
            }
        }
    }
    assert!(sampled > 0, "no installed transform curves were found");
    println!(
        "sampled {} transform curves from {animations} UAX files at three times; formats={formats:?}",
        sampled / 3
    );
    assert!(
        failures.is_empty(),
        "{} sampling failures:\n{}",
        failures.len(),
        failures
            .iter()
            .take(64)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn sample(
    curve: &CurveData,
    dimension: usize,
    identity: &[f32],
    local_clock: f32,
    frame_index: usize,
) -> Vec<f32> {
    sample_curve(
        curve,
        dimension,
        identity,
        AnimationSample::new(local_clock, frame_index),
        false,
    )
    .unwrap()
}

fn sample_normalized(curve: &CurveData, local_clock: f32) -> Vec<f32> {
    sample_curve(
        curve,
        4,
        &[0.0, 0.0, 0.0, 1.0],
        AnimationSample::new(local_clock, 0),
        true,
    )
    .unwrap()
}

fn assert_close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 0.000_1,
            "{actual} != {expected}"
        );
    }
}
