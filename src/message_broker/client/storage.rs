use std::{env, path::{Path, PathBuf}};

use bytes::{BufMut, BytesMut};
use tokio::{fs::OpenOptions, io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader}};

use crate::protocol::subscribe::Subscribe;

use super::{client::ClientID, DATA_STORE};

#[derive(Debug, Clone)]
pub struct ClientStore {
    path: PathBuf,
}

impl ClientStore {
    pub async fn new() -> io::Result<Self> {
        let s = format!("{}/{}", env::current_dir().unwrap().display(), DATA_STORE);
        let path = Path::new(&s);
        if !path.is_dir() {
            tokio::fs::create_dir(&path).await?;
        };

        Ok(Self { path: path.to_owned() })
    }


    pub async fn prepare(&self, clid: &ClientID) -> io::Result<()> {
        let mut cl_space = self.path.clone();
        cl_space.push(clid.to_string());
        cl_space.push("subscribe");
        
        if !cl_space.exists() {
            let mut fopt = OpenOptions::new();
            fopt.write(true).create(true).open(cl_space).await?;
        }
        Ok(())
    }

    // TODO: search if exists
    pub async fn subscribe(&self, clid: &ClientID, topics: &[Subscribe]) -> io::Result<()> {
        let mut fopt = OpenOptions::new();
        let mut f = fopt
            .append(true)
            .open(format!("{}/{}/subscribed", self.path.display(), clid))
            .await?;
        let mut buffer = BytesMut::with_capacity(20 * topics.len());
        topics.iter().for_each(|item| {
            buffer.put_u8(0x1);
            buffer.put_u8(item.qos);
            buffer.put(item.topic.as_bytes());
            buffer.put_u8(0xA);
        });
        f.seek(io::SeekFrom::End(0)).await?;
        f.write_all(&buffer).await
    }

    pub async fn unsubscribe(&self, clid: &ClientID, topic: &str) -> io::Result<()> {
        let mut fopt = OpenOptions::new();
        let f = fopt
            .read(true)
            .write(true)
            .open(format!("{}/{}/subscribed", self.path.display(), clid))
            .await?;

        let mut reader = BufReader::new(f);
        let mut buf = BytesMut::zeroed(1024);
        let mut target_seek = 0;
        // TODO: check when looping
        while let Ok(n) = reader.read(&mut buf).await {
            if n == 0 {
                break;
            }
            
            {
                let r_buf = buf.split_to(n);
                println!("rbuf: {:?}", r_buf);
                target_seek += n;
                let mut c = 0;
                for i in 0..n {
                    let is_eq = r_buf[i] == 0xA;
                    if !is_eq {
                        continue;
                    }

                    let ok = r_buf[c+1..i].eq(topic.as_bytes());
                    if ok {
                        c = i;
                    }
                    
                    let t = target_seek - n;
                    target_seek = t + c;
                    println!("found {}", target_seek);
                    let mut f = reader.into_inner();
                    f.seek(io::SeekFrom::Start(target_seek as u64)).await?;
                    f.write(&[0x0]).await?;
                    return Ok(());
                }
            }
            buf.reserve(n);
            unsafe{buf.set_len(0)};
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound, 
            format!("cannot find topic {}", topic))
        )
    }
}

pub(super) async fn prepare(clid: &ClientID) -> io::Result<()> {
    let s = format!("{}/{}/{}", env::current_dir().unwrap().display(), DATA_STORE, clid);
    let path = Path::new(&s);
    if !path.is_dir() {
        tokio::fs::create_dir(&path).await?;
    };

    let sub = format!("{}/subscribed", &path.display());
    let subpath = Path::new(&sub);
    if !subpath.exists() {
        let mut fopt = OpenOptions::new();
        fopt.write(true).create(true).open(subpath).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::protocol::subscribe::Subscribe;

    use super::{ClientID, ClientStore};

    #[tokio::test]
    async fn log_path() {
        let clid = ClientID::new("raw_clid".to_owned());
        let l = ClientStore::new().await;
        let logs = l.unwrap();
        logs.prepare(&clid).await;
        tokio::fs::remove_dir(logs.path).await.unwrap();
    }

    #[tokio::test]
    async fn append() {
        let clid = ClientID::new("raw_clid".to_owned());
        let l = ClientStore::new().await;
        let logs = l.unwrap();
        logs.prepare(&clid).await;
        
        let v = vec![
            Subscribe{
                qos: 0x1,
                topic: "topic/b".to_owned()
            },Subscribe{
                qos: 0x1,
                topic: "topic/c".to_owned()
            },
        ];
        logs.subscribe(&clid, &v)
        .await
        .unwrap();
        logs.unsubscribe(&clid, &v[0].topic).await.unwrap();
    }
}