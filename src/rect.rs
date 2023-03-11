use std::cmp::{Ordering};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use itertools::Itertools;
use crate::ProgramStorage;

pub type RecId = i8;
pub type RecDimension = u32;
pub(crate) type Combination = BTreeSet<Rectangle>;
pub(crate) type RectCombinationStorage = HashSet<Combination>;

/// load a combination from a string
pub(crate) fn combination_from_string(storage: &ProgramStorage, line: &str) -> Combination {
    line.split(',').map(|x| *storage.available_block_map.get(&x.parse::<RecId>().unwrap()).unwrap()).collect()
}

/// convert combination to string
pub(crate) fn combination_to_string(combination: &Combination) -> String {
    combination.iter().map(|r| r.id).join(",")
}

/// load all combinations from file
pub(crate) fn combination_storage_from_file(filepath: &str, storage: &ProgramStorage) -> RectCombinationStorage {
    fs::read_to_string(filepath).unwrap_or_else(|_| "".to_owned())
        .split('\n').filter(|s| !s.is_empty())
        .map(|line| combination_from_string(storage, line))
        .collect::<RectCombinationStorage>()
}

/// store combinations to file
pub(crate) fn combination_storage_to_file(filepath: &str, combination_storage: &RectCombinationStorage) {
    fs::write(
        filepath,
        combination_storage.iter()
            .sorted_by_key(|c| -(c.iter().map(|r| r.area).sum::<u32>() as i32))
            .map(combination_to_string)
            .join("\n"),
    ).expect("Unable to write file");
}

/// get deduplication key for combination, useful for `unique_by_key`
pub(crate) fn get_unique_combination_key(combination: &Combination) -> String {
    combination.iter()
        .map(|r| r.dedup())
        .sorted()
        .map(|s| format!("{},{}", s.0, s.1))
        .join(" ")
}

/// a normal rectangle
#[derive(Debug, Clone, Copy)]
pub struct Rectangle {
    pub id: RecId,
    pub width: RecDimension,
    pub height: RecDimension,
    pub area: u32,
}

impl Rectangle {
    pub(crate) fn new(id: RecId, height: RecDimension, width: RecDimension) -> Rectangle {
        Rectangle { id, width, height, area: width as u32 * height as u32 }
    }

    fn rotate(&self) -> Rectangle {
        Rectangle::new(
            self.id,
            self.width,
            self.height,
        )
    }

    /// get all possible orientations for this rectangle\
    /// if an orientation does no fit inside the big rectangle at all, it is excluded
    pub(crate) fn get_possible_orientations(&self, big_rect: &Rectangle) -> HashSet<Rectangle> {
        let mut orientations = HashSet::new();
        if self.height <= big_rect.height && self.width <= big_rect.width {
            orientations.insert(*self);
        }
        if self.width <= big_rect.height && self.height <= big_rect.width {
            orientations.insert(self.rotate());
        }
        orientations
    }

    /// get dedup key for this rectangle
    pub(crate) fn dedup(&self) -> (RecDimension, RecDimension) {
        let x = match self.height >= 100 {
            true => (self.height / 10) * 10,
            false => self.height
        };
        let y = match self.width >= 100 {
            true => (self.width / 10) * 10,
            false => self.width
        };
        if x > y {
            return (x, y);
        }
        (y, x)
    }
}

impl PartialEq for Rectangle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Rectangle {}

impl Hash for Rectangle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.hash(state);
        self.height.hash(state);

        // transpose the dimensions and hash them again
        let mut hasher = DefaultHasher::new();
        self.height.hash(&mut hasher);
        self.width.hash(&mut hasher);
        let transposed_hash = hasher.finish();

        // combine the two hash values using bitwise XOR
        transposed_hash.hash(state);
        self.id.hash(state);
    }
}

impl Ord for Rectangle {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.id != other.id {
            self.id.cmp(&other.id)
        } else if self.width != other.width {
            self.width.cmp(&other.width)
        } else {
            self.height.cmp(&other.height)
        }
    }
}

