#![allow(dead_code)] 
use std::{cell::RefCell, io};

use bytes::{BufMut, BytesMut};
use tokio::{fs::{File, OpenOptions}, io::{AsyncSeekExt, AsyncWriteExt}};
const END_RECORD: u8 = 0x1E;
const GROUP_SEPARATOR: u8 = 0x1D;
const VALUE_SIGN: u8 = 0x2;
// const 
pub struct Authenticator {
    storage: RefCell<File>
}

impl Authenticator {
    pub async fn new() -> Result<Self, io::Error> {
        let storage = RefCell::new(Self::prepare_storage().await?);
        Ok(Self { storage })
    }

    // FIXME: Error create_new when file exists
    // TODO: open or create when file not existx
    #[inline]
    async fn prepare_storage() -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .append(true)
            .create_new(true)
            .open("./wow")
            .await
    }

    // async fn get(&self, username: &str) -> AuthData {
    //     let mut storage = self.storage.borrow_mut();
    //     let seeker_index = 0;

    //     let seek_from = io::SeekFrom::Current(seeker_index);
    //     let seeker = storage.seek(seek_from).await;
    //     storage.
    //     AuthData::new("username".to_owned(), "password".to_owned())
    // }

    async fn write(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut writer = self.storage.borrow_mut();
        writer.seek(io::SeekFrom::End(0)).await?;
        let written = writer.write(&buffer).await?;
        writer.flush().await?;
        Ok(written)
    }
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

        counter += 1;
        buffer.put_bytes(END_RECORD, counter);

        counter
    }

    
    fn parts(v: &str, counter: &mut usize, buffer: &mut impl BufMut) {
        *counter += 1;
        buffer.put_bytes(GROUP_SEPARATOR, *counter);
        
        let len_str = format!("{}", v.len());
        buffer.put(len_str.as_bytes());

        *counter += len_str.len() + 1;
        buffer.put_bytes(VALUE_SIGN, *counter);
        
        *counter += v.len() + 1;
        buffer.put(v.as_bytes());
        buffer.put_bytes(GROUP_SEPARATOR, *counter);
    }
}

#[cfg(test)]
mod test {
    use super::{AuthenticationStore, Authenticator};
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