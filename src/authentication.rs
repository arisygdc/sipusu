#![allow(dead_code)] 
use std::{cell::RefCell, io, mem};

use bytes::BytesMut;
use tokio::{fs::{File, OpenOptions}, io::AsyncWriteExt};

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
        OpenOptions::new()
            .read(true)
            .write(true)
            .append(true)
            .create_new(true)
            .open("/var/test_host/authentication_table")
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
        let written = writer.write(&buffer).await?;
        writer.flush().await?;
        Ok(written)
    }
}

impl Authentication for Authenticator {
    #[allow(unused_variables)]
    async fn authenticate(&self, username: &str, password: &str) -> bool {
        true
    }

    async fn create(&mut self, username: String, password: String) -> bool {
        let auth = AuthData::new(username, password);

        let mut buffer = BytesMut::with_capacity(300);
        let read_count = auth.read(&mut buffer);

        // TODO: ensure all data is written or rollback
        match self.write(&mut buffer).await {
            Err(e) => { eprintln!("[authentication] {}", e.to_string()); false },
            Ok(v) => v == read_count
        }
    }
}

pub trait Authentication {
    async fn authenticate(&self, username: &str, password: &str) -> bool;
    async fn create(&mut self, username: String, password: String) -> bool;
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

    fn read(self, buffer: &mut [u8]) -> usize {
        let counter = 0;

        let mut username_inner = self.username.into_bytes();
        for v in username_inner.iter_mut() {
            buffer[counter] = mem::take(v);
        }

        counter
    }
}