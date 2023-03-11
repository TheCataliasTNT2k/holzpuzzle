mod rect;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::{fs, thread};
use std::cmp::max;
use std::time::{Instant};
use itertools::{Itertools};

use std::sync::{Arc, Mutex};
use crate::rect::{RecDimension, RecId, Rectangle, RectCombinationStorage, combination_storage_to_file, combination_storage_from_file, combination_to_string, Combination, get_unique_combination_key, combination_from_string, PlacedRectangle};


pub(crate) struct ProgramStorage {
    pub big_rect: Rectangle,
    pub available_blocks: Vec<Rectangle>,
    pub available_block_map: HashMap<RecId, Rectangle>,
    pub rotated_available_block_map: HashMap<RecId, HashSet<Rectangle>>,
    pub gathered_combinations: RectCombinationStorage,
    pub meaningful_combinations: RectCombinationStorage,
    pub solutions: RectCombinationStorage,
    pub combined_solutions: HashSet<BTreeSet<Combination>>,
    pub final_combinations: Vec<Combination>,
    pub settings: Settings
}

pub struct Settings {
    pub thread_count: u8,
    pub min_solution_area_multiplier: f32,
    pub min_rectangle_amount: u8,
    pub max_rectangle_amount: u8,
    pub steps: [bool; 4],
    pub candidates_path: &'static str,
    pub fitting_candidates_path: &'static str,
    pub solutions_filepath: &'static str,
    pub final_combinations_path: &'static str,

}

/// collect all candidates, which may fit inside the big rectangle
fn generate_candidates(storage: &mut ProgramStorage) {
    let mut gathered_combinations = combination_storage_from_file(storage.settings.candidates_path, storage);
    println!("GATHERING COMBINATIONS....");
    if !storage.settings.steps[0] {
        println!("SKIPPED");
        storage.gathered_combinations = gathered_combinations;
        return;
    }
    let start = Instant::now();
    let mut counter: u64 = 0;
    // for each number of rectangles, collect all combinations
    for s in storage.settings.min_rectangle_amount..storage.settings.max_rectangle_amount + 1 {
        for comb in storage.available_blocks.iter().combinations(s as usize) {
            counter += 1;
            if counter % 1000000 == 0 {
                println!("{s} {}, {} {}", counter, gathered_combinations.len(), start.elapsed().as_secs());
            }
            // if this combination may fit in the big rectangle, keep it
            if comb.iter().map(|r| r.area).sum::<u32>() <= storage.big_rect.area {
                gathered_combinations.insert(comb.iter().map(|r| **r).collect());
            }
        }
        println!("{s} {counter}");
    }
    // sort, save and all that stuff
    println!("{}", gathered_combinations.len());
    // be careful, this might eliminate possible final solutions!!!!!!
    gathered_combinations.retain(|s|
        s.iter().map(|r| r.area).sum::<u32>() >=
        (storage.big_rect.area as f32 * storage.settings.min_solution_area_multiplier) as u32
    );
    println!("{}", gathered_combinations.len());
    storage.gathered_combinations = gathered_combinations;

    combination_storage_to_file(storage.settings.candidates_path, &storage.gathered_combinations);

    println!("GATHERING COMBINATIONS.... DONE AFTER {} seconds, found {} combinations", start.elapsed().as_secs(), storage.gathered_combinations.len());
}

