use std::{fs, thread};
use std::cmp::max;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use itertools::Itertools;

use crate::{ProgramStorage, Settings};
use crate::data_configuration::RectConfiguration;

use crate::rect::{Combination, combination_from_string, combination_storage_from_file, combination_storage_to_file, combination_to_string, duplicate_combination, get_unique_combination_key, get_unique_permutation_key, PlacedRectangle, RecDimension, RecId, Rectangle, RectCombinationStorage};
use crate::rect_image::draw_image;


/// collect all candidates, which may fit inside the big rectangle
pub(crate) fn step1_generate_candiates(storage: &mut ProgramStorage) {
    let mut gathered_combinations = RectCombinationStorage::new();
    println!("GATHERING COMBINATIONS...");
    if !storage.settings.steps[0] {
        println!("SKIPPED");
        if let Some(path) = storage.settings.candidates_path {
            storage.gathered_combinations = combination_storage_from_file(path, storage);
        }
        return;
    }
    let start = Instant::now();
    let mut counter: u64 = 0;
    // for each number of rectangles, collect all combinations
    for s in storage.settings.min_rectangle_amount..=storage.settings.max_rectangle_amount {
        let mut counter2 = 0;
        for comb in storage.rect_configuration.available_blocks.iter().sorted_by_key(|r| r.id).combinations(s as usize) {
            counter += 1;
            if counter % 1000000 == 0 {
                println!("{s} {}, {} {}", counter, gathered_combinations.len(), start.elapsed().as_secs());
            }
            // if this combination may fit in the big rectangle, keep it
            if comb.iter().map(|r| r.area).sum::<u32>() <= storage.rect_configuration.big_rect.area {
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

    if let Some(path) = storage.settings.candidates_path {
        combination_storage_to_file(path, &storage.gathered_combinations);
    }

    println!("GATHERING COMBINATIONS... DONE AFTER {} seconds, found {} combinations", start.elapsed().as_secs(), storage.gathered_combinations.len());
}

/// deduplicates all equivalent combinations\
/// needs changes in filter_fitting_candidates to use this output as next step\
/// needs "reduplication" before calculate_matches!
pub(crate) fn step2_deduplication(storage: &mut ProgramStorage) {
    let candidates = storage.gathered_combinations.iter().cloned().collect::<Vec<BTreeSet<Rectangle>>>();

    println!("DEDUPLICATING {} COMBINATIONS...", candidates.len());
    storage.deduplicated_combinations = candidates.into_iter()
        .sorted_by_key(|c| c.iter().map(|r| r.id as i32).sum::<i32>())
        .unique_by(get_unique_combination_key)
        .collect();
    if let Some(path) = storage.settings.deduplicated_combinations_path {
        fs::write(
            path,
            storage.deduplicated_combinations.iter()
                .map(combination_to_string)
                .join("\n"),
        ).expect("Unable to write file");
    }
    println!("We have {} deduplicated combinations!", storage.deduplicated_combinations.len());
}

/// check for each combination, if it can be arranged inside the big rectangle
pub(crate) fn step3_filter_fitting_candidates(storage: &mut ProgramStorage) {
    println!("CALCULATING SOLUTIONS (1 layer)...");
    if !storage.settings.steps[1] {
        if let Some(path) = storage.settings.fitting_candidates_path {
            storage.solutions = combination_storage_from_file(path, storage);
        }
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
    if let Some(path) = storage.settings.fitting_candidates_path {
        combination_storage_to_file(path, &fitting_candidates);
    }
    storage.solutions = fitting_candidates;
    println!("CALCULATING SOLUTIONS (1 layer)... DONE AFTER {} seconds, found {} solutions", start.elapsed().as_secs(), storage.solutions.len());
}

/// this function is the main function, which will be run by the threads of filter_fitting_candidates
pub(crate) fn step3_thread_procedure(number: u8,
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
            println!("Thread {number} working counter {counter} with data {}. I am alive for {} seconds.", data.iter().map(|r| r.id).join(","), thread_start.elapsed().as_secs());
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

/// check if a combination fits inside the big rect
pub(crate) fn step3_check_candidate(number: u8, counter: i32,
                                    storage: &ProgramStorage,
                                    candidate: &BTreeSet<Rectangle>,
) -> Option<Vec<PlacedRectangle>> {
    let c_start = Instant::now();
    // because the small rectangles can be rotated, we need to check each combination of each rotation for the input
    for product in candidate.iter().map(|r| storage.rect_configuration.rotated_available_block_map.get(&r.id).unwrap()).multi_cartesian_product() {
        // because I have no better idea, just check each permutation of each combination individually
        for per in product.iter().cloned().permutations(product.len()).unique_by(get_unique_permutation_key) {
            if let Some(sol) = step3_check_permutation(storage, per) {
                if counter % 100 == 0 && number > 0 {
                    println!("Thread {number} worked {counter} in {} seconds (success)", c_start.elapsed().as_secs());
                }
                println!("SOLUTION_DEBUG {}", sol.iter().map(|r| format!("{} {} {} {} {}", r.rect.id, r.rect.height, r.rect.width, r.x, r.y)).join("  "));
                return Some(sol);
            }
        }
    }
    if counter % 100 == 0 && number > 0 {
        println!("Thread {number} worked {counter} in {} seconds (fail)", c_start.elapsed().as_secs());
    }
    None
}

/// check if a specific set of rectangles fits inside the big rect, without rotating or rearranging them
pub(crate) fn step3_check_permutation(storage: &ProgramStorage, candidate: Vec<&Rectangle>) -> Option<Vec<PlacedRectangle>> {
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
    // make everything more cache efficient (width and height with spaces put in)
    let big_width = storage.rect_configuration.big_rect.width;
    // store the height of each column
    let x_size = big_width + storage.settings.max_rectangle_amount as RecDimension * distance;
    let mut taken_with_spaces = vec![0 as RecDimension; x_size as usize];
    let mut taken = vec![0 as RecDimension; x_size as usize];
    for rect in candidate {
        // if the new rect will collide with the border of the big rect
        if x_normal + rect.width > big_width {
            // go back to x 0 for next row
            x_normal = 0;
            x_with_spaces = 0;
            if taken.iter().max().unwrap() > &storage.rect_configuration.big_rect.height {
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
    if taken.iter().max().unwrap() > &storage.rect_configuration.big_rect.height {
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
                break;
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
pub(crate) fn step4_calculate_matches(storage: &mut ProgramStorage) {
    if !storage.settings.steps[2] {
        if let Some(path) = storage.settings.solutions_filepath {
            storage.combined_solutions = fs::read_to_string(path).unwrap_or_else(|_| "".to_owned())
                .split('\n')
                .filter(|line| !line.is_empty())
                .map(|line|
                    line.split(' ')
                        .map(|c| combination_from_string(storage, c)).collect()
                ).collect::<HashSet<BTreeSet<Combination>>>();
        }
        println!("CALCULATING COMBINED SOLUTIONS (3 layers)...\nSKIPPED");
        return;
    }
    // sort the candidates by area
    let candidates = storage.solutions.iter()
        .sorted_by_key(|c| -(c.iter().map(|r| r.area).sum::<u32>() as i32))
        .collect::<Vec<&BTreeSet<Rectangle>>>();
    let start = Instant::now();

    println!("CALCULATING COMBINED SOLUTIONS (3 layers) with {} candidates...", candidates.len());
    for i in 0..candidates.len() {
        println!("i {} solutions {}", i, storage.combined_solutions.len());
        let is = *candidates.get(i).unwrap();
        for j in i + 1..candidates.len() {
            let js = *candidates.get(j).unwrap();
            for k in j + 1..candidates.len() {
                let ks = *candidates.get(k).unwrap();
                let amount = is.len() + js.len() + ks.len();
                if amount >= storage.rect_configuration.available_blocks.len() {
                    let vec: Vec<&Rectangle> = is.iter()
                        .map(|r| storage.rect_configuration.duplication_map.get(&r.id).unwrap().first().unwrap())
                        .chain(
                            js.iter().map(|r| storage.rect_configuration.duplication_map.get(&r.id).unwrap().first().unwrap())
                        )
                        .chain(
                            ks.iter().map(|r| storage.rect_configuration.duplication_map.get(&r.id).unwrap().first().unwrap())
                        ).collect();
                    if storage.rect_configuration.duplication_map.iter().all(
                        |(id, rects)| vec.iter().filter(|r| &r.id == id).count() <= rects.len()
                    ) {
                        println!("{} {} {}", i, j, k);
                        let mut found = false;
                        for is2 in duplicate_combination(&mut is.clone(), &storage.rect_configuration.duplication_map, &BTreeSet::new()) {
                            if found {
                                break;
                            }
                            for js2 in duplicate_combination(&mut js.clone(), &storage.rect_configuration.duplication_map, &is2) {
                                if found {
                                    break;
                                }
                                let union = &is2 | &js2;
                                if union.len() >= is2.len() + js2.len() {
                                    for ks2 in duplicate_combination(&mut ks.clone(), &storage.rect_configuration.duplication_map, &union) {
                                        if (&union | &ks2).len() >= storage.rect_configuration.available_blocks.len() {
                                            let mut solution = BTreeSet::new();
                                            solution.insert(is2.to_owned().clone());
                                            solution.insert(js2.to_owned().clone());
                                            solution.insert(ks2.to_owned().clone());
                                            println!("Found {}", solution.iter().map(combination_to_string).join(" "));
                                            storage.combined_solutions.insert(solution);
                                            found = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(path) = storage.settings.solutions_filepath {
        fs::write(
            path,
            storage.combined_solutions.iter()
                .map(|l| l.iter().map(combination_to_string).join(" "))
                .join("\n"),
        ).expect("Unable to write file");
    }
    println!("CALCULATING COMBINED SOLUTIONS (3 layers)... DONE AFTER {} seconds, found {} combined solutions", start.elapsed().as_secs(), storage.combined_solutions.len());
}

/// take the possible solutions for three layers and split them in single layer combinations\
/// sort by how often each combination appears within the possible solutions
pub(crate) fn step5_sort_final_combinations(storage: &mut ProgramStorage) {
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

    if let Some(path) = storage.settings.final_combinations_path {
        fs::write(path,
                  final_combinations.iter().map(|c| c.iter().map(|r| r.id).join(" ")).join("\n"),
        ).expect("Unable to write file");
    }
    storage.final_combinations = final_combinations;
    println!("CALCULATING FINAL COMBINATIONS... DONE AFTER {} seconds, found {} final combinations", start.elapsed().as_secs(), storage.final_combinations.len());
}

#[test]
fn test_single_combination() {
    let rects = RectConfiguration::new(
        Rectangle::new(-1, 4, 10),
        vec![
            Rectangle::new(1, 2, 2),
            Rectangle::new(2, 1, 2),
            Rectangle::new(3, 2, 3),
            Rectangle::new(4, 1, 3),
            Rectangle::new(5, 2, 4),
            Rectangle::new(6, 2, 2),
            Rectangle::new(7, 3, 2),
            Rectangle::new(8, 1, 5),
            Rectangle::new(9, 1, 2),
        ],
    );

    let settings = Settings {
        thread_count: 14,
        steps: [false, false, false, false],
        distance_between_rectangles: 10,
        min_rectangle_amount: 9,
        max_rectangle_amount: 9,
        ..Default::default()
    };
    let storage = ProgramStorage::new(&rects, settings);

    // check all permutations for solution
    let s = "1,2,3,4,5,6,7,8,9".split('\n');

    for s1 in s {
        let mut c = BTreeSet::new();
        for id in s1.trim().split(',') {
            c.insert(*(storage.rect_configuration.available_block_map.get(&id.trim().parse::<RecId>().unwrap()).unwrap()));
        }
        assert!(step3_check_candidate(100, 0, &storage, &c).is_some());
    }
}

#[test]
fn test_single_combination2() {
    let rects = RectConfiguration::new(
        Rectangle::new(-1, 9, 27),
        vec![
            Rectangle::new(1, 1, 1),
            Rectangle::new(2, 6, 1),
            Rectangle::new(3, 2, 8),
            Rectangle::new(4, 2, 6),
            Rectangle::new(5, 2, 8),
            Rectangle::new(6, 3, 5),
            Rectangle::new(7, 2, 2),
            Rectangle::new(8, 3, 5),
            Rectangle::new(9, 2, 5),
            Rectangle::new(10, 7, 2),
            Rectangle::new(11, 1, 6),
            Rectangle::new(12, 1, 11),
            Rectangle::new(13, 1, 1),
            Rectangle::new(14, 1, 2),
            Rectangle::new(15, 4, 5),
            Rectangle::new(16, 2, 6),
            Rectangle::new(17, 3, 2),
            Rectangle::new(18, 2, 3),
            Rectangle::new(19, 2, 3),
            Rectangle::new(20, 1, 5),
            Rectangle::new(21, 2, 7),
            Rectangle::new(22, 2, 5),
            Rectangle::new(23, 3, 4),
            Rectangle::new(24, 5, 5),
        ],
    );

    let settings = Settings {
        thread_count: 14,
        steps: [false, false, false, false],
        distance_between_rectangles: 10,
        min_rectangle_amount: 9,
        max_rectangle_amount: 24,
        ..Default::default()
    };
    let storage = ProgramStorage::new(&rects, settings);

    // check all permutations for solution
    let s = "1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24".split('\n');

    for s1 in s {
        let mut c = BTreeSet::new();
        for id in s1.trim().split(',') {
            c.insert(*(storage.rect_configuration.available_block_map.get(&id.trim().parse::<RecId>().unwrap()).unwrap()));
        }
        if let Some(data) = step3_check_candidate(100, 0, &storage, &c) {
            println!("Solution is: {}", data.iter().map(|r| format!("{} {} {}", r.rect.id, r.rect.height, r.rect.width)).join("  "));
            println!("Area is: {}", data.iter().map(|r| r.rect.area).sum::<u32>());
            draw_image("./out.png", &storage, &data);
        } else {
            println!("Not a solution!");
        }
    }
}

#[test]
fn test_single_combination3() {
    let rects = crate::data_configuration::mm10_rects_floor();

    let settings = Settings {
        thread_count: 14,
        steps: [false, false, false, false],
        distance_between_rectangles: 10,
        min_rectangle_amount: 9,
        max_rectangle_amount: 24,
        ..Default::default()
    };
    let storage = ProgramStorage::new(&rects, settings);

    // check all permutations for solution
    let s = "4,5,8,9,11,14,15,18".split('\n');

    for s1 in s {
        let mut c = BTreeSet::new();
        for id in s1.trim().split(',') {
            c.insert(*(storage.rect_configuration.available_block_map.get(&id.trim().parse::<RecId>().unwrap()).unwrap()));
        }
        let start = Instant::now();
        for _ in 0..10 {
            if let Some(data) = step3_check_candidate(100, 0, &storage, &c) {
                println!("Solution is: {}", data.iter().map(|r| format!("{} {} {}", r.rect.id, r.rect.height, r.rect.width)).join("  "));
            } else {
                println!("Not a solution!");
            }
        }
        println!("{:?}", start.elapsed().as_millis());
    }
}

#[test]
fn test_single_permutation() {
    let rects = RectConfiguration::new(
        Rectangle::new(-1, 4, 10),
        vec![
            Rectangle::new(1, 2, 2),
            Rectangle::new(2, 1, 2),
            Rectangle::new(3, 2, 3),
            Rectangle::new(4, 1, 3),
            Rectangle::new(5, 2, 4),
            Rectangle::new(6, 2, 2),
            Rectangle::new(7, 3, 2),
            Rectangle::new(8, 1, 5),
            Rectangle::new(9, 1, 2),
        ],
    );

    let settings = Settings {
        thread_count: 14,
        steps: [false, false, false, false],
        distance_between_rectangles: 10,
        min_rectangle_amount: 9,
        max_rectangle_amount: 9,
        ..Default::default()
    };
    let storage = ProgramStorage::new(&rects, settings);

    // check all permutations for solution
    let s = "2,9,5,4,7,1,8,6,3".split('\n');

    for s1 in s {
        let mut c = vec![];
        for id in s1.trim().split(',') {
            c.push(storage.rect_configuration.available_block_map.get(&id.trim().parse::<RecId>().unwrap()).unwrap());
        }
        assert!(step3_check_permutation(&storage, c).is_none());
    }
}

#[test]
fn test_multiple_layers() {
    let start = Instant::now();
    let rects = RectConfiguration::new(
        Rectangle::new(-1, 4, 8),
        vec![
            Rectangle::new(1, 1, 2),
            Rectangle::new(2, 1, 2),
            Rectangle::new(3, 3, 4),
            Rectangle::new(4, 2, 2),
            Rectangle::new(5, 2, 3),
            Rectangle::new(6, 2, 3),
            Rectangle::new(7, 2, 3),
            Rectangle::new(8, 2, 3),
            Rectangle::new(9, 1, 1),
            Rectangle::new(10, 1, 4),
            Rectangle::new(11, 1, 4),
            Rectangle::new(12, 3, 3),
            Rectangle::new(13, 2, 3),
            Rectangle::new(14, 2, 2),
            Rectangle::new(15, 2, 3),
            Rectangle::new(16, 2, 4),
            Rectangle::new(17, 2, 2),
            Rectangle::new(18, 1, 2),
            Rectangle::new(19, 1, 2),
            Rectangle::new(20, 1, 2),
        ],
    );

    let settings = Settings {
        thread_count: 14,
        steps: [false, false, true, true],
        distance_between_rectangles: 10,
        min_rectangle_amount: 5,
        max_rectangle_amount: 9,
        min_solution_area: 30,
        candidates_path: Some("./tests/candidates.txt"),
        deduplicated_combinations_path: Some("./tests/dedup_comb.txt"),
        fitting_candidates_path: Some("./tests/fitting_cand.txt"),
        solutions_filepath: Some("./tests/solutions.txt"),
        final_combinations_path: Some("./tests/final_solutions.txt"),
        ..Default::default()
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
fn test_multiple_layers2() {
    let start = Instant::now();
    let rects = RectConfiguration::new(
        Rectangle::new(-1, 4, 8),
        vec![
            Rectangle::new(1, 1, 2),
            Rectangle::new(2, 1, 2),
            Rectangle::new(3, 3, 4),
            Rectangle::new(4, 2, 2),
            Rectangle::new(5, 2, 3),
            Rectangle::new(6, 2, 3),
            Rectangle::new(7, 2, 3),
            Rectangle::new(8, 2, 3),
            Rectangle::new(9, 1, 1),
            Rectangle::new(10, 1, 4),
            Rectangle::new(11, 1, 4),
            Rectangle::new(12, 3, 3),
            Rectangle::new(13, 2, 3),
            Rectangle::new(14, 2, 2),
            Rectangle::new(15, 2, 3),
            Rectangle::new(16, 2, 4),
            Rectangle::new(17, 2, 2),
            Rectangle::new(18, 1, 2),
            Rectangle::new(19, 1, 2),
            Rectangle::new(20, 1, 2),
        ],
    );

    let settings = Settings {
        thread_count: 14,
        steps: [false, false, false, false],
        distance_between_rectangles: 10,
        min_rectangle_amount: 5,
        max_rectangle_amount: 9,
        min_solution_area: 30,
        ..Default::default()
    };
    let mut storage = ProgramStorage::new(&rects, settings);

    // check all permutations for solution
    let s = "1,2,4,6,7,11,14,18,19
    3,5,9,10,12
    8,13,15,16,17,20".split('\n');

    for (i, s1) in s.enumerate() {
        let mut c = BTreeSet::new();
        for id in s1.trim().split(',') {
            c.insert(*(storage.rect_configuration.available_block_map.get(&id.trim().parse::<RecId>().unwrap()).unwrap()));
        }
        println!("Testing possible solution: {}", s1.split(',').join(" "));
        if let Some(data) = step3_check_candidate(100, 0, &storage, &c) {
            println!("Solution is: {}", data.iter().map(|r| format!("{} {} {}", r.rect.id, r.rect.height, r.rect.width)).join("  "));
            println!("Area is: {}", data.iter().map(|r| r.rect.area).sum::<u32>());
            draw_image(&format!("./out{i}.png"), &storage, &data);
        } else {
            println!("Not a solution!");
        }
    }
}

/*
let rects = RectConfiguration::new(
        Rectangle::new(-1, 4, 10),
        vec![
            Rectangle::new(1, 2, 2),
            Rectangle::new(2, 1, 2),
            Rectangle::new(3, 2, 3),
            Rectangle::new(4, 1, 3),
            Rectangle::new(5, 2, 4),
            Rectangle::new(6, 2, 2),
            Rectangle::new(7, 3, 2),
            Rectangle::new(8, 1, 5),
            Rectangle::new(9, 1, 2),
        ],
    );

    let settings = Settings {
        thread_count: 14,
        steps: [false, false, false, false],
        distance_between_rectangles: 10,
        min_rectangle_amount: 9,
        max_rectangle_amount: 9,
        ..Default::default()
    };
    let storage = ProgramStorage::new(&rects, settings);

    // check all permutations for solution
    let s = "1,2,3,4,5,6,7,8,9".split('\n');

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
    let mut x: Vec<Combination> = redup_comb_iter(storage.solutions.iter().filter(
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
    }*/
