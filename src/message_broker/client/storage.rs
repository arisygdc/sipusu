use std::{env, path::{Path, PathBuf}};

use bytes::{BufMut, BytesMut};
use tokio::{fs::{create_dir, OpenOptions}, io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader}};

use crate::protocol::v5::subscribe::Subscribe;

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
        let mut fopt = OpenOptions::new();
        fopt
            .write(true)
            .create(true);

        cl_space.push(clid.to_string());
        if !cl_space.is_dir() {
            create_dir(&cl_space).await?;
        }

        cl_space.push("subscribed");
        if !cl_space.exists() {
            fopt.open(&cl_space).await?;
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
            buffer.put_u8(item.max_qos);
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
                target_seek += n;
                let mut c = 0;
                for i in 0..n {
                    let is_eq = r_buf[i] == 0xA;
                    if !is_eq {
                        continue;
                    }

                    let ok = r_buf[c+2..i].eq(topic.as_bytes());
                    if !ok {
                        c = i+1;
                        continue;
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

#[cfg(test)]
mod tests {
    use crate::protocol::v5::subscribe::Subscribe;

    use super::{ClientID, ClientStore};

    #[tokio::test]
    async fn log_path() {
        let clid = ClientID::new("raw_clid".to_owned());
        let l = ClientStore::new().await;
        let logs = l.unwrap();
        logs.prepare(&clid).await
            .unwrap();
        tokio::fs::remove_dir(logs.path).await.unwrap();
    }

    #[tokio::test]
    async fn append() {
        let clid = ClientID::new("raw_clid".to_owned());
        let l = ClientStore::new().await;
        let logs = l.unwrap();
        logs.prepare(&clid).await
            .unwrap();
        
        let v = vec![
            Subscribe{
                max_qos: 0x1,
                topic: "topic/a".to_owned()
            },Subscribe{
                max_qos: 0x1,
                topic: "topic/b".to_owned()
            }, Subscribe{
                max_qos: 0x1,
                topic: "topic/c".to_owned()
            },
        ];
        logs.subscribe(&clid, &v)
        .await
        .unwrap();
        // logs.unsubscribe(&clid, &v[0].topic).await.unwrap();
        logs.unsubscribe(&clid, &v[2].topic).await.unwrap();
    }
}