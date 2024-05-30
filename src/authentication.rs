#![allow(dead_code)] 
use std::{cell::RefCell, io::{self}};

use argon2::{self, Config};
use bytes::{BufMut, BytesMut};
use tokio::{fs::{File, OpenOptions}, io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt}};

// file specifier
const SEGMENT_SIZE: usize = 311;

// Table information
const USERNAME_CAP: usize = 30;
const PASSWORD_CAP: usize = 256;
const ALG_CAP: usize = 10;
const SALT_CAP: usize = 10;

pub struct Authenticator {
    storage: RefCell<File>
}

impl Authenticator {
    pub async fn new(path: String) -> Result<Self, io::Error> {
        let storage = RefCell::new(Self::prepare_storage(path).await?);
        Ok(Self { storage })
    }

    #[inline]
    async fn prepare_storage(path: String) -> io::Result<File> {
        let mut storage_opt = OpenOptions::new();
        let open_option = storage_opt
            .read(true)
            .write(true)
            .append(true);

        let try_open = open_option
            .open(&path)
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

    async fn get(&self, username: &str) -> io::Result<PasswordHashed> {
        let mut storage = self.storage.borrow_mut();

        let mut seek_idx = 0;

        let segread_wide = 4;
        let buffer_cap = segread_wide * SEGMENT_SIZE;
        let mut buffer = BytesMut::with_capacity(buffer_cap);
        // TODO: Timeout
        loop {
            let seek_from = io::SeekFrom::Start(seek_idx);
            storage.seek(seek_from).await?;
            storage.read_buf(&mut buffer).await?;

            if buffer.len() == 0 {
                break;
            }

            for i in 0..segread_wide {
                let seg_read = SegmentRead::new(i*SEGMENT_SIZE, &buffer);
                if let Some(res) = seg_read.evaluate(username) {
                    return Ok(res);
                }
            }

            if buffer.len() < buffer_cap {
                break;
            }

            seek_idx += SEGMENT_SIZE as u64 + 1;
            unsafe { buffer.set_len(0) };
        }
        
        let err = io::Error::new(io::ErrorKind::NotFound, "username not found".to_owned());
        Err(err)
    }

    async fn write(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut writer = self.storage.borrow_mut();
        writer.seek(io::SeekFrom::End(0)).await?;
        let written = writer.write(&buffer).await?;
        writer.flush().await?;
        Ok(written)
    }
}

impl AuthenticationStore for Authenticator {
    async fn authenticate(&self, username: &str, password: &str) -> bool {
        let result_pwd = match self.get(username).await {
            Ok(pwd) => pwd,
            Err(_) => return false
        };
    
        match argon2::verify_encoded(&result_pwd.password, password.as_bytes()) {
            Err(_) => false,
            Ok(l) => l 
        }
    }

    // TODO: check if username is exists, and produce error
    async fn create(&self, username: String, password: String) -> bool {
        let auth = AuthData::new(username, password);

        let mut buffer = BytesMut::with_capacity(311);
        println!("read: {}", auth.read(&mut buffer));
        
        // TODO: ensure all data is written or rollback
        match self.write(&mut buffer).await {
            Err(e) => { eprintln!("[authentication] {}", e.to_string()); false },
            Ok(v) => v > 0
        }
    }
}

pub struct SegmentRead<'a> {
    pos: usize,
    src: &'a [u8]
}

impl<'a> SegmentRead<'a> {
    fn new(pos: usize, src: &'a [u8]) -> Self {
        SegmentRead { pos, src }
    }

    fn evaluate(mut self, target: &str) -> Option<PasswordHashed> {
        let mut uname_bytes = Vec::with_capacity(USERNAME_CAP);
        let end = self.pos+USERNAME_CAP;
        for j in self.pos..end-(USERNAME_CAP - target.len()) {
            uname_bytes.push(self.src[j])
        }
        
        if !uname_bytes.eq(target.as_bytes()) {
            return None;
        }

        self.pos = end;

        let password = self.grab(PASSWORD_CAP);
        let password = String::from_utf8(password).unwrap_or_default();

        let alg = self.grab(ALG_CAP);
        let alg = String::from_utf8(alg).unwrap_or_default();

        let salt = self.grab(SALT_CAP);
        let salt = {
            if salt.len() == 0 {
                return None;
            }
            String::from_utf8(salt).ok()
        };

        Some(PasswordHashed {
            password, alg, salt
        })
        
    }

