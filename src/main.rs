extern crate core;

use std::collections::{BTreeSet, HashSet};
use std::time::Instant;

use itertools::Itertools;

use crate::data_configuration::{mm100_rects, RectConfiguration};
use crate::rect::{Combination, RecId, Rectangle, RectCombinationStorage, redup_comb_iter};
use crate::rect_image::draw_image;
use crate::steps::{step1_generate_candiates, step2_deduplication, step3_check_candidate, step3_filter_fitting_candidates, step4_calculate_matches, step5_sort_final_combinations};

mod rect;
mod steps;
mod rect_image;
mod data_configuration;

pub(crate) struct ProgramStorage<'a> {
    pub rect_configuration: &'a RectConfiguration,
    pub gathered_combinations: RectCombinationStorage,
    pub deduplicated_combinations: RectCombinationStorage,
    pub solutions: RectCombinationStorage,
    pub combined_solutions: HashSet<BTreeSet<Combination>>,
    pub final_combinations: Vec<Combination>,
    pub settings: Settings,
}

impl ProgramStorage<'_> {
    fn new(rect_configuration: &RectConfiguration, settings: Settings) -> ProgramStorage {
        ProgramStorage {
            rect_configuration,
            gathered_combinations: Default::default(),
            deduplicated_combinations: Default::default(),
            solutions: Default::default(),
            combined_solutions: Default::default(),
            final_combinations: vec![],
            settings,
        }
    }
}

pub struct Settings {
    pub thread_count: u8,
    pub min_solution_area: u32,
    pub min_rectangle_amount: u8,
    pub max_rectangle_amount: u8,
    pub distance_between_rectangles: u32,
    pub steps: [bool; 4],
    pub candidates_path: Option<&'static str>,
    pub fitting_candidates_path: Option<&'static str>,
    pub deduplicated_combinations_path: Option<&'static str>,
    pub solutions_filepath: Option<&'static str>,
    pub final_combinations_path: Option<&'static str>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            thread_count: 2,
            min_solution_area: 1,
            min_rectangle_amount: 1,
            max_rectangle_amount: 100,
            distance_between_rectangles: 0,
            steps: [false, false, false, false],
            candidates_path: None,
            fitting_candidates_path: None,
            deduplicated_combinations_path: None,
            solutions_filepath: None,
            final_combinations_path: None,
        }
    }
}

fn main() {
    let start = Instant::now();
    let rects = mm100_rects();

    let settings = Settings {
        thread_count: 16,
        min_solution_area: rects.available_blocks.iter().map(|b| b.area).sum::<u32>() - 2 * rects.big_rect.area,
        min_rectangle_amount: 3,
        max_rectangle_amount: 9,
        distance_between_rectangles: 100,
        candidates_path: Some("./candidates.txt"),
        deduplicated_combinations_path: Some("./deduplicated_candidates.txt"),
        fitting_candidates_path: Some("./fitting_candidates.txt"),
        solutions_filepath: Some("./solutions.txt"),
        final_combinations_path: Some("./final_candidates.txt"),
        steps: [false, false, false, false],
    };
    let mut storage = ProgramStorage::new(&rects, settings);

    println!("Using blocks:\n{}\n", storage.rect_configuration.available_blocks.iter().map(|b| format!("ID: {}, area: {}", b.id, b.area)).join("\n"));
    println!("Big rect area = {}\nSmall react area sum = {}\n", 3 * rects.big_rect.area, storage.rect_configuration.available_blocks.iter().map(|b| b.area).sum::<u32>());

    step1_generate_candiates(&mut storage);
    step2_deduplication(&mut storage);
    step3_filter_fitting_candidates(&mut storage);
    step4_calculate_matches(&mut storage);
    step5_sort_final_combinations(&mut storage);

    println!("The whole run took us {} seconds!", start.elapsed().as_secs());
    println!("{}", storage.solutions.len());
}

