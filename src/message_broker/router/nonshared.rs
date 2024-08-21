use std::{ptr, sync::atomic::{AtomicPtr, Ordering}};

use crate::ds::trie::{BaseLeaf, Leaf};

pub struct NonShared<T>
where
    T: Clone + PartialEq + Sync,
{
    value: AtomicPtr<T>,
}

impl<T> BaseLeaf for NonShared<T>
where
    T: Clone + PartialEq + Sync,
{
    fn delete(&self) {
        let garbage = self.value.swap(ptr::null_mut(), Ordering::Release);
        if garbage.is_null() {
            return;
        }
        unsafe { drop(Box::from_raw(garbage)) }
    }

    fn is_empty(&self) -> bool {
        self.value.load(Ordering::Acquire).is_null()
    }
}

impl<T> Leaf<T> for NonShared<T>
where
    T: Clone + PartialEq + Sync,
{
    fn get(&self) -> Option<T> {
        let vptr = self.value.load(Ordering::Acquire);
        if vptr.is_null() {
            return None;
        }
        unsafe {Some((*vptr).clone())}
    }

    fn store(&self, value: T) -> bool {
        let new_vptr = Box::into_raw(Box::new(value));
        let result = self.value.compare_exchange_weak(
            ptr::null_mut(), 
            new_vptr, 
            Ordering::Release, 
            Ordering::Relaxed
        );
        result.is_ok()
    }
}

impl<T> Default for NonShared<T> 
where
    T: Clone + PartialEq + Sync,
{
    fn default() -> Self {
        Self{ value: AtomicPtr::new(ptr::null_mut()) }
    }
}