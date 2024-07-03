pub mod linked_list;
pub mod trie;

pub trait InsertQueue<T: Default> {
    fn enqueue(&self, val: T);
}

pub trait GetFromQueue<T: Default> {
    fn dequeue(&self) -> Option<T>;
}