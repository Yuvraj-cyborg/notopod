//! Where each note goes in the picture.
//!
//! A Fruchterman–Reingold force simulation: notes push each other apart,
//! links pull their ends together, a little gravity keeps islands from
//! drifting off. It runs in a space where one unit is a cell's width and
//! half a cell's height, so distances mean the same thing in both
//! directions on a terminal grid. Afterwards a pass separates any boxes
//! that still overlap. No randomness: the starting positions lie on a
//! spiral, so the same graph always gets the same picture.

use canvas::Rect;

use crate::Graph;

/// Rows of an ellipse: enough for one line of text with a curve around it.
const NODE_ROWS: i32 = 3;

/// Cells of margin kept between boxes.
const GAP: i32 = 1;

/// The rects of every node, in node order, inside `width` columns.
pub(crate) fn place(graph: &Graph, width: usize) -> Vec<Rect> {
    let count = graph.nodes.len();
    // A drawing's extent has one cell of margin past its last column.
    let width = (width.max(20) - 1) as i32;
    let sizes: Vec<(i32, i32)> = graph
        .nodes
        .iter()
        .map(|node| {
            let cols = (node.label().chars().count() as i32 + 4).clamp(8, width);
            (cols, NODE_ROWS)
        })
        .collect();

    // Room: grow with the square root of the count, and never less than
    // the boxes need with air around them.
    let boxes_area: i32 = sizes
        .iter()
        .map(|(cols, rows)| (cols + GAP) * (rows + GAP))
        .sum();
    let rows = ((count as f32).sqrt() * 4.0 + 4.0)
        .max((boxes_area as f32 * 2.2 / width as f32).ceil())
        .clamp(8.0, 400.0) as i32;

    let area = (width as f32, (rows * 2) as f32);
    let mut pos = spiral(count, area);
    simulate(graph, &mut pos, &sizes, area);

    let mut rects: Vec<Rect> = pos
        .iter()
        .zip(&sizes)
        .map(|((px, py), &(cols, node_rows))| {
            let left = (px - cols as f32 / 2.0).round() as i32;
            let top = (py / 2.0 - node_rows as f32 / 2.0).round() as i32;
            Rect::new(left.clamp(0, width - cols), top.max(0), cols, node_rows)
        })
        .collect();
    separate(&mut rects, width);
    rects
}

/// Starting positions on a spiral around the middle of `area`.
fn spiral(count: usize, area: (f32, f32)) -> Vec<(f32, f32)> {
    let (cx, cy) = (area.0 / 2.0, area.1 / 2.0);
    let radius = area.0.min(area.1) / 2.0 * 0.8;
    (0..count)
        .map(|i| {
            let frac = (i as f32 + 0.5) / count as f32;
            let reach = radius * frac.sqrt();
            let angle = i as f32 * 2.399_963;
            (cx + reach * angle.cos(), cy + reach * angle.sin())
        })
        .collect()
}

fn simulate(graph: &Graph, pos: &mut [(f32, f32)], sizes: &[(i32, i32)], area: (f32, f32)) {
    let count = pos.len();
    let (area_w, area_h) = area;
    if count < 2 {
        if count == 1 {
            pos[0] = (area_w / 2.0, area_h / 2.0);
        }
        return;
    }
    let iterations = match count {
        0..=100 => 200,
        101..=400 => 100,
        _ => 40,
    };
    let k = 0.9 * (area_w * area_h / count as f32).sqrt();
    let start_temp = area_w / 6.0;
    let mut push = vec![(0.0f32, 0.0f32); count];

    for step in 0..iterations {
        let temp = start_temp * (1.0 - step as f32 / iterations as f32) + 0.5;
        push.fill((0.0, 0.0));
        // Repulsion between every pair, stronger for wide boxes.
        for i in 0..count {
            for j in (i + 1)..count {
                let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                let dist = (dx * dx + dy * dy).sqrt().max(0.05);
                let reach = k + (sizes[i].0 + sizes[j].0) as f32 / 4.0;
                let force = reach * reach / dist;
                let (fx, fy) = (dx / dist * force, dy / dist * force);
                push[i].0 += fx;
                push[i].1 += fy;
                push[j].0 -= fx;
                push[j].1 -= fy;
            }
        }
        // Attraction along edges.
        for &(a, b) in &graph.edges {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let dist = (dx * dx + dy * dy).sqrt().max(0.05);
            let force = dist * dist / k;
            let (fx, fy) = (dx / dist * force, dy / dist * force);
            push[a].0 -= fx;
            push[a].1 -= fy;
            push[b].0 += fx;
            push[b].1 += fy;
        }
        // Gravity, so notes with no links stay in the picture.
        for (p, at) in push.iter_mut().zip(pos.iter()) {
            p.0 -= (at.0 - area_w / 2.0) * 0.03;
            p.1 -= (at.1 - area_h / 2.0) * 0.03;
        }
        for ((at, &(dx, dy)), &(cols, node_rows)) in pos.iter_mut().zip(&push).zip(sizes) {
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.0 {
                let scale = len.min(temp) / len;
                at.0 += dx * scale;
                at.1 += dy * scale;
            }
            let (half_w, half_h) = (cols as f32 / 2.0, node_rows as f32);
            at.0 = at.0.clamp(half_w, (area_w - half_w).max(half_w));
            at.1 = at.1.clamp(half_h, (area_h - half_h).max(half_h));
        }
    }
}

