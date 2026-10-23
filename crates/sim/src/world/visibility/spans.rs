//! Retail tile span circles of the visibility map.
//!
//! `BVisibleMap_setup` (`0x1405BDE80`) builds one span list for each radius from 1 to 128
//! with `BTileSpanList_buildFromRadius` (`0x1405BDC50`) and `BTileSpanList_addSpan`
//! (`0x1405BDA20`). A span is one column `x` with the rows `start_z..start_z + count`. The
//! midpoint circle gives each column `-z..z`, not `-z..=z`, so a circle is one tile larger on
//! the negative side. The retail partial update (`BVisibleMap_calcSpanDiff`) is an exact
//! difference of two circles, so this port moves a circle by an unexplore of the old circle and
//! an explore of the new circle. The tile counts are the same.

use std::sync::LazyLock;

/// Smallest radius of the retail span tables (`MIN_RADIUS`).
pub(super) const MIN_RADIUS: i32 = 1;
/// Largest radius of the retail fixed span tables (`MAX_FIXED_RADIUS`).
pub(super) const MAX_RADIUS: i32 = 128;

/// One column of tiles relative to the circle center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TileSpan {
    pub(super) x: i32,
    pub(super) start_z: i32,
    pub(super) count: i32,
}

/// Retail span lists for every radius of `MIN_RADIUS..=MAX_RADIUS`.
static CIRCLES: LazyLock<Vec<Vec<TileSpan>>> =
    LazyLock::new(|| (MIN_RADIUS..=MAX_RADIUS).map(build_circle).collect());

/// Return the retail span list of a radius, after the clamp to `1..=128`.
pub(super) fn circle(radius: i32) -> &'static [TileSpan] {
    let index = radius.clamp(MIN_RADIUS, MAX_RADIUS) - MIN_RADIUS;
    usize::try_from(index)
        .ok()
        .and_then(|index| CIRCLES.get(index))
        .map_or(&[], Vec::as_slice)
}

/// Port of `BTileSpanList::buildFromRadius`.
fn build_circle(radius: i32) -> Vec<TileSpan> {
    let mut spans = Vec::new();
    let mut x = 0;
    let mut z = radius;
    // C integer division truncates toward zero, like Rust.
    let mut decision = (5 - radius * 4) / 4;
    add_span(&mut spans, 0, -z, 2 * z);
    while x < z {
        x += 1;
        if decision < 0 {
            add_span(&mut spans, -x, -z, 2 * z);
            add_span(&mut spans, x, -z, 2 * z);
            decision += 2 * x + 1;
        } else {
            let start = -x + 1;
            let count = x - 1 - start;
            add_span(&mut spans, z, start, count);
            add_span(&mut spans, -z, start, count);
            z -= 1;
            if x < z {
                add_span(&mut spans, x, -z, 2 * z);
                add_span(&mut spans, -x, -z, 2 * z);
            }
            decision += 2 * (x - z) + 1;
        }
    }
    add_span(&mut spans, z, -x, 2 * x);
    add_span(&mut spans, -z, -x, 2 * x);
    spans
}

/// Port of `BTileSpanList::addSpan` with `merge = true`.
///
/// The list stays sorted by column. A span in a known column merges into an overlapped span of
/// that column; an identical or contained span is dropped.
fn add_span(spans: &mut Vec<TileSpan>, x: i32, start_z: i32, count: i32) {
    if count <= 0 {
        return;
    }
    let span = TileSpan { x, start_z, count };
    let Some(mut insert_at) = spans.iter().position(|current| current.x >= x) else {
        spans.push(span);
        return;
    };
    if spans[insert_at].x == x {
        let end = start_z + count;
        while let Some(current) = spans.get_mut(insert_at) {
            if current.x != x || end < current.start_z {
                break;
            }
            if *current == span {
                return;
            }
            let current_end = current.start_z + current.count;
            if start_z > current.start_z && start_z <= current_end {
                if end > current_end {
                    current.count = end - current.start_z;
                }
                return;
            }
            if start_z < current.start_z && end > current.start_z {
                current.count = current_end - start_z;
                current.start_z = start_z;
                return;
            }
            insert_at += 1;
        }
    }
    spans.insert(insert_at, span);
}

#[cfg(test)]
mod tests;