/// not used\
/// deduplicates all equivalent combinations\
/// needs changes in filter_fitting_candidates to use this output as next step\
/// needs "reduplication" before calculate_matches!
/*fn filter_meaningful_combinations(storage: &mut ProgramStorage) {
    let candidates = storage.gathered_combinations.iter()
        .map(|c| (c.iter().map(|r| r.area).sum::<u32>(), c))
        .sorted_by_key(|(a, _)| -(*a as i32))
        .map(|(_, s)| s)
        .collect::<Vec<&BTreeSet<Rectangle>>>();
    let mut result = HashSet::new();

    println!("BLAAAAAAAAAAAAAAA with {} candidates....", candidates.len());
    for i in 0..candidates.len() {
        if i % 100 == 0 {
            println!("{} {}", i, result.len());
        }
        if result.len() >= candidates.len() {
            break;
        }
        let is = candidates.get(i).unwrap();
        for j in i+1..candidates.len() {
            let js = candidates.get(j).unwrap();
            let ij = *is | *js;
            if ij.len() == is.len() + js.len() && (storage.available_blocks.len() as u8) >= ij.len() as u8 + storage.settings.min_rectangle_amount {
                for k in j+1..candidates.len() {
                    let ks = candidates.get(k).unwrap();
                    let amount = (&ij | ks).len();
                    if amount >= storage.available_blocks.len() {
                        result.insert(is.to_owned().clone());
                        result.insert(ks.to_owned().clone());
                        result.insert(js.to_owned().clone());
                    }
                }
            }
        }
    }
    storage.meaningful_combinations = result.into_iter()
        .unique_by(get_unique_combination_key)
        .collect();
    fs::write(
        "./fiiiiiiiiiiiiiiiiiiiiileeeee",
        storage.meaningful_combinations.iter()
            .map(combination_to_string)
            .join("\n"),
    ).expect("Unable to write file");
    println!("We have {} meaningful combinations!", storage.meaningful_combinations.len());
}*/