impl PartialOrd for Rectangle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// a retangle at a specific location
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlacedRectangle {
    pub(crate) rect: Rectangle,
    pub(crate) x: RecDimension,
    pub(crate) y: RecDimension,
}

impl PlacedRectangle {
    /// get bottom left and upper right corner
    fn get_corners(&self) -> ((RecDimension, RecDimension), (RecDimension, RecDimension)) {
        (
            (self.x, self.y),
            (self.x + self.rect.width - 1, self.y + self.rect.height - 1)
        )
    }

    /// move this rectangle as close as possible to (0,0) without colliding with any other rectangle
    pub(crate) fn compact(&mut self, others: &[PlacedRectangle]) -> bool {
        // store old x and y to reset if collision
        let mut old_x_val;
        let mut old_y_val;
        // return value
        let mut moved_at_all = false;
        // guard for outer loop
        let mut moved_during_iteration = true;
        // run as long as this rectangle moved at least one unit in any direction
        while moved_during_iteration {
            moved_during_iteration = false;
            // try moving -x and -y at the same time
            loop {
                old_x_val = self.x;
                old_y_val = self.y;
                // do not overshoot
                if self.x > 0 {
                    self.x -= 1;
                }
                if self.y > 0 {
                    self.y -= 1;
                }
                if self.x == 0 && self.y == 0 {
                    break;
                }
                // if collision with any other rectangle
                // revert last change and break loop
                if others.iter().any(|r| r.check_collision(self)) {
                    self.x = old_x_val;
                    self.y = old_y_val;
                    break;
                } else {
                    moved_during_iteration = true;
                }
            }
            // ame thing but for x only
            loop {
                if self.x == 0 {
                    break;
                }
                old_x_val = self.x;
                self.x -= 1;
                if others.iter().any(|r| r.check_collision(self)) {
                    self.x = old_x_val;
                    break;
                } else {
                    moved_during_iteration = true;
                }
            }
            // ame thing but for y only
            loop {
                if self.y == 0 {
                    break;
                }
                old_y_val = self.y;
                self.y -= 1;
                if others.iter().any(|r| r.check_collision(self)) {
                    self.y = old_y_val;
                    break;
                } else {
                    moved_during_iteration = true;
                }
            }
            if moved_during_iteration {
                moved_at_all = true;
            }
        }
        moved_at_all
    }

    /// check if two rectangles collide
    pub(crate) fn check_collision(&self, other: &Self) -> bool {
        if self.rect.id == other.rect.id {
            return false;
        }
        let ((sx0, sy0), (sx1, sy1)) = self.get_corners();
        let ((ox0, oy0), (ox1, oy1)) = other.get_corners();
        ox0 <= sx0 && oy0 <= sy0 && ox1 >= sx0 && oy1 >= sy0 ||         // self: bottom left corner collides with other
            ox0 <= sx1 && oy0 <= sy0 && ox1 >= sx1 && oy1 >= sy0 ||     // self: bottom right corner collides with other
            ox0 <= sx0 && oy0 <= sy1 && ox1 >= sx0 && oy1 >= sy1 ||     // self: upper left corner collides with other
            ox0 <= sx1 && oy0 <= sy1 && ox1 >= sx1 && oy1 >= sy1 ||     // self: upper right corner collides with other´
            sx0 <= ox0 && sy0 <= oy0 && sx1 >= ox0 && sy1 >= oy0 ||     // other: bottom left corner collides with self
            sx0 <= ox1 && sy0 <= oy0 && sx1 >= ox1 && sy1 >= oy0 ||     // other: bottom right corner collides with self
            sx0 <= ox0 && sy0 <= oy1 && sx1 >= ox0 && sy1 >= oy1 ||     // other: upper left corner collides with self
            sx0 <= ox1 && sy0 <= oy1 && sx1 >= ox1 && sy1 >= oy1        // other: upper right corner collides with self
    }

    /// check if this rectangle is completely inside of the bound of the big rectangle
    pub(crate) fn check_bounds(&self, storage: &ProgramStorage) -> bool {
        self.x + self.rect.width <= storage.big_rect.width &&
            self.y + self.rect.height <= storage.big_rect.height
    }
}