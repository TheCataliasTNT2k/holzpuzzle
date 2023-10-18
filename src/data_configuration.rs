use std::collections::{HashMap, HashSet};

use crate::rect::{RecId, Rectangle};

pub(crate) struct RectConfiguration {
    pub big_rect: Rectangle,
    pub available_blocks: Vec<Rectangle>,
    pub available_block_map: HashMap<RecId, Rectangle>,
    pub rotated_available_block_map: HashMap<RecId, HashSet<Rectangle>>,
}

impl RectConfiguration {
    pub(crate) fn new(big_rect: Rectangle, available_blocks: Vec<Rectangle>) -> RectConfiguration {
        let block_map: HashMap<RecId, Rectangle> = available_blocks.iter().map(|b| (b.id, *b)).collect();
        RectConfiguration {
            big_rect,
            rotated_available_block_map: available_blocks.iter().map(|r| (r.id, r.get_possible_orientations(&big_rect))).collect(),
            available_blocks,
            available_block_map: block_map,
        }
    }
}

#[allow(dead_code)]
/// rounded to millimeter
pub(crate) fn mm_rects() -> RectConfiguration {
    RectConfiguration::new(
        Rectangle::new(-1, 47, 71),
        vec![
            Rectangle::new(1, 45, 19),
            Rectangle::new(2, 27, 22),
            Rectangle::new(3, 27, 25),
            Rectangle::new(4, 27, 27),
            Rectangle::new(5, 32, 17),
            Rectangle::new(6, 32, 22),
            Rectangle::new(7, 17, 20),
            Rectangle::new(8, 22, 12),
            Rectangle::new(9, 17, 14),
            Rectangle::new(10, 17, 19),
            Rectangle::new(11, 24, 14),
            Rectangle::new(12, 32, 17),
            Rectangle::new(13, 44, 17),
            Rectangle::new(14, 32, 14),
            Rectangle::new(15, 22, 12),
            Rectangle::new(16, 52, 12),
            Rectangle::new(17, 45, 12),
            Rectangle::new(18, 22, 10),
        ],
    )
}

#[allow(dead_code)]
/// rounded to mm * 10^-1
pub(crate) fn mm10_rects() -> RectConfiguration {
    RectConfiguration::new(
        Rectangle::new(-1, 464, 704),
        vec![
            Rectangle::new(1, 450, 198),
            Rectangle::new(2, 274, 223),
            Rectangle::new(3, 274, 249),
            Rectangle::new(4, 274, 274),
            Rectangle::new(5, 323, 173),
            Rectangle::new(6, 323, 223),
            Rectangle::new(7, 173, 200),
            Rectangle::new(8, 224, 124),
            Rectangle::new(9, 173, 148),
            Rectangle::new(10, 173, 198),
            Rectangle::new(11, 249, 148),
            Rectangle::new(12, 323, 173),
            Rectangle::new(13, 448, 174),
            Rectangle::new(14, 323, 148),
            Rectangle::new(15, 224, 124),
            Rectangle::new(16, 524, 123),
            Rectangle::new(17, 455, 123),
            Rectangle::new(18, 224, 99),
        ],
    )
}

/// rounded to mm * 10^-2
pub(crate) fn mm100_rects() -> RectConfiguration {
    RectConfiguration::new(
        Rectangle::new(-1, 4635, 7040),
        vec![
            Rectangle::new(1, 4500, 1980),
            Rectangle::new(2, 2740, 2235),
            Rectangle::new(3, 2740, 2490),
            Rectangle::new(4, 2740, 2740),
            Rectangle::new(5, 3235, 1730),
            Rectangle::new(6, 3230, 2235),
            Rectangle::new(7, 1735, 2000),
            Rectangle::new(8, 2240, 1240),
            Rectangle::new(9, 1735, 1485),
            Rectangle::new(10, 1735, 1980),
            Rectangle::new(11, 2495, 1485),
            Rectangle::new(12, 3235, 1735),
            Rectangle::new(13, 4485, 1740),
            Rectangle::new(14, 3235, 1485),
            Rectangle::new(15, 2240, 1240),
            Rectangle::new(16, 5245, 1235),
            Rectangle::new(17, 4550, 1235),
            Rectangle::new(18, 2240, 990),
        ],
    )
}
