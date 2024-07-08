use std::io;

pub mod linked_list;
pub mod trie;

pub trait InsertQueue<T: Default> {
    fn enqueue(&self, val: T);
}

pub trait GetFromQueue<T: Default> {
    async fn dequeue(&self) -> io::Result<T>;
}