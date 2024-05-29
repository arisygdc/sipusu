#![allow(dead_code)] 
use std::{cell::RefCell, io::{self}};

use bytes::{BufMut, BytesMut};
use tokio::{fs::{File, OpenOptions}, io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt}};

const END_RECORD: u8 = 0x1E;
const GROUP_SEPARATOR: u8 = 0x1D;
const VALUE_SIGN: u8 = 0x2;

pub struct Authenticator {
    storage: RefCell<File>
}

impl Authenticator {
    pub async fn new() -> Result<Self, io::Error> {
        let storage = RefCell::new(Self::prepare_storage().await?);
        Ok(Self { storage })
    }

    #[inline]
    async fn prepare_storage() -> io::Result<File> {
        let path = "./wow3";
        let mut storage_opt = OpenOptions::new();
        let open_option = storage_opt
            .read(true)
            .write(true)
            .append(true);

        let try_open = open_option
            .open(path)
            .await;

        match try_open {
            Ok(f) => Ok(f),
            Err(err) => {
                eprintln!("[storage] {}", err.to_string());
                println!("[storage] creating...");
                open_option
                    .create_new(true)
                    .open(path)
                    .await
            } 
        }
    }

    #[allow(unused_variables)]
    async fn get(&self, username: &str) -> io::Result<Vec<u8>> {
        let mut storage = self.storage.borrow_mut();

        let mut seek_idx = 1;

        let buffer_cap = 2048;
        let mut buffer = BytesMut::with_capacity(buffer_cap);
        // TODO: Timeout
        loop {
            let seek_from = io::SeekFrom::Current(seek_idx);
            storage.seek(seek_from).await?;
            storage.read_buf(&mut buffer).await?;

            if buffer.len() == 0 {
                break;
            }

            let (pwd, last_eor) = get_comparator(username, &mut buffer);
            if pwd.len() > 0 {
                return Ok(pwd);
            }

            if buffer.len() != last_eor {
                seek_idx = last_eor as i64
            }

            if buffer.len() < buffer_cap {
                break;
            }
            unsafe { buffer.set_len(0) };
        }
        
        Ok(Vec::new())
    }

    async fn write(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut writer = self.storage.borrow_mut();
        writer.seek(io::SeekFrom::End(0)).await?;
        let written = writer.write(&buffer).await?;
        writer.flush().await?;
        Ok(written)
    }
}

// TODO: build as struct, buffer into iter
/// vector leng == 0 indicates username not yet found
/// return (password bytes, last END_RECORD position)
fn get_comparator(username: &str, buffer: &mut BytesMut) -> (Vec<u8>, usize) {
    // read value state
    // state 1: get username value
    // state 2: skip reading password
    // state 3: return AuthData
    let mut val_state: u8 = 0;
    let mut val_counter = 0;

    let mut i = 0;
    let mut last_eor = 0;
    
    while i < buffer.len() {
        if val_state == 1 {
            let idx = i + val_counter;
            if idx > buffer.len() {
                break;
            }
            
            let f = &buffer[i..idx];
            let vv = f.to_vec();
            val_state += vv.eq(username.as_bytes()) as u8;
        }

        else if val_state == 2 {
            let idx = i + val_counter;
            if idx > buffer.len() {
                break;
            } else if idx == buffer.len() {
                last_eor += 2;
                break;
            }
            val_state = 0;
        } 
        
        else if val_state == 3 {
            let idx = i + val_counter;
            if idx > buffer.len() {
                break;
            }
            
            let f = &buffer[i..idx];
            return (f.to_vec(), last_eor);
        } 
        
        else {
            match buffer[i] {
                END_RECORD => { last_eor = i; },
                VALUE_SIGN => { val_state = (val_state % 2) + 1; },
                GROUP_SEPARATOR => {},
                _ => {
                    let mut loop_buf = Vec::with_capacity(8);
                    while i < buffer.len() {
                        i+=1;
                        let b = buffer[i];
                        if b == VALUE_SIGN {
                            break;
                        }
                        loop_buf.push(b);
                    }
                    
                    val_counter = String::from_utf8(loop_buf)
                        .unwrap()
                        .parse()
                        .unwrap();
                }
            }
        }
        i+=1;
    }
    (Vec::new(), last_eor)
}

impl AuthenticationStore for Authenticator {
    #[allow(unused_variables)]
    async fn authenticate(&self, username: &str, password: &str) -> bool {
        true
    }

    async fn create(&self, username: String, password: String) -> bool {
        let auth = AuthData::new(username, password);

        let mut buffer = BytesMut::with_capacity(300);
        auth.read(&mut buffer);
        
        // TODO: ensure all data is written or rollback
        match self.write(&mut buffer).await {
            Err(e) => { eprintln!("[authentication] {}", e.to_string()); false },
            Ok(v) => v > 0
        }
    }
}

pub trait AuthenticationStore {
    async fn authenticate(&self, username: &str, password: &str) -> bool;
    async fn create(&self, username: String, password: String) -> bool;
}

struct AuthData {
    username: String,
    password: String
}

impl AuthData {
    // TODO: hash password
    fn new(username: String, password: String) -> Self {
        Self { username, password }
    }

    fn read(&self, buffer: &mut impl BufMut) -> usize {
        let mut counter = 1;
        
        Self::parts(&self.username, &mut counter, buffer);
        Self::parts(&self.password, &mut counter, buffer);

        buffer.put_u8(END_RECORD);
        counter + 1
    }

    
    fn parts(v: &str, counter: &mut usize, buffer: &mut impl BufMut) {
        let v_str_leng = format!("{}", v.len());
        buffer.put(v_str_leng.as_bytes());
        
        *counter += v_str_leng.len() + 1;
        buffer.put_u8(VALUE_SIGN);
        
        *counter += v.len() + 1;
        buffer.put(v.as_bytes());
        buffer.put_u8(GROUP_SEPARATOR);
    }
}

#[cfg(test)]
mod test {
    use super::{AuthenticationStore, Authenticator};

    #[tokio::test]
    async fn ensure_create_or_open() {
        match Authenticator::new().await {
            Err(e) => {
                eprintln!("[auth] {}", e.to_string());
                panic!()
            }, Ok(v) => v
        };

        match Authenticator::new().await {
            Err(e) => {
                eprintln!("[auth] {}", e.to_string());
                panic!()
            }, Ok(v) => v
        };
    }
    async fn authenticate(authctr: &impl AuthenticationStore, username: String, password: String) -> bool {
        authctr.create(username, password).await
    }

    #[tokio::test]
    async fn create_user() {
        let authenticator = match Authenticator::new().await {
            Err(e) => {
                eprintln!("[auth] {}", e.to_string());
                panic!()
            }, Ok(v) => v
        };

        let t1 = authenticate(&authenticator, "arisy".to_owned(), "wadidawww l;".to_owned()).await;
        let t2 = authenticate(&authenticator, "prikis".to_owned(), "kenllopm21".to_owned()).await;
        let v = vec![t1, t2];
        for vv in v {
            assert!(vv)
        }
    }
}