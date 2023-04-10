extern crate core;

use std::{fs, thread};
use std::cmp::{max, min};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::process::id;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use image::{Rgb, RgbImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use rusttype::{Font, Scale};
use imageproc::rect::Rect;
use itertools::Itertools;

use crate::rect::{Combination, combination_from_string, combination_storage_from_file, combination_storage_to_file, combination_to_string, get_unique_combination_key, PlacedRectangle, RecDimension, RecId, Rectangle, RectCombinationStorage, redup_comb_iter};

mod rect;

pub(crate) struct ProgramStorage {
    pub big_rect: Rectangle,
    pub available_blocks: Vec<Rectangle>,
    pub available_block_map: HashMap<RecId, Rectangle>,
    pub rotated_available_block_map: HashMap<RecId, HashSet<Rectangle>>,
    pub gathered_combinations: RectCombinationStorage,
    pub deduplicated_combinations: RectCombinationStorage,
    pub solutions: RectCombinationStorage,
    pub combined_solutions: HashSet<BTreeSet<Combination>>,
    pub final_combinations: Vec<Combination>,
    pub settings: Settings,
}

pub struct Settings {
    pub thread_count: u8,
    pub min_solution_area: u32,
    pub min_rectangle_amount: u8,
    pub max_rectangle_amount: u8,
    pub distance_between_rectangles: u32,
    pub steps: [bool; 4],
    pub candidates_path: &'static str,
    pub fitting_candidates_path: &'static str,
    pub deduplicated_combinations_path: &'static str,
    pub solutions_filepath: &'static str,
    pub final_combinations_path: &'static str,

}

/// collect all candidates, which may fit inside the big rectangle
fn step1_generate_candiates(storage: &mut ProgramStorage) {
    let mut gathered_combinations = combination_storage_from_file(storage.settings.candidates_path, storage);
    println!("GATHERING COMBINATIONS...");
    if !storage.settings.steps[0] {
        println!("SKIPPED");
        storage.gathered_combinations = gathered_combinations;
        return;
    }
    let start = Instant::now();
    let mut counter: u64 = 0;
    // for each number of rectangles, collect all combinations
    for s in storage.settings.min_rectangle_amount..=storage.settings.max_rectangle_amount {
        let mut counter2 = 0;
        for comb in storage.available_blocks.iter().combinations(s as usize) {
            counter += 1;
            if counter % 1000000 == 0 {
                println!("{s} {}, {} {}", counter, gathered_combinations.len(), start.elapsed().as_secs());
            }
            // if this combination may fit in the big rectangle, keep it
            if comb.iter().map(|r| r.area).sum::<u32>() <= storage.big_rect.area {
                counter2 += 1;
                gathered_combinations.insert(comb.iter().map(|r| **r).collect());
            }
        }
        println!("{s} {counter2}");
    }
    // sort, save and all that stuff
    println!("{}", gathered_combinations.len());
    // be careful, this might eliminate possible final solutions!!!!!!
    gathered_combinations.retain(|s|
        s.iter().map(|r| r.area).sum::<u32>() >= storage.settings.min_solution_area
    );
    println!("{}", gathered_combinations.len());
    storage.gathered_combinations = gathered_combinations;

    combination_storage_to_file(storage.settings.candidates_path, &storage.gathered_combinations);

    println!("GATHERING COMBINATIONS... DONE AFTER {} seconds, found {} combinations", start.elapsed().as_secs(), storage.gathered_combinations.len());
}

/// deduplicates all equivalent combinations\
/// needs changes in filter_fitting_candidates to use this output as next step\
/// needs "reduplication" before calculate_matches!
fn step2_deduplication(storage: &mut ProgramStorage) {
    let candidates = storage.gathered_combinations.iter().cloned().collect::<Vec<BTreeSet<Rectangle>>>();

    println!("DEDUPLICATING {} COMBINATIONS...", candidates.len());
    storage.deduplicated_combinations = candidates.into_iter()
        .unique_by(get_unique_combination_key)
        .collect();
    fs::write(
        storage.settings.deduplicated_combinations_path,
        storage.deduplicated_combinations.iter()
            .map(combination_to_string)
            .join("\n"),
    ).expect("Unable to write file");
    println!("We have {} deduplicated combinations!", storage.deduplicated_combinations.len());
}

/// check for each combination, if it can be arranged inside the big rectangle
fn step3_filter_fitting_candidates(storage: &mut ProgramStorage) {
    println!("CALCULATING SOLUTIONS (1 layer)...");
    if !storage.settings.steps[1] {
        storage.solutions = combination_storage_from_file(storage.settings.fitting_candidates_path, storage);
        println!("SKIPPED");
        return;
    }

    let start = Instant::now();
    let output = Arc::new(Mutex::new(HashSet::new()));
    // do all the checking threaded
    thread::scope(|s| {
        let input = Arc::new((
            &*storage, Mutex::new(
                (
                    0,
                    // sort by area of combination
                    storage.deduplicated_combinations.iter().cloned()
                        .sorted_by_key(|c| -(c.iter().map(|r| r.area).sum::<u32>() as i32))
                        .collect::<Vec<BTreeSet<Rectangle>>>()
                )
            )
        ));
        let mut threads = Vec::new();

        // run threads because this will take a while
        for i in 1..=storage.settings.thread_count {
            let thread_input = input.clone();
            let thread_output = output.clone();
            let thread = s.spawn(move || {
                step3_thread_procedure(i, thread_input, thread_output);
            });
            threads.push(thread);
        }
        println!("Threads created, waiting for results...");
        for thread in threads {
            thread.join().unwrap();
        }
    });
    // save, sort, do all that stuff
    println!("All threads finished! Took us {} seconds", start.elapsed().as_secs());
    let fitting_candidates = Arc::try_unwrap(output).unwrap().into_inner().unwrap();
    combination_storage_to_file(storage.settings.fitting_candidates_path, &fitting_candidates);
    storage.solutions = fitting_candidates;
    println!("CALCULATING SOLUTIONS (1 layer)... DONE AFTER {} seconds, found {} solutions", start.elapsed().as_secs(), storage.solutions.len());
}

/// this funktion is the main function, which will be run by the threads of filter_fitting_candidates
fn step3_thread_procedure(number: u8,
                          input: Arc<(&ProgramStorage, Mutex<(i32, Vec<BTreeSet<Rectangle>>)>)>,
                          output: Arc<Mutex<RectCombinationStorage>>,
) {
    let thread_start = Instant::now();
    let storage = input.0;
    let mutex = &input.1;
    loop {
        // get combination to check
        let mut lock = mutex.lock().unwrap();
        if lock.1.is_empty() {
            println!("Thread {number} is shutting down! I was alive {} for seconds.", thread_start.elapsed().as_secs());
            drop(lock);
            return;
        }
        lock.0 += 1;
        let counter = lock.0;
        let data = lock.1.remove(0);
        drop(lock);
        if counter % 100 == 0 {
            println!("Thread {number} working counter {counter} with data {}.", data.iter().map(|r| r.id).join(","));
        }
        // ic combination can be put somehow in the big rect, store it
        if step3_check_candidate(number, counter, storage, &data).is_some() {
            let mut lock = output.lock().unwrap();
            lock.insert(data);
            println!("Thread {number}: We have {} candidates so far.", lock.len());
            drop(lock);
        }
    }
}

/// for checking if a combination fits inside the big rect
fn step3_check_candidate(number: u8, counter: i32,
                         storage: &ProgramStorage,
                         candidate: &BTreeSet<Rectangle>,
) -> Option<Vec<PlacedRectangle>> {
    let c_start = Instant::now();
    // because the small rectangles can be rotated, we need to check each combination of each rotation for the input
    for product in candidate.iter().map(|r| storage.rotated_available_block_map.get(&r.id).unwrap()).multi_cartesian_product() {
        // because I have no better idea, just check each permutation of each combination individually
        for per in product.iter().cloned().permutations(product.len()) {
            if let Some(sol) = step3_check_permutation(storage, per.clone()) {
                if counter % 100 == 0 && number > 0 {
                    println!("Thread {number} worked {counter} in {} seconds (success)", c_start.elapsed().as_secs());
                }
                return Some(sol);
            }
        }
    }
    if counter % 100 == 0 && number > 0 {
        println!("Thread {number} worked {counter} in {} seconds (fail)", c_start.elapsed().as_secs());
    }
    None
}

/// if a specific set of rectangles fits inside the big rect, without rotating or rearranging them
fn step3_check_permutation(storage: &ProgramStorage, candidate: Vec<&Rectangle>) -> Option<Vec<PlacedRectangle>> {
    /*
    idea:
    put all rectangles inside the big rectangle in order
    if rect has e.g. width 23 and x is 0 at the moment, it will occupy 0, 1, 2... 22, not 23!
    leave space horizontally and vertically to accommodate for slightly smaller or bigger ones
    if width is filled, go to "higher" row
    move rectangles closer together without collisions (touching is also not allowed)
     */
    // x value to put next rectangle, with "spaces"
    let mut x_with_spaces: RecDimension = 0;
    // x without spaces to check for "full row"
    let mut x_normal: RecDimension = 0;
    // how big the "spaces" should be
    let distance = storage.settings.distance_between_rectangles;
    // store placed rects
    let mut placed_rects: Vec<PlacedRectangle> = vec![];
    // store the height of each column
    let x_size = storage.big_rect.width + (storage.big_rect.width / storage.settings.max_rectangle_amount as RecDimension) as RecDimension * distance;
    let mut taken_with_spaces = vec![0 as RecDimension; x_size as usize];
    let mut taken = vec![0 as RecDimension; x_size as usize];
    // make everything more cache efficient (width and height with spaces put in)
    let big_width = storage.big_rect.width;
    for rect in candidate {
        // if the new rect will collide with the border of the big rect
        if x_normal + rect.width >= big_width {
            // go back to x 0 for next row
            x_normal = 0;
            x_with_spaces = 0;
            if taken.iter().max().unwrap() > &storage.big_rect.height {
                return None;
            }
        }
        // the new rect can not be lower than the highest point within its "area"
        let mut ymax_with_spaces: RecDimension = 0;
        for i in x_with_spaces..x_with_spaces + rect.width {
            ymax_with_spaces = max(ymax_with_spaces, taken_with_spaces[i as usize]);
        }
        let mut ymax: RecDimension = 0;
        for i in x_with_spaces..x_with_spaces + rect.width {
            ymax = max(ymax, taken[i as usize]);
        }
        // if this is not the first row, add space to height
        if ymax_with_spaces > 0 {
            ymax_with_spaces += distance;
        }
        // add new rect to placed rects, at this position
        placed_rects.push(PlacedRectangle {
            rect: *rect,
            x: x_with_spaces,
            y: ymax_with_spaces,
        });
        // calculate new height to avoid colliding with this rectangle when setting the next row
        ymax_with_spaces += rect.height;
        ymax += rect.height;
        // store ymax for all "columns" the new rect occupies
        for i in x_with_spaces..x_with_spaces + rect.width {
            taken_with_spaces[i as usize] = ymax_with_spaces;
        }
        for i in x_with_spaces..x_with_spaces + rect.width {
            taken[i as usize] = ymax;
        }
        // calculate new x values for next rect
        x_with_spaces += rect.width + distance;
        x_normal += rect.width;
    }
    if taken.iter().max().unwrap() > &storage.big_rect.height {
        return None;
    }
    let mut compacted = true;
    let mut p;
    // move the placed rects closer together, without touching or colliding
    while compacted {
        compacted = false;
        for i in 0..placed_rects.len() {
            p = *placed_rects.get(i).unwrap();
            // move this rect as far as possible
            if p.compact(&placed_rects) {
                compacted = true;
                placed_rects[i] = p;
            }
        }
    }
    // check if all rects are inside the big rect now
    if placed_rects.iter().all(|p| p.check_bounds(storage)) {
        Some(placed_rects)
    } else {
        None
    }
}

/// take three disjunctive combinations of the combinations, which fit inside the big rect\
/// these three combinations represent the three layers inside the big rect
fn step4_calculate_matches(storage: &mut ProgramStorage) {
    if !storage.settings.steps[2] {
        storage.combined_solutions = fs::read_to_string(storage.settings.solutions_filepath).unwrap_or_else(|_| "".to_owned())
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(|line|
                line.split(' ')
                    .map(|c| combination_from_string(storage, c)).collect()
            ).collect::<HashSet<BTreeSet<Combination>>>();
        println!("CALCULATING COMBINED SOLUTIONS (3 layers)...\nSKIPPED");
        return;
    }
    // sort the candidates by area
    let candidates = redup_comb_iter(storage.solutions.iter(), storage)
        .sorted_by_key(|c| -(c.iter().map(|r| r.area).sum::<u32>() as i32))
        .collect::<Vec<BTreeSet<Rectangle>>>();
    let start = Instant::now();

    println!("CALCULATING COMBINED SOLUTIONS (3 layers) with {} candidates...", candidates.len());
    for i in 0..candidates.len() {
        if i % 100 == 0 {
            println!("{} {}", i, storage.combined_solutions.len());
        }
        let is = candidates.get(i).unwrap();
        for j in i + 1..candidates.len() {
            let js = candidates.get(j).unwrap();
            let ij = is | js;
            // two candidates need to be disjunctive, otherwise they can not be a solution
            if ij.len() == is.len() + js.len() {
                for k in j + 1..candidates.len() {
                    let ks = candidates.get(k).unwrap();
                    // three candidates need to be disjunctive, otherwise they can not be a solution
                    let amount = (&ij | ks).len();
                    if amount >= storage.available_blocks.len() {
                        let mut solution = BTreeSet::new();
                        solution.insert(is.to_owned().clone());
                        solution.insert(js.to_owned().clone());
                        solution.insert(ks.to_owned().clone());
                        storage.combined_solutions.insert(solution);
                    }
                }
            }
        }
    }

    fs::write(
        storage.settings.solutions_filepath,
        storage.combined_solutions.iter()
            .map(|l| l.iter().map(combination_to_string).join(" "))
            .join("\n"),
    ).expect("Unable to write file");
    println!("CALCULATING COMBINED SOLUTIONS (3 layers)... DONE AFTER {} seconds, found {} combined solutions", start.elapsed().as_secs(), storage.combined_solutions.len());
}

/// take the possible solutions for three layers and split them in single layer combinations\
/// sort by how often each combination appears within the possible solutions
fn step5_sort_final_combinations(storage: &mut ProgramStorage) {
    if !storage.settings.steps[3] {
        println!("SORTING FINAL COMBINATIONS...\nSKIPPED");
        return;
    }
    let mut combination_counter_map = HashMap::new();
    let mut dedup_string_combination_map = HashMap::new();
    let start = Instant::now();

    println!("SORTING FINAL COMBINATIONS with {} solutions...", storage.combined_solutions.len());

    storage.combined_solutions.iter().for_each(|solution| solution.iter().for_each(|c| {
        let dedup = get_unique_combination_key(c);
        dedup_string_combination_map.entry(dedup.clone()).or_insert(c);
        *combination_counter_map.entry(dedup).or_insert(0) += 1;
    }));
    let final_combinations: Vec<Combination> = combination_counter_map.keys()
        .sorted_by_key(|s| combination_counter_map.get(*s).unwrap())
        .map(|s| (*dedup_string_combination_map.get(s).unwrap()).clone())
        .collect();

    fs::write(storage.settings.final_combinations_path,
              final_combinations.iter().map(|c| c.iter().map(|r| r.id).join(" ")).join("\n"),
    ).expect("Unable to write file");
    storage.final_combinations = final_combinations;
    println!("CALCULATING FINAL COMBINATIONS... DONE AFTER {} seconds, found {} final combinations", start.elapsed().as_secs(), storage.final_combinations.len());
}

fn get_color(id: RecId, len: usize) -> Rgb<u8> {
    // let multiplier = (255 / len) as RecId;
    Rgb([
        255,
        255,
        255
    ])
}

fn draw_image(path: &str, storage: &ProgramStorage, data: &Vec<PlacedRectangle>) {
    let mut image = RgbImage::new(storage.big_rect.width + 20, storage.big_rect.height + 60);
    draw_filled_rect_mut(&mut image, Rect::at(9, 9).of_size(storage.big_rect.width + 2, storage.big_rect.height + 2), Rgb([0u8, 0u8, 255u8]));
    draw_hollow_rect_mut(&mut image, Rect::at(9, 9).of_size(storage.big_rect.width + 2, storage.big_rect.height + 2), Rgb([0u8, 255u8, 0u8]));

    let font = Font::try_from_vec(Vec::from(include_bytes!("../DejaVuSans.ttf") as &[u8])).unwrap();

    data.iter().for_each(|r| {
        let col = get_color(r.rect.id, storage.available_blocks.len());
        draw_filled_rect_mut(&mut image, Rect::at(r.x as i32 + 10, r.y as i32 + 10).of_size(r.rect.width, r.rect.height), col);
        let col = Rgb([255 - col[0], 255 - col[1], 255 - col[2]]);
        draw_hollow_rect_mut(&mut image, Rect::at(r.x as i32 + 10, r.y as i32 + 10).of_size(r.rect.width, r.rect.height), col);
        draw_text_mut(
            &mut image,
            col,
            (r.x + r.rect.width / 2) as i32,
            (r.y + r.rect.height / 2) as i32,
            Scale { x: 50_f32, y: 50_f32 },
            &font,
            &format!("{}", r.rect.id)
        );
    });

    draw_text_mut(
        &mut image,
        Rgb([255u8, 255u8, 255u8]),
        70,
        (storage.big_rect.height + 10) as i32,
        Scale { x: 50 as f32, y: 50 as f32 },
        &font,
        &data.iter().map(|r| r.rect.id).sorted().join(", ")
    );
    image.save(path).expect("no panic!");
}

// 5, 6, 11, 13, 16
// 6, 11, 12, 13, 16

//1, 2, 3, 5, 9
//1, 2, 3, 6, 9

// 712 8545

fn main() {
    let start = Instant::now();
    /*let rect = Rectangle::new(-1, 47, 71);
    let blocks = vec![
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
    ];*/
    let rect = Rectangle::new(-1, 464, 704);
    let blocks = vec![
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
    ];
    /*let rect = Rectangle::new(-1, 4635, 7040);
    let blocks = vec![
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
    ];*/
    let block_map: HashMap<RecId, Rectangle> = blocks.iter().map(|b| (b.id, *b)).collect();
    let rotated_available_blocks: HashMap<RecId, HashSet<Rectangle>> = blocks.iter().map(|r| (r.id, r.get_possible_orientations(&rect))).collect();

    let settings = Settings {
        thread_count: 16,
        min_solution_area: blocks.iter().map(|b| b.area).sum::<u32>() - 2 * rect.area,
        min_rectangle_amount: 3,
        max_rectangle_amount: 9,
        distance_between_rectangles: 100,
        candidates_path: "./candidates.txt",
        deduplicated_combinations_path: "./deduplicated_candidates.txt",
        fitting_candidates_path: "./fitting_candidates.txt",
        solutions_filepath: "./solutions.txt",
        final_combinations_path: "./final_candidates.txt",
        steps: [false, false, false, false],
    };
    let mut storage = ProgramStorage {
        big_rect: rect,
        available_blocks: blocks,
        available_block_map: block_map,
        rotated_available_block_map: rotated_available_blocks,
        gathered_combinations: Default::default(),
        deduplicated_combinations: Default::default(),
        solutions: Default::default(),
        combined_solutions: Default::default(),
        final_combinations: Default::default(),
        settings,
    };
    println!("Using blocks:\n{}\n", storage.available_blocks.iter().map(|b| format!("ID: {}, area: {}", b.id, b.area)).join("\n"));
    println!("Big rect area = {}\nSmall react area sum = {}\n", 3 * rect.area, storage.available_blocks.iter().map(|b| b.area).sum::<u32>());

    step1_generate_candiates(&mut storage);
    step2_deduplication(&mut storage);
    step3_filter_fitting_candidates(&mut storage);
    step4_calculate_matches(&mut storage);
    step5_sort_final_combinations(&mut storage);

    println!("The whole run took us {} seconds!", start.elapsed().as_secs());
    println!("{}", storage.solutions.len());




    // check all permutations for solution
    let s = "5,12,13,14,16".split('\n');

    for s1 in s {
        let mut c = BTreeSet::new();
        for id in s1.trim().split(',') {
            c.insert(*storage.available_block_map.get(&id.trim().parse::<RecId>().unwrap()).unwrap());
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

    let mut x: Vec<Combination> = redup_comb_iter(storage.solutions.iter().filter(
        |c| c.iter().map(|r| r.area).sum::<u32>() >= 300000
    )
        .clone(), &storage).collect();
    for i in 0..x.len() {
        let is = x.get(i).unwrap();
        println!("{}", is.iter().map(|r| r.id).join(","));
        for j in i+1..x.len() {
            let js = x.get(j).unwrap();
            let union: HashSet<RecId> = (is | js).iter().map(|r| r.id).collect();
            if is.len() + js.len() == union.len() && storage.available_blocks.len() - union.len() <= storage.settings.max_rectangle_amount as usize {
                println!("{}    {}", is.iter().map(|r| r.id).join(", "), js.iter().map(|r| r.id).join(", "));
                let c: BTreeSet<Rectangle> = storage.available_block_map.iter()
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

    /*x.clear();
    x.push();*/
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
