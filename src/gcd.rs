use ::num::Unsigned;
use std::num;

/// Source: https://gist.github.com/victor-iyi/8a84185c1d52419b0d4915a648d5e3e1
/// Computes the greatest common divisor of two integers using Euclid's algorithm
/// (https://en.wikipedia.org/wiki/Euclidean_algorithm).
///
/// # Example
///
/// ```rust
/// assert_eq!(gcd(3, 5), 1);
///
/// assert_eq!(gcd(2 * 3 * 5 * 11 * 17, 3 * 7 * 11 * 13 * 19), 3 * 11);
/// ```
///
/// ## List of numbers.
///
/// ```rust
/// // Compute divisor one after the other.
/// let numbers: [u64; 4] = [3, 9, 21, 81];
///
/// // Method 1: Using for-loop.
/// let mut divisor: u64 = numbers[0];
/// for no in &numbers[1..] {
///     divisor = gcd(divisor, *no);
/// }
/// assert_eq!(divisor, 3);
///
/// // Method 2: Using iterator & fold.
/// let divisor: u64 = numbers.iter().fold(numbers[0], |acc, &x| gcd(acc, x));
/// assert_eq!(divisor, 3);
/// ```
pub fn gcd(mut n: u64, mut m: u64) -> u64 {
    assert!(n != 0 && m != 0);
    while m != 0 {
        if m < n {
            std::mem::swap(&mut m, &mut n);
        }
        m %= n;
    }
    n
}

#[test]
fn test_gcd() {
    // Simple greatest common divisor.
    assert_eq!(gcd(3, 5), 1);
    assert_eq!(gcd(14, 15), 1);

    // More complex greatest common divisor.
    assert_eq!(gcd(2 * 3 * 5 * 11 * 17, 3 * 7 * 11 * 13 * 19), 3 * 11);
}

#[test]
fn test_multiple_gcd() {
    // List of numbers.
    let numbers: [u64; 4] = [3, 9, 21, 81];
    // Compute divisor one after the other.
    // Method 1: Using for-loop.
    let mut divisor = numbers[0];
    for no in &numbers[1..] {
        divisor = gcd(divisor, *no);
    }
    assert_eq!(divisor, 3);

    // Method 2: Using iterator & fold.
    let divisor: u64 = numbers.iter().fold(numbers[0], |acc, &x| gcd(acc, x));
    assert_eq!(divisor, 3);
}
