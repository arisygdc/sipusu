use std::{collections::HashMap, ptr, sync::atomic::{AtomicPtr, Ordering}};
use tokio::sync::RwLock;

// TODO: wilcard
pub struct Trie<T> 
    where T: Eq + Clone
{
    root: AtmcNode<T>
}

type AtmcNode<T> = AtomicPtr<TrieNode<T>>;
struct TrieNode<T> 
    where T: Eq + Clone
{
    child: HashMap<String, AtmcNode<T>>,
    subscribers: RwLock<Vec<T>>
}

impl<T> TrieNode<T> 
    where T: Eq + Clone
{
    fn new() -> Self {
        Self {
            child: HashMap::new(), 
            subscribers: RwLock::new(Vec::new()) 
        }
    }

    async fn add_subscriber(&self, value: T) {
        let mut add_sub = self.subscribers.write().await;
        add_sub.push(value);
    }

    async fn get_subscriber(&self) -> Vec<T>{
        let rsub = self.subscribers.read().await;
        rsub.to_vec()
    }
}

impl<T> Trie<T> 
    where T: Eq + Clone
{
    pub fn new() -> Self {
        Trie {
            root: AtomicPtr::new(ptr::null_mut()),
        }
    }

    fn empty_atmcnode(cur: &AtomicPtr<TrieNode<T>>) -> Result<*mut TrieNode<T>, *mut TrieNode<T>> {
        let new_node: *mut TrieNode<T> = Box::into_raw(Box::new(TrieNode::new()));
        cur.compare_exchange(
            ptr::null_mut(), 
            new_node, 
            Ordering::Release,
            Ordering::Relaxed
        )
    }

    fn with_traverse<R>(&self, topic: &str, f: impl FnOnce(&AtomicPtr<TrieNode<T>>) -> R) -> R {
        let parts: Vec<&str> = topic.split('/').collect();
        let mut cur = &self.root;
        let mut i = 0;
        while i < parts.len() {
            let part = parts[i];
            if cur.load(Ordering::Acquire).is_null() {
                Self::empty_atmcnode(cur).unwrap();
                continue;
            }

            let get_opt = unsafe {
                (*cur.load(Ordering::Acquire)).child.get(part)
            };

            cur = match get_opt {
                None => unsafe {
                    let new_null = AtomicPtr::new(ptr::null_mut());
                    (*cur.load(Ordering::Acquire)).child.insert(
                        part.to_owned(),
                        new_null
                    );
                    continue;
                }, Some(branch) => branch
            };
            
            i = i+1;
        }

        f(cur)
    } 

    pub async fn insert(&self, topic: &str, value: T) {
        self.with_traverse(topic, |p| {
            if p.load(Ordering::Acquire).is_null() {
                Self::empty_atmcnode(p).unwrap();
            }
    
            let val = p.load(Ordering::Acquire);
            unsafe {(*val).add_subscriber(value)}
        }).await;
    }

    pub async fn get(&self, topic: &str) -> Option<Vec<T>> {
        let m = self.with_traverse(topic, |p| {
            if p.load(Ordering::Acquire).is_null() {
                return None;
            }

            Some(unsafe {(*p.load(Ordering::Acquire)).get_subscriber()})
        })?.await;

        if m.len() == 0 {
            return None;
        }

        Some(m)
    }
}

#[cfg(test)]
mod tests {
    use super::Trie;

    #[tokio::test]
    async fn insert() {
        let pref_tree: Trie<u32> = Trie::new();
        struct TrieTest {
            topic: String,
            value: u32
        }

        let test_table = vec![
            TrieTest{ topic: String::from("topic/topek/tokek"), value: 66},
            TrieTest{ topic: String::from("topic/topek/tokek"), value: 67},
            TrieTest{ topic: String::from("topic/topek/toket"), value: 98},
            TrieTest{ topic: String::from("topek/tokek/topic"), value: 78},
            TrieTest{ topic: String::from("tokek/topic/topek"), value: 99}
        ];

        for test in test_table {
            pref_tree.insert(&test.topic, test.value).await;
            let get = pref_tree.get(&test.topic).await;
            if let Some(v) = get {
                println!("{:?}", v);
                assert!(v.contains(&test.value))
            } else {
                panic!()
            }
        }
    }
}