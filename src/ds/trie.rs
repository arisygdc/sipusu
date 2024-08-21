use std::{collections::HashMap, sync::atomic::{AtomicPtr, Ordering}};

type ATrieChild<T, L> = AtomicPtr<Child<T, L>>;


pub trait BaseLeaf: Default
{
    fn is_empty(&self) -> bool;
    /// set leaf to empty leaf
    fn delete(&self);
}

pub trait Leaf<T>
where
    T: Clone + PartialEq + Sync,
{
    fn store(&self, value: T) -> bool;
    fn get(&self) -> Option<T>;
}

pub trait LeafCollection<T>: Default
where
    T: Clone + PartialEq + Sync,
{
    fn insert(&self, value: T);
    fn get_collection(&self) -> Vec<T>;
    /// remove specific value
    /// return removed value
    fn remove(&self, value: T) -> Option<T>;
}

struct Child<T, L>
where
    T: Clone + PartialEq + Sync,
    L: Default,
{
    child: HashMap<String, ATrieChild<T, L>>,
    leaf: L,
}

impl<T, L> Default for Child<T, L>
where
    T: Clone + PartialEq + Sync,
    L: Default,
{
    fn default() -> Self {
        Child {
            child: HashMap::new(),
            leaf: L::default(),
        }
    }
}

impl<T, L> Child<T, L>
where
    T: Clone + PartialEq + Sync,
    L: Default,
{
    fn create_branch(&mut self, topic: String) {
        let p = Box::into_raw(Box::default());
        self.child.insert(
            topic,
            AtomicPtr::new(p)
        );
    }
}



pub struct Trie<T, L>
where
    T: Clone + PartialEq + Sync,
    L: BaseLeaf,
{
    root: ATrieChild<T, L>,
}

impl<T, L> Trie<T, L>
where
    T: Clone + PartialEq + Sync,
    L: BaseLeaf
{
    fn traverse(&self, prefix: &str) -> &L {
        let parts: Vec<&str> = prefix.split('/').collect();
        let mut cur = &self.root;
        let mut i = 0;
        while i < parts.len() {
            let part = parts[i];
            
            let next = cur.load(Ordering::Acquire);
            if next.is_null() {
                let p = Box::into_raw(Box::default());
                cur.store(p, Ordering::Relaxed);
                continue;
            }

            let branch_map = unsafe {&mut (*next).child};

            cur = match branch_map.get(part) {
                Some(branch) => branch,
                None => unsafe {
                    (*next).create_branch(part.to_string());
                    continue;
                }
            };
            
            i += 1;
        }

        unsafe { &(*cur.load(Ordering::Acquire)).leaf }
    }

    #[allow(dead_code)]
    pub fn clean_branch(&self) {
        unsafe {
            Self::dfs_empty_and_remove(self.root.load(Ordering::SeqCst));
        }
    }

    #[allow(dead_code)]
    unsafe fn dfs_empty_and_remove(node_ptr: *mut Child<T, L>) -> bool {
        if node_ptr.is_null() {
            return true;
        }

        let node = &mut *node_ptr;
        let mut empty_pref = Vec::new();

        for (key, child_ptr) in node.child.iter() {
            if Self::dfs_empty_and_remove(child_ptr.load(Ordering::SeqCst)) {
                empty_pref.push(key.clone());
            }
        }

        empty_pref.iter().for_each(|elm| {
            node.child.remove(elm);
        });

        node.child.is_empty() && node.leaf.is_empty()
    }

    unsafe fn truncate(node_ptr: *mut Child<T, L>) {
        // DFS (Depth First Search)
        if node_ptr.is_null() {
            return;
        }

        let node = &mut *node_ptr;
        let mut empty_keys = Vec::new();

        for (key, child_ptr) in node.child.iter() {
            Self::truncate(child_ptr.load(Ordering::SeqCst));
            empty_keys.push(key.clone());
        }

        node.leaf.delete();

        empty_keys.iter().for_each(|elm| {
            println!("{}", elm);
            node.child.remove(elm);
        });
    }
}

impl<T, L> Trie<T, L>
where
    T: Clone + PartialEq + Sync,
    L: BaseLeaf + Leaf<T>
{
    pub fn new_single() -> Self {
        let child = Box::into_raw(Box::default());
        Trie {
            root: AtomicPtr::new(child),
        }
    }

    // store can fail when value already exists
    pub fn store(&self, prefix: &str, val: T) -> bool {
        self.traverse(prefix).store(val)
    }

    pub fn get(&self, prefix: &str) -> Option<T> {
        self.traverse(prefix).get()
    }
}

impl<T, L> Trie<T, L>
where
    T: Clone + PartialEq + Sync,
    L: BaseLeaf + LeafCollection<T>
{
    pub fn new_multi() -> Self {
        let child = Box::into_raw(Box::default());
        Trie {
            root: AtomicPtr::new(child),
        }
    }
}