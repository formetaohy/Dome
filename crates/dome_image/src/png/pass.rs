use crate::png::header::Header;

pub(crate) struct Pass {
    pub(crate) x0: u32,
    pub(crate) y0: u32,
    pub(crate) dx: u32,
    pub(crate) dy: u32,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) row_bytes: usize,
}

pub(crate) struct Layout {
    pub(crate) passes: Vec<Pass>,
    pub(crate) bpp: usize,
}

const ADAM7: [(u32, u32, u32, u32); 7] = [
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
];

impl Layout {
    pub(crate) fn raw_bytes(&self) -> usize {
        self.passes
            .iter()
            .map(|pass| pass.height * (pass.row_bytes + 1))
            .sum()
    }
}

pub(crate) fn layout(header: &Header) -> Layout {
    let channels = header.color.channels();
    let bpp = channels * usize::from(header.depth) / 8;
    let bpp = bpp.max(1);
    let pass = |x0, y0, dx, dy| {
        let width = extent(header.width, x0, dx);
        let height = extent(header.height, y0, dy);
        let row_bytes = (width * channels * usize::from(header.depth)).div_ceil(8);
        Pass {
            x0,
            y0,
            dx,
            dy,
            width,
            height,
            row_bytes,
        }
    };
    let passes = if header.interlaced {
        ADAM7
            .iter()
            .map(|(x0, y0, dx, dy)| pass(*x0, *y0, *dx, *dy))
            .filter(|pass| pass.width > 0 && pass.height > 0)
            .collect()
    } else {
        vec![pass(0, 0, 1, 1)]
    };
    Layout { passes, bpp }
}

fn extent(size: u32, start: u32, step: u32) -> usize {
    if size <= start {
        return 0;
    }
    ((size - start) as usize).div_ceil(step as usize)
}

pub(crate) fn unfilter<'a>(
    raw: &'a mut [u8],
    layout: &Layout,
    what: &str,
) -> Result<Vec<&'a [u8]>, String> {
    let mut blocks = Vec::with_capacity(layout.passes.len());
    let mut rest = raw;
    for pass in &layout.passes {
        let span = pass.height * (pass.row_bytes + 1);
        if span > rest.len() {
            return Err(format!(
                "{what} holds {} bytes where its scanlines span {span}",
                rest.len(),
            ));
        }
        let (block, tail) = rest.split_at_mut(span);
        blocks.push(block);
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(format!(
            "{what} holds {} bytes beyond its scanlines",
            rest.len(),
        ));
    }
    for (pass, block) in layout.passes.iter().zip(blocks.iter_mut()) {
        for row in 0..pass.height {
            let start = row * (pass.row_bytes + 1);
            let filter = block[start];
            let (before, after) = block.split_at_mut(start + 1);
            let line = &mut after[..pass.row_bytes];
            let previous = if row == 0 {
                None
            } else {
                Some(&before[start - pass.row_bytes..start])
            };
            unfilter_row(line, previous, filter, layout.bpp, what)?;
        }
    }
    Ok(blocks.into_iter().map(|block| &*block).collect())
}

fn unfilter_row(
    row: &mut [u8],
    previous: Option<&[u8]>,
    filter: u8,
    bpp: usize,
    what: &str,
) -> Result<(), String> {
    match filter {
        0 => {}
        1 => {
            for at in bpp..row.len() {
                row[at] = row[at].wrapping_add(row[at - bpp]);
            }
        }
        2 => {
            if let Some(previous) = previous {
                for at in 0..row.len() {
                    row[at] = row[at].wrapping_add(previous[at]);
                }
            }
        }
        3 => {
            for at in 0..row.len() {
                let left = if at >= bpp { row[at - bpp] } else { 0 };
                let above = previous.map_or(0, |previous| previous[at]);
                row[at] = row[at].wrapping_add(((u16::from(left) + u16::from(above)) / 2) as u8);
            }
        }
        4 => {
            for at in 0..row.len() {
                let left = if at >= bpp { row[at - bpp] } else { 0 };
                let above = previous.map_or(0, |previous| previous[at]);
                let corner = if at >= bpp {
                    previous.map_or(0, |previous| previous[at - bpp])
                } else {
                    0
                };
                row[at] = row[at].wrapping_add(paeth(left, above, corner));
            }
        }
        other => return Err(format!("{what} holds filter {other}")),
    }
    Ok(())
}

fn paeth(left: u8, above: u8, corner: u8) -> u8 {
    let estimate = i32::from(left) + i32::from(above) - i32::from(corner);
    let left_gap = (estimate - i32::from(left)).abs();
    let above_gap = (estimate - i32::from(above)).abs();
    let corner_gap = (estimate - i32::from(corner)).abs();
    if left_gap <= above_gap && left_gap <= corner_gap {
        left
    } else if above_gap <= corner_gap {
        above
    } else {
        corner
    }
}