/// check for each combination, if it can be arranged inside the big rectangle
fn filter_fitting_candidates(storage: &mut ProgramStorage) {
    println!("CALCULATING SOLUTIONS (1 layer)....");
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
                    storage.gathered_combinations.iter().cloned()
                        .sorted_by_key(|c| -(c.iter().map(|r| r.area).sum::<u32>() as i32))
                        .collect::<Vec<BTreeSet<Rectangle>>>()
                )
            )
        ));
        let mut threads = Vec::new();

        // run threads because this will take a while
        for i in 1..storage.settings.thread_count + 1 {
            let thread_input = input.clone();
            let thread_output = output.clone();
            let thread = s.spawn(move || {
                do_work(i, thread_input, thread_output);
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
    println!("CALCULATING SOLUTIONS (1 layer).... DONE AFTER {} seconds, found {} solutions", start.elapsed().as_secs(), storage.solutions.len());
}

/// this funktion is the main function, which will be run by the threads of filter_fitting_candidates
fn do_work(number: u8,
           input: Arc<(&ProgramStorage, Mutex<(i32, Vec<BTreeSet<Rectangle>>)>)>,
           output: Arc<Mutex<RectCombinationStorage>>,
) {
    let thread_start = Instant::now();
    let storage = input.0;
    let mutex = &input.1;
    loop {
        // get combination to check
        let mut lock = mutex.lock().unwrap();
        lock.0 += 1;
        let counter = lock.0;
        if lock.1.is_empty() {
            println!("Thread {number} is shutting down! I was alive {} for seconds.", thread_start.elapsed().as_secs());
            drop(lock);
            return;
        }
        let data = lock.1.remove(0);
        drop(lock);
        if counter % 100 == 0 {
            println!("Thread {number} working counter {counter} with data {}.", data.iter().map(|r| r.id).join(","));
        }
        // ic combination can be put somehow in the big rect, store it
        if check_candidate(number, counter, storage, &data).is_some() {
            let mut lock = output.lock().unwrap();
            lock.insert(data);
            println!("Thread {number}: We have {} candidates so far.", lock.len());
            drop(lock);
        }
    }
}

/// for checking if a combination fits inside the big rect
fn check_candidate(number: u8, counter: i32,
                   storage: &ProgramStorage,
                   candidate: &BTreeSet<Rectangle>,
) -> Option<Vec<Rectangle>> {
    let c_start = Instant::now();
    // because the small rectangles can be rotated, we need to check each combination of each rotation for the input
    for product in candidate.iter().map(|r| storage.rotated_available_block_map.get(&r.id).unwrap()).multi_cartesian_product() {
        // because I have no better idea, just check each permutation of each combination individually
        for per in product.iter().cloned().permutations(product.len()) {
            if check_permutation(storage, per.clone()) {
                if counter % 100 == 0 && number > 0 {
                    println!("Thread {number} worked {counter} in {} seconds (success)", c_start.elapsed().as_secs());
                }
                return Some(per.into_iter().copied().collect());
            }
        }
    }
    if counter % 100 == 0 && number > 0 {
        println!("Thread {number} worked {counter} in {} seconds (fail)", c_start.elapsed().as_secs());
    }
    None
}

/// if a specific set of rectangles fits inside the big rect, without rotating or rearranging them
fn check_permutation(storage: &ProgramStorage, candidate: Vec<&Rectangle>) -> bool {
    /*
    idea:
    put all rectangles inside the big rectangle in order
    if rect has e.g. width 23 and x is 0 at the moment, it will occupy 0, 1, 2.... 22, not 23!
    leave space horizontally and vertically to accommodate for slightly smaller or bigger ones
    if width is filled, go to "higher" row
    move rectangles closer together without collisions (touching is also not allowed)
     */
    // x value to put next rectangle, with "spaces"
    let mut x_with_spaces: RecDimension = 0;
    // x without spaces to check for "full row"
    let mut x_normal: RecDimension = 0;
    // how big the "spaces" should be
    let distance = 10;
    // store placed rects
    let mut placed_rects: Vec<PlacedRectangle> = vec![];
    // store the height of each column
    let x_size = storage.big_rect.width + (storage.big_rect.width / storage.settings.max_rectangle_amount as RecDimension) as RecDimension * distance;
    let mut taken = vec![0 as RecDimension; x_size as usize];
    // make everything more cache effizient (width and height with spaces put in)
    let big_width = storage.big_rect.width;
    let mut row_height: RecDimension = 0;
    let mut total_height: RecDimension = 0;
    for rect in candidate {
        // if the new rect will collide with the border of the big rect
        if x_normal + rect.width >= big_width {
            // go back to x 0 for next row
            x_normal = 0;
            x_with_spaces = 0;
            // store height of last column
            total_height += row_height;
            row_height = 0;
            // if the total height of all rects in one row, without spacings, is higher than the rect height, break
            if total_height > storage.big_rect.height {
                return false;
            }
        }
        // the new rect can not be lower than the highest point within its "area"
        let mut ymax: RecDimension = 0;
        for i in x_with_spaces..x_with_spaces + rect.width {
            ymax = max(ymax, taken[i as usize]);
        }
        // if this is not the first row, add space to last row
        if ymax > 0 {
            ymax += distance;
        }
        // add new rect to placed rects, at this position
        placed_rects.push(PlacedRectangle {
            rect: *rect,
            x: x_with_spaces,
            y: ymax,
        });
        // calculate new height to avoid this rectangle when setting the next row
        ymax += rect.height;
        // calculate new height of this row
        row_height = max(row_height, rect.height);
        // store ymax for all "columns" the new rect occupies
        for i in x_with_spaces..x_with_spaces + rect.width {
            taken[i as usize] = ymax;
        }
        // calculate new x values for next rect
        x_with_spaces += rect.width + distance;
        x_normal += rect.width;
    }
    // if the total height of all rects in one row, without spacings, is higher than the rect height, break
    if total_height > storage.big_rect.height {
        return false;
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
    placed_rects.iter().all(|p| p.check_bounds(storage))
}

/// take three disjunctive combinations of the combinations, which fit inside the big rect\
/// these three combinations represent the three layers inside the big rect
fn calculate_matches(storage: &mut ProgramStorage) {
    if !storage.settings.steps[2] {
        storage.combined_solutions = fs::read_to_string(storage.settings.solutions_filepath).unwrap_or_else(|_| "".to_owned())
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(|line|
                line.split(' ')
                .map(|c| combination_from_string(storage, c)).collect()
            ).collect::<HashSet<BTreeSet<Combination>>>();
        println!("CALCULATING COMBINED SOLUTIONS (3 layers)....\nSKIPPED");
        return;
    }
    // sort the candidates by area
    let candidates = storage.solutions.iter()
        .map(|c| (c.iter().map(|r| r.area).sum::<u32>(), c))
        .sorted_by_key(|(a, _)| -(*a as i32))
        .map(|(_, s)| s)
        .collect::<Vec<&BTreeSet<Rectangle>>>();
    let start = Instant::now();

    println!("CALCULATING COMBINED SOLUTIONS (3 layers) with {} candidates....", candidates.len());
    for i in 0..candidates.len() {
        if i % 100 == 0 {
            println!("{} {}", i, storage.combined_solutions.len());
        }
        let is = candidates.get(i).unwrap();
        for j in i+1..candidates.len() {
            let js = candidates.get(j).unwrap();
            let ij = *is | *js;
            // two candidates need to be disjunctive, otherwise they can not be a solution
            if ij.len() == is.len() + js.len() {
                for k in j+1..candidates.len() {
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
    println!("CALCULATING COMBINED SOLUTIONS (3 layers).... DONE AFTER {} seconds, found {} combined solutions", start.elapsed().as_secs(), storage.combined_solutions.len());
}

/// take the possible solutions for three layers and split them in single layer combinations\
/// sort by how often each combination appears within the possible solutions
fn sort_final_combinations(storage: &mut ProgramStorage) {
    if !storage.settings.steps[3] {
        println!("SORTING FINAL COMBINATIONS....\nSKIPPED");
        return;
    }
    let mut combination_counter_map = HashMap::new();
    let mut dedup_string_combination_map = HashMap::new();
    let start = Instant::now();

    println!("SORTING FINAL COMBINATIONS with {} solutions....", storage.combined_solutions.len());

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
              final_combinations.iter().map(|c| c.iter().map(|r| r.id).join(" ")).join("\n")
    ).expect("Unable to write file");
    storage.final_combinations = final_combinations;
    println!("CALCULATING FINAL COMBINATIONS.... DONE AFTER {} seconds, found {} final combinations", start.elapsed().as_secs(), storage.final_combinations.len());
}

// 5, 6, 11, 13, 16
// 6, 11, 12, 13, 16

//1, 2, 3, 5, 9
//1, 2, 3, 6, 9

// 712 8545

fn main() {
    let start = Instant::now();
    let rect = Rectangle::new(-1, 47, 71);
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
    ];
    /*let rect = Rectangle::new(-1, 464, 704);
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
    ];*/
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
        min_solution_area_multiplier: 0.9,
        min_rectangle_amount: 3,
        max_rectangle_amount: 9,
        candidates_path: "./candidates.txt",
        fitting_candidates_path: "./fitting_candidates.txt",
        solutions_filepath: "./solutions.txt",
        final_combinations_path: "./final_candidates.txt",
        steps: [false, false, true, true],
    };
    let mut storage = ProgramStorage {
        big_rect: rect,
        available_blocks: blocks,
        available_block_map: block_map,
        rotated_available_block_map: rotated_available_blocks,
        gathered_combinations: Default::default(),
        meaningful_combinations: Default::default(),
        solutions: Default::default(),
        combined_solutions: Default::default(),
        final_combinations: Default::default(),
        settings
    };
    println!("Using blocks:\n{}\n", storage.available_blocks.iter().map(|b| format!("ID: {}, area: {}", b.id, b.area)).join("\n"));

    generate_candidates(&mut storage);
    //filter_meaningful_combinations(&mut storage);
    filter_fitting_candidates(&mut storage);
    calculate_matches(&mut storage);
    sort_final_combinations(&mut storage);

    println!("The whole run took us {} seconds!", start.elapsed().as_secs());

    // check specific permutation with specific sizes (debug code)
    /*let x: Vec<Rectangle> = "7 173 200  12 173 323  8 124 224  11 148 249  16 123 524  13 448 174"
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


    // check all permutations for solution
    /*let s = "5 7 8 10 12 14 15 18".split('\n');

    for s1 in s {
        let mut c = BTreeSet::new();
        for id in s1.trim().split(' ') {
            c.insert(*storage.available_block_map.get(&id.parse::<RecId>().unwrap()).unwrap());
        }
        println!("Testing possible solution: {}", s1.split(',').join(" "));
        if let Some(data) = check_candidate(100, 0, &storage, &c) {
            println!("Solution is: {}", data.iter().map(|r| format!("{} {} {}", r.id, r.height, r.width)).join("  "));
            println!("Area is: {}", data.iter().map(|r| r.area).sum::<u32>());
        } else {
            println!("Not a solution!");
        }
    }*/
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