/// Pushes overlapping boxes apart, a little at a time, until none overlap
/// or patience runs out. Rows are twice as costly to move across as
/// columns, so boxes slide sideways when they can.
fn separate(rects: &mut [Rect], width: i32) {
    let rounds = if rects.len() <= 200 { 200 } else { 60 };
    for _ in 0..rounds {
        let mut moved = false;
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let (a, b) = (rects[i], rects[j]);
                let over_x = (a.right() + GAP).min(b.right() + GAP) - a.x.max(b.x);
                let over_y = (a.bottom() + GAP).min(b.bottom() + GAP) - a.y.max(b.y);
                if over_x <= 0 || over_y <= 0 {
                    continue;
                }
                moved = true;
                let (l, r) = if a.x + a.w / 2 <= b.x + b.w / 2 {
                    (i, j)
                } else {
                    (j, i)
                };
                let down = if a.y + a.h / 2 <= b.y + b.h / 2 { j } else { i };
                let shift_x = (over_x + 1) / 2;
                let sideways = over_x <= over_y * 2
                    && rects[l].x - shift_x >= 0
                    && rects[r].x + rects[r].w + shift_x <= width;
                if sideways {
                    rects[l].x -= shift_x;
                    rects[r].x += shift_x;
                } else {
                    // No room beside each other: the lower one moves down.
                    // Boxes never move up, so this cannot go round in circles.
                    rects[down].y += over_y;
                }
            }
        }
        if !moved {
            return;
        }
    }
    // Whatever still overlaps is stacked below what it hits, a row at a
    // time; y only grows, so this always ends.
    for i in 0..rects.len() {
        while let Some(j) = (0..i).find(|&j| touching(&rects[i], &rects[j])) {
            rects[i].y = rects[j].bottom() + GAP + 1;
        }
    }
}

fn touching(a: &Rect, b: &Rect) -> bool {
    a.x <= b.right() + GAP
        && b.x <= a.right() + GAP
        && a.y <= b.bottom() + GAP
        && b.y <= a.bottom() + GAP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;
    use std::path::PathBuf;

    fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
        Graph {
            root: PathBuf::new(),
            nodes: (0..n)
                .map(|i| Node {
                    path: PathBuf::new(),
                    name: format!("note {i}"),
                    title: None,
                    out: 0,
                    inbound: 0,
                })
                .collect(),
            edges: edges.to_vec(),
            truncated: false,
        }
    }

    fn overlaps(a: &Rect, b: &Rect) -> bool {
        a.x < b.right() + GAP
            && b.x < a.right() + GAP
            && a.y < b.bottom() + GAP
            && b.y < a.bottom() + GAP
    }

    #[test]
    fn boxes_fit_the_width_and_do_not_overlap() {
        for (n, width) in [(1, 40), (2, 20), (7, 60), (30, 80), (60, 100)] {
            let edges: Vec<(usize, usize)> = (1..n).map(|i| (i - 1, i)).collect();
            let rects = place(&graph(n, &edges), width);
            assert_eq!(rects.len(), n);
            for (i, a) in rects.iter().enumerate() {
                assert!(
                    a.x >= 0 && a.right() <= width as i32,
                    "{n} nodes at {width}: {a:?}"
                );
                assert!(a.y >= 0);
                for b in &rects[i + 1..] {
                    assert!(!overlaps(a, b), "{n} nodes at {width}: {a:?} vs {b:?}");
                }
            }
        }
    }

    #[test]
    fn linked_notes_end_up_closer_than_strangers() {
        // Two pairs, each linked inside, nothing between the pairs.
        let rects = place(&graph(4, &[(0, 1), (2, 3)]), 80);
        let d = |a: &Rect, b: &Rect| {
            let dx = (a.x + a.w / 2 - b.x - b.w / 2) as f32;
            let dy = ((a.y - b.y) * 2) as f32;
            (dx * dx + dy * dy).sqrt()
        };
        let inside = d(&rects[0], &rects[1]).min(d(&rects[2], &rects[3]));
        let between = d(&rects[0], &rects[2])
            .min(d(&rects[0], &rects[3]))
            .min(d(&rects[1], &rects[2]))
            .min(d(&rects[1], &rects[3]));
        assert!(inside < between, "inside {inside} vs between {between}");
    }

    #[test]
    fn the_layout_is_deterministic() {
        let g = graph(12, &[(0, 1), (1, 2), (2, 0), (5, 6)]);
        assert_eq!(place(&g, 70), place(&g, 70));
    }
}
