use std::{
    convert::TryInto,
    fmt::Debug,
    sync::{Mutex, MutexGuard},
};

pub trait IntoFixedArray {
    type Item;

    fn into_fixed<const N: usize>(self) -> [Self::Item; N];
}

impl<T: Debug> IntoFixedArray for Vec<T> {
    type Item = T;

    fn into_fixed<const N: usize>(self) -> [Self::Item; N] {
        #[allow(clippy::unwrap_used)]
        self.try_into().unwrap()
    }
}

pub trait UnwrapLock<T> {
    fn unwrap_lock(&self) -> MutexGuard<'_, T>;
}

impl<M, U> UnwrapLock<U> for M
where M: AsRef<Mutex<U>>
{
    fn unwrap_lock(&self) -> MutexGuard<'_, U> {
        #[allow(clippy::unwrap_used)]
        self.as_ref().lock().unwrap()
    }
}
