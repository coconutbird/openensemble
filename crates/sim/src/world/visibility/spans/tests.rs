use super::*;
use std::collections::BTreeSet;

fn tiles(radius: i32) -> Vec<(i32, i32)> {
    circle(radius)
        .iter()
        .flat_map(|span| (span.start_z..span.start_z + span.count).map(move |z| (span.x, z)))
        .collect()
}

#[test]
fn small_circles_match_the_retail_midpoint_spans() {
    assert_eq!(
        circle(1),
        &[TileSpan {
            x: 0,
            start_z: -1,
            count: 2
        }]
    );
    assert_eq!(
        circle(2),
        &[
            TileSpan {
                x: -1,
                start_z: -1,
                count: 2
            },
            TileSpan {
                x: 0,
                start_z: -2,
                count: 4
            },
            TileSpan {
                x: 1,
                start_z: -1,
                count: 2
            },
        ]
    );
}

#[test]
fn radius_is_clamped_to_the_retail_table() {
    assert_eq!(circle(0), circle(1));
    assert_eq!(circle(-5), circle(1));
    assert_eq!(circle(500), circle(128));
}

#[test]
fn every_circle_has_one_sorted_span_per_column_and_no_repeated_tile() {
    for radius in MIN_RADIUS..=MAX_RADIUS {
        let spans = circle(radius);
        assert!(spans.windows(2).all(|pair| pair[0].x < pair[1].x));
        let tiles = tiles(radius);
        let unique = tiles.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), tiles.len(), "radius {radius}");
        let column = spans
            .iter()
            .find(|span| span.x == 0)
            .expect("center column");
        assert_eq!((column.start_z, column.count), (-radius, 2 * radius));
    }
}
