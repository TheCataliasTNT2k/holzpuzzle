use std::cmp::max;

use image::{Rgb, RgbImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use itertools::Itertools;
use rusttype::{Font, Scale};

use crate::ProgramStorage;
use crate::rect::{get_smallest_side, PlacedRectangle, RecId};

#[allow(unused)]
pub(crate) fn get_color(id: RecId, len: usize) -> Rgb<u8> {
    // let multiplier = (255 / len) as RecId;
    Rgb([
        255,
        255,
        255
    ])
}

pub(crate) fn draw_image(path: &str, storage: &ProgramStorage, data: &Vec<PlacedRectangle>) {
    let multiplyer = max(1, 40 / get_smallest_side(data));
    let font_size = 20 as f32;
    let big_rect = storage.rect_configuration.big_rect;
    let width = big_rect.width * multiplyer;
    let height = big_rect.height * multiplyer;
    let mut image = RgbImage::new(width + 20, height + 60);
    draw_filled_rect_mut(&mut image, Rect::at(9, 9).of_size(width + 2, height + 2), Rgb([0u8, 0u8, 255u8]));
    draw_hollow_rect_mut(&mut image, Rect::at(9, 9).of_size(width + 2, height + 2), Rgb([0u8, 255u8, 0u8]));

    let font = Font::try_from_vec(Vec::from(include_bytes!("../DejaVuSans.ttf") as &[u8])).unwrap();

    data.iter().for_each(|r| {
        let col = get_color(r.rect.id, storage.rect_configuration.available_blocks.len());
        draw_filled_rect_mut(&mut image, Rect::at((r.x * multiplyer) as i32 + 10, (r.y * multiplyer) as i32 + 10).of_size(r.rect.width * multiplyer, r.rect.height * multiplyer), col);
        let col = Rgb([255 - col[0], 255 - col[1], 255 - col[2]]);
        draw_hollow_rect_mut(&mut image, Rect::at((r.x * multiplyer) as i32 + 10, (r.y * multiplyer) as i32 + 10).of_size(r.rect.width * multiplyer, r.rect.height * multiplyer), col);
        draw_text_mut(
            &mut image,
            col,
            (r.x * multiplyer + r.rect.width * multiplyer / 2) as i32,
            (r.y * multiplyer + r.rect.height * multiplyer / 2) as i32,
            Scale { x: font_size, y: font_size },
            &font,
            &format!("{}", r.rect.id),
        );
        image.save(path).expect("no panic!");
    });

    draw_text_mut(
        &mut image,
        Rgb([255u8, 255u8, 255u8]),
        70,
        (height + 10) as i32,
        Scale { x: font_size, y: font_size },
        &font,
        &data.iter().map(|r| r.rect.id).sorted().join(", "),
    );
    image.save(path).expect("no panic!");
}