#[test]
fn test() {
    let rects = mm100_rects();

    let settings = Settings {
        thread_count: 4,
        steps: [false, false, false, false],
        distance_between_rectangles: 100,
        ..Default::default()
    };
    let storage = ProgramStorage::new(&rects, settings);

    // check all permutations for solution
    let s = "5,12,13,14,16".split('\n');

    for s1 in s {
        let mut c = BTreeSet::new();
        for id in s1.trim().split(',') {
            c.insert(*(storage.rect_configuration.available_block_map.get(&id.trim().parse::<RecId>().unwrap()).unwrap()));
        }
        println!("Testing possible solution: {}", s1.split(',').join(" "));
        if let Some(data) = step3_check_candidate(100, 0, &storage, &c) {
            println!("Solution is: {}", data.iter().map(|r| format!("{} {} {}", r.rect.id, r.rect.height, r.rect.width)).join("  "));
            println!("Area is: {}", data.iter().map(|r| r.rect.area).sum::<u32>());
            draw_image("./out.png", &storage, &data);
        } else {
            println!("Not a solution!");
        }
    }

    let x: Vec<Combination> = redup_comb_iter(storage.solutions.iter().filter(
        |c| c.iter().map(|r| r.area).sum::<u32>() >= 300000
    )
                                                      .clone(), &storage).collect();
    for i in 0..x.len() {
        let is = x.get(i).unwrap();
        println!("{}", is.iter().map(|r| r.id).join(","));
        for j in i + 1..x.len() {
            let js = x.get(j).unwrap();
            let union: HashSet<RecId> = (is | js).iter().map(|r| r.id).collect();
            if is.len() + js.len() == union.len() && storage.rect_configuration.available_blocks.len() - union.len() <= storage.settings.max_rectangle_amount as usize {
                println!("{}    {}", is.iter().map(|r| r.id).join(", "), js.iter().map(|r| r.id).join(", "));
                let c: BTreeSet<Rectangle> = storage.rect_configuration.available_block_map.iter()
                    .filter(|(id, _)| !union.contains(id))
                    .map(|(_, r)| r)
                    .copied()
                    .collect();
                println!("Testing: {}", c.iter().map(|r| r.id).join(", "));
                if let Some(data) = step3_check_candidate(0, 0, &storage, &c) {
                    println!("Solution is: {}", data.iter().map(|r| format!("{} {} {}", r.rect.id, r.rect.height, r.rect.width)).join("  "));
                    println!("Area is: {}", data.iter().map(|r| r.rect.area).sum::<u32>());
                    draw_image("./out.png", &storage, &data);
                } else {
                    println!("Not a solution!");
                }
            }
        }
    }
}


/*
4,5,6,8,9,18



2,3,13,14,16,18








2 3 6 12 9 15
2 3 6 5 9 15

2 3 5 6 9 15
2 3 5 6 8 9
2 3 6 9 12 15
2 3 6 8 9 12
310616
*/

/*

1,2,3,4,5,6
7,8,9,10,11,12
13,14,15,16,17,18
7,8,9,10,11,5
 */


/*
3,4,5,7,13  2,6,10,11,15,17,18

3,4,5,10,13  2,6,7,11,15,17,18
3,4,5,10,13  1,6,7,8,11,15,18



 */



/*
// filter combinations by area
let s: RectCombinationStorage = storage.solutions.iter().filter(
    |c| c.iter().map(|r| r.area).sum::<u32>() >= 310000
)
    .cloned()
    .unique_by(get_unique_combination_key)
    .sorted_by_key(|c| -(c.iter().map(|r| r.area).sum::<u32>() as i32))
    .collect();
println!("{}", s.len());
combination_storage_to_file("./sorted.txt", &s);


// filter for combinations with specific rect
let x = storage.solutions.iter().filter(
    |c| c.iter().any(|r| r.id == 16)
)
    .cloned()
    .sorted_by_key(|c| -(c.iter().map(|r| r.area).sum::<u32>() as i32))
    .collect::<Vec<Combination>>()
    .first().unwrap();

println!("{}, {}", x.iter().map(|r| r.id).join(", "), 2+2);*/
//
// x.iter().map(|r| r.area).sum::<u32>()

/*// check specific permutation with specific sizes (debug code)
let mut rect2 = Rectangle::new(-1, 9, 25);
mem::swap(&mut storage.big_rect, &mut rect2);
let x: Vec<Rectangle> = "1 4 17  2 7 7  4 3 8  3 2 22"
     .split("  ")
     .map(|s| {
         let split = s.split(" ").collect::<Vec<&str>>();
             Rectangle::new(
                 split[0].parse::<RecId>().unwrap(), split[1].parse::<RecDimension>().unwrap(), split[2].parse::<RecDimension>().unwrap()
             )
     }
     ).collect();
 println!("{}", check_permutation(&storage, x.iter().collect()));*/

// check collision between two placed rectangles (debug)
/*let r1 = PlacedRectangle {
    rect: *storage.available_block_map.get(&8).unwrap(),
    x: 0,
    y: 0,
};
let r2 = PlacedRectangle {
    rect: *storage.available_block_map.get(&11).unwrap(),
    x: 100,
    y: 0,
};
let x = vec![r1, r2];
println!("{}", r2.check_collision(&r1));*/


// redup combination
/*
let mut c = BTreeSet::new();
for id in "1,2,3,4,5,6".split(',') {
    c.insert(*storage.available_block_map.get(&id.trim().parse::<RecId>().unwrap()).unwrap());
}
let mut stuff: RectCombinationStorage = HashSet::new();
stuff.insert(c.clone());
let stuff2: Vec<Combination> = redup_comb_iter(stuff.iter(), &storage).collect();
println!("{}", stuff2.iter().map(|c| c.iter().map(|r| r.id).join(", ")).join("\n"));
*/
