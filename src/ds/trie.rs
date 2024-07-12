use std::{collections::HashMap, ptr, sync::atomic::{AtomicPtr, Ordering}};

type ATrieChild<T> = AtomicPtr<Child<T>>;

struct Child<T>
    where T: Clone + PartialEq
{
    child: HashMap<String, ATrieChild<T>>,
    subscriber: AtomicPtr<T>
}

impl<T> Child<T> 
    where T: Clone + PartialEq
{
    fn new() -> Self {
        Self {
            child: HashMap::new(), 
            subscriber: AtomicPtr::default()
        }
    }

    fn create_branch(&mut self, topic: String) {
        let p = Box::into_raw(Box::new(Child::new()));
        self.child.insert(
            topic,
            AtomicPtr::new(p)
        );
    }

    fn set_subscriber(&self, subscriber: T) -> bool {
        let v_ptr = Box::into_raw(Box::new(subscriber));
        let exw = self.subscriber.compare_exchange_weak(
            ptr::null_mut(),
            v_ptr, 
            Ordering::AcqRel,
            Ordering::Relaxed
        );
        exw.is_ok()
    }

    fn delete_subscriber(&self, subscriber: T) -> bool {
        let cur_psub = self.subscriber.load(Ordering::Acquire);
        if cur_psub.is_null() {
            return false;
        }

        unsafe {
            if (*cur_psub).ne(&subscriber) {
                return false;
            }

            let old_psub = self.subscriber.swap(ptr::null_mut(), Ordering::Relaxed);
            drop(Box::from_raw(old_psub))
        };
        true

    }

    fn get_subscriber(&self) -> Option<T> {
        unsafe {
            let sub = self.subscriber.load(Ordering::Acquire);
            if sub.is_null() {
                return None;
            }

            Some((*sub).clone())
        }
    }
}

pub struct Trie<T> 
    where T: Clone + PartialEq
{
    root: ATrieChild<T>
}

