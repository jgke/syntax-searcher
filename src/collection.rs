//! vec!-like generic collection macro.

/// Generate any collection.
///
/// ```
/// # #[macro_use] extern crate syns;
/// # fn main() {
/// use std::collections::{HashMap, HashSet};
/// let set: HashSet<i32> = [1, 2, 3].iter().copied().collect();
/// let macro_set: HashSet<i32> = collection!(1, 2, 3);
/// assert_eq!(macro_set, set);
///
/// let map: HashMap<i32, i32> = [(1, 2), (3, 3), (2, 4)].iter().copied().collect();
/// let macro_map: HashMap<i32, i32> = collection!(1 => 2, 2 => 4, 3 => 3);
/// assert_eq!(macro_map, map);
/// # }
/// ```
#[macro_export]
macro_rules! collection {
    // map-like
    ($($k:expr => $v:expr),* $(,)?) => {{
        use std::iter::{Iterator, IntoIterator};
        Iterator::collect(vec![$(($k.into(), $v.into()),)*].into_iter())
    }};
    // set-like
    ($($v:expr),* $(,)?) => {{
        use std::iter::{Iterator, IntoIterator};
        Iterator::collect(vec![$($v.into(),)*].into_iter())
    }};
}
