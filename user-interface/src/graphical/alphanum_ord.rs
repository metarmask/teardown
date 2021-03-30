use std::cmp::Ordering;

pub(crate) struct AlphanumericOrd<T>(pub T);

impl<T: AsRef<str>> Ord for AlphanumericOrd<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        alphanumeric_sort::compare_str(&self.0, &other.0)
    }
}

impl<T> Eq for AlphanumericOrd<T> where Self: Ord {}
impl<T> PartialEq for AlphanumericOrd<T> where Self: Ord {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<T> PartialOrd for AlphanumericOrd<T> where Self: Ord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