impl<T> Trie<T> 
    where T: Clone + PartialEq
{
    pub fn new() -> Self {
        let pchild = Box::into_raw(Box::new(Child::<T>::new()));
        Self { root: AtomicPtr::new(pchild) }
    }
    
    pub fn insert(&self, topic: &str, value: T) -> bool {
        let parts: Vec<&str> = topic.split('/').collect();
        let mut cur = &self.root;
        let mut i = 0;
        while i < parts.len() {
            let part = parts[i];
            
            let next = cur.load(Ordering::Acquire);
            if next.is_null() {
                let p = Box::into_raw(Box::new(Child::<T>::new()));
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
            
            i = i+1;
        }

        unsafe {(*cur.load(Ordering::Acquire)).set_subscriber(value)}
    }

    fn get<R>(&self, topic: &str, f: impl FnOnce(&Child<T>) -> R) -> Option<R> {
        let parts: Vec<&str> = topic.split('/').collect();
        let mut cur = &self.root;

        for part in parts.into_iter() {
            let branch_ptr = cur.load(Ordering::Acquire);
            if branch_ptr.is_null() {
                return None;
            }

            let get_opt = unsafe {(*branch_ptr).child.get(part)};

            cur = match get_opt {
                Some(branch) => branch,
                None => return None
            };
        }
        
        let child = unsafe {&*cur.load(Ordering::Acquire)};
        Some(f(child))
    }

    pub fn get_val(&self, topic: &str) -> Option<T> {
        self.get(topic, |child| {
            child.get_subscriber()
        })?
    }

    #[allow(dead_code)]
    pub fn remove(&self, topic: &str, value: T) -> Option<bool> {
        self.get(topic, |child| {
            child.delete_subscriber(value)
        })
    }

    #[allow(dead_code)]
    pub fn clean_branch(&self) {
        unsafe {
            Self::dfs_empty_and_remove(self.root.load(Ordering::SeqCst));
        }
    }
}

impl<T> Trie<T> 
    where T: Clone + PartialEq
{
    #[allow(dead_code)]
    unsafe fn dfs_empty_and_remove(node_ptr: *mut Child<T>) -> bool {
        if node_ptr.is_null() {
            return true;
        }

        let node = &mut *node_ptr;
        let mut empty_keys = Vec::new();

        for (key, child_ptr) in node.child.iter() {
            if Self::dfs_empty_and_remove(child_ptr.load(Ordering::SeqCst)) {
                empty_keys.push(key.clone());
            }
        }

        empty_keys.iter().for_each(|elm| {
            node.child.remove(elm);
        });

        node.child.is_empty() && node.subscriber.load(Ordering::Acquire).is_null()
    }

    unsafe fn dfs_drop(node_ptr: *mut Child<T>) {
        if node_ptr.is_null() {
            return;
        }

        let node = &mut *node_ptr;
        let mut empty_keys = Vec::new();

        for (key, child_ptr) in node.child.iter() {
            Self::dfs_drop(child_ptr.load(Ordering::SeqCst));
            empty_keys.push(key.clone());
            
        }

        if let Some(sub) = node.get_subscriber() {
            node.delete_subscriber(sub);
        }
        
        empty_keys.iter().for_each(|elm| {
            println!("{}", elm);
            node.child.remove(elm);
        });
    }
}

impl<T> Drop for Trie<T> 
where T: Clone + PartialEq
{
    fn drop(&mut self) {
        unsafe {
            let inner = self.root.load(Ordering::SeqCst);
            if inner.is_null() {
                return;
            }

            Self::dfs_drop(inner);
        }
    }
}


#[cfg(test)]
mod tests {
    use crate::{ds::trie::Trie, message_broker::client::clobj::ClientID, protocol::v5::{subscribe::Subscribe, ServiceLevel}};

    struct TrieTest {
        clid: ClientID,
        value: Vec<Subscribe>
    }

    #[test]
    fn subscriber() {
        let pref_tree: Trie<ClientID> = Trie::new();

        let test_cases = vec![
            TrieTest {
                clid: ClientID::new("clid1".to_string()),
                value: vec![Subscribe {
                    topic: "home/bathroom/lamp".to_string(),
                    max_qos: ServiceLevel::QoS1,
                }, Subscribe {
                    topic: "home/kitchen".to_string(),
                    max_qos: ServiceLevel::QoS2,
                }],
            },
            TrieTest {
                clid: ClientID::new("clid2".to_string()),
                value: vec![Subscribe {
                    topic: "home/kitchen/topek".to_string(),
                    max_qos: ServiceLevel::QoS2,
                }, Subscribe {
                    topic: "home/livingroom/fan".to_string(),
                    max_qos: ServiceLevel::QoS2,
                }],
            },
        ];
        
        for test in test_cases.iter() {
            for sub in test.value.iter() {
                let ins = pref_tree.insert(&sub.topic, test.clid.clone());
                assert!(ins)
            }

            for sub in test.value.iter() {
                let got = pref_tree.get_val(&sub.topic).unwrap();
                assert_eq!(got, test.clid)
            }
        }

        for test in test_cases {
            for sub in test.value.iter() {
                let del = pref_tree.remove(&sub.topic, test.clid.clone()).unwrap();
                assert!(del)
            }

            for sub in test.value.iter() {
                let got = pref_tree.get_val(&sub.topic);
                assert!(got.is_none())
            }
        }
    }

    #[test]
    fn cleanup() {
        let pref_tree: Trie<ClientID> = Trie::new();

        let test_cases = vec![
            TrieTest {
                clid: ClientID::new("clid1".to_string()),
                value: vec![Subscribe {
                    topic: "home/bathroom/lamp".to_string(),
                    max_qos: ServiceLevel::QoS1,
                }, Subscribe {
                    topic: "home/kitchen".to_string(),
                    max_qos: ServiceLevel::QoS2,
                }],
            },
            TrieTest {
                clid: ClientID::new("clid2".to_string()),
                value: vec![Subscribe {
                    topic: "home/kitchen/topek".to_string(),
                    max_qos: ServiceLevel::QoS2,
                }, Subscribe {
                    topic: "home/livingroom/fan".to_string(),
                    max_qos: ServiceLevel::QoS2,
                }],
            },
        ];

        for test in test_cases.iter() {
            for sub in test.value.iter() {
                let ins = pref_tree.insert(&sub.topic, test.clid.clone());
                assert!(ins)
            }
        }

        let target = &test_cases[1];
        pref_tree.remove(&target.value[1].topic, target.clid.clone());
        let got = pref_tree.get_val(&target.value[1].topic);
        assert!(got.is_none());
    }
}