    fn grab(&mut self, cap: usize) -> Vec<u8> {
        self.pos += 1;
        let end = self.pos + cap;

        let mut placeholder = Vec::with_capacity(PASSWORD_CAP);
        for j in self.pos..end {
            let val =  self.src[j];
            if 0x0 == val {
                break;
            }
            placeholder.push(val)
        }

        self.pos += cap;
        placeholder
    }
}

pub trait AuthenticationStore {
    async fn authenticate(&self, username: &str, password: &str) -> bool;
    async fn create(&self, username: String, password: String) -> bool;
}


struct PasswordHashed {
    password: String,
    alg: String,
    salt: Option<String>
}

impl PasswordHashed {
    fn new(pwd: &[u8]) -> Self {
        let hash_cfg = Config::default();
        let salt = String::from("random78");
        let password = argon2::hash_encoded(pwd, salt.as_bytes(), &hash_cfg).unwrap();

        PasswordHashed { password, alg: String::from("argon2"), salt: Some(salt) }
    }
}

enum Password {
    Hashed(PasswordHashed),
    Plain(String)
}

impl Password {
    fn new_hash(pwd: &[u8]) -> Self {
        let hashed = PasswordHashed::new(pwd);
        Password::Hashed(hashed)
    }
}

struct AuthData {
    username: String,
    password: Password,
}

impl AuthData {
    fn new(username: String, password: String) -> Self {
        let hashed_password = Password::new_hash(password.as_bytes());
        Self { username, password: hashed_password }
    }

    fn read(&self, buffer: &mut impl BufMut) -> usize {
        let mut counter = 0;
        
        Self::parts(&self.username, USERNAME_CAP, &mut counter, buffer);
        // FIXME: should not error when password is not plain text
        let password = match &self.password {
            Password::Plain(_) => panic!(),
            Password::Hashed(pwd) => pwd
        };

        Self::parts(&password.password, PASSWORD_CAP, &mut counter, buffer);
        Self::parts(&password.alg, ALG_CAP, &mut counter, buffer);

        let salt = password.salt.as_ref()
            .map(|f| f.clone())
            .unwrap_or_default();
        Self::parts(&salt, SALT_CAP, &mut counter, buffer);

        buffer.put_u8(0x0A);
        counter + 1
    }

    
    /// this function assume !v.len() > cap 
    fn parts(v: &str, cap: usize, counter: &mut usize,buffer: &mut impl BufMut) {
        *counter += cap + 1;
        buffer.put(v.as_bytes());
        if v.len() < cap {
            buffer.put_bytes(0x0, cap - v.len());
        }

        buffer.put_u8(0x1f)
    }
}

#[cfg(test)]
mod test {
    use super::{AuthenticationStore, Authenticator};

    #[tokio::test]
    async fn ensure_create_or_open() {
        match Authenticator::new(String::from("./piw")).await {
            Err(e) => {
                eprintln!("[auth] {}", e.to_string());
                panic!()
            }, Ok(v) => v
        };

        match Authenticator::new(String::from("./piw")).await {
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
    async fn create_user_checked() {
        let authenticator = match Authenticator::new(String::from("./piw")).await {
            Err(e) => {
                eprintln!("[auth] {}", e.to_string());
                panic!()
            }, Ok(v) => v
        };

        struct DataTable {
            uname: String,
            pwd: String,
        }

        let data_testing = vec![
            DataTable { uname: "arisy".to_owned(), pwd: "wadidawww9823".to_owned() },
            DataTable { uname: "prikis".to_owned(), pwd: "kenllopm21".to_owned() },
            DataTable { uname: "jtrtt".to_owned(), pwd: "uohafw@43eughr\"ew2185few{}Q@$".to_owned() }
        ];

        for test in data_testing {
            let created = authenticator.create(test.uname.clone(), test.pwd.clone()).await;
            assert!(created);
            let authenticated = authenticator.authenticate(&test.uname, &test.pwd).await;
            assert!(authenticated)
        }
    }
}