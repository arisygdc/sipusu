#![allow(dead_code)] 
use std::{io, mem};

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

pub struct Authenticator { storage_path: String }

impl Authenticator {
    pub async fn new(path: String) -> Result<Self, io::Error> {
        Self::prepare_storage(&path).await?;
        Ok(Self{ storage_path: path })
    }

    #[inline]
    async fn prepare_storage(path: &String) -> io::Result<File> {
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

    async fn get(&self, username: &str) -> io::Result<PasswordHashed> {
        let mut storage = OpenOptions::new()
            .read(true)
            .open(&self.storage_path)
            .await?;

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
        let mut storage = OpenOptions::new()
            .write(true)
            .open(&self.storage_path)
            .await?;
        storage.seek(io::SeekFrom::End(0)).await?;
        let written = storage.write(&buffer).await?;
        storage.flush().await?;
        Ok(written)
    }
}

impl AuthenticationStore for Authenticator {
    async fn authenticate(&self, auth: &AuthData) -> bool {
        let result_pwd = match self.get(&auth.username).await {
            Ok(pwd) => pwd,
            Err(_) => return false
        };
        
        match &auth.password {
            Password::Hashed(h) => {
                result_pwd.password.eq(&h.password)
            }, Password::Plain(s) => {
                if let Ok(res) = argon2::verify_encoded(&result_pwd.password, s.as_bytes()) {
                    return res;
                }
                false
            }
        }
    }

    // TODO: check if username is exists, and produce error
    async fn create(&self, auth: AuthData) -> bool {
        let data_create = match auth.password {
            Password::Plain(_) => auth.hash_password(),
            Password::Hashed(_) => auth
        };

        let mut buffer = BytesMut::with_capacity(311);
        println!("read: {}", data_create.read(&mut buffer));
        
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
    async fn authenticate(&self, auth: &AuthData) -> bool;
    async fn create(&self, auth: AuthData) -> bool;
}

#[cfg_attr(test, derive(Clone))]
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

#[cfg_attr(test, derive(Clone))]
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

#[cfg_attr(test, derive(Clone))]
pub struct AuthData {
    username: String,
    password: Password,
}

impl AuthData {
    // FIXME: it should return error instead of panic
    pub fn decode(buffer: &[u8]) -> Self {
        let mut placeholder = Vec::with_capacity(2);
        let mut i = 0;
        while i < buffer.len() {
            match buffer[i] {
                0x5B => (),
                _ => {
                    i += 1;
                    continue
                }
            }

            i += 1;
            let start = i;
            let mut end = 0;
            while i < buffer.len() {
                if buffer[i] == 0x5D {
                    end = i;
                    break;
                }
                i+=1;
            }
            
            let enc_str = &buffer[start..end];
            let encd_str = String::from_utf8(enc_str.to_vec()).unwrap();
            placeholder.push(encd_str);
        }

        if placeholder.len() != 2 {
            panic!()
        }
        
        let username: String = mem::take(&mut placeholder[0]);
        let password = mem::take(&mut placeholder[1]);
        Self::new(username, password)
    }

    #[inline]
    pub fn new_hashed(username: String, password: String) -> Self {
        let hashed_password = Password::new_hash(password.as_bytes());
        Self { username, password: hashed_password }
    }

    #[inline]
    pub fn new(username: String, password: String) -> Self {
        Self { username, password: Password::Plain(password) }
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
    
    pub fn hash_password(mut self) -> Self {
        if let Password::Plain(s) = self.password {
            let hashed = PasswordHashed::new(s.as_bytes());
            self.password = Password::Hashed(hashed)
        }

        self
    }
}

#[cfg(test)]
mod test {
    #![allow(unused)]
    use super::{AuthenticationStore, Authenticator, AuthData};

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
            let auth_data = AuthData::new(test.uname, test.pwd);
            let created = authenticator.create(auth_data.clone().hash_password()).await;
            assert!(created);
            let authenticated = authenticator.authenticate(&auth_data).await;
            assert!(authenticated)
        }
    }

    #[tokio::test]
    async fn decode_checked() {
        let authenticator = match Authenticator::new(String::from("./piw")).await {
            Err(e) => {
                eprintln!("[auth] {}", e.to_string());
                panic!()
            }, Ok(v) => v
        };

        let words = vec![
            "[arisy]\n[wadidawww9823]",
            "[prikis]\n[kenllopm21]",
            "[jtrtt]\n[uohafw@43eughr\"ew2185few{}Q@$]"
        ];

        for test in words {
            let auth_data = AuthData::decode(test.as_bytes());
            let authenticated = authenticator.authenticate(&auth_data).await;
            assert!(authenticated)
        }
    }
}