use std::{env, path::{Path, PathBuf}};

use bytes::{BufMut, BytesMut};
use tokio::{fs::OpenOptions, io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader}};

use super::{client::ClientID, DATA_STORE};

pub struct ClientLogs {
    path: PathBuf,
}

impl ClientLogs {
    async fn new(clid: &ClientID) -> io::Result<Self> {
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
        Ok(Self { path: path.to_owned() })
    }

    async fn subscribe(&self, topics: &[String]) -> io::Result<()> {
        let mut fopt = OpenOptions::new();
        let mut f = fopt
            .append(true)
            .open(format!("{}/subscribed", self.path.display()))
            .await?;
        let mut buffer = BytesMut::with_capacity(20 * topics.len());
        topics.iter().for_each(|item| {
            buffer.put_u8(0x1);
            buffer.put(item.as_bytes());
            buffer.put_u8(0xA);
        });
        f.seek(io::SeekFrom::End(0)).await?;
        f.write_all(&buffer).await
    }

    async fn unsubscribe(&self, topic: &str) -> io::Result<()> {
        let mut fopt = OpenOptions::new();
        let f = fopt
            .read(true)
            .write(true)
            .open(format!("{}/subscribed", self.path.display()))
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

#[cfg(test)]
mod tests {
    use super::{ClientID, ClientLogs};

    #[tokio::test]
    async fn log_path() {
        let clid = ClientID::new("raw_clid".to_owned());
        let l = ClientLogs::new(&clid).await;
        let logs = l.unwrap();
        tokio::fs::remove_dir(logs.path).await.unwrap();
    }

    #[tokio::test]
    async fn append() {
        let clid = ClientID::new("raw_clid".to_owned());
        let l = ClientLogs::new(&clid).await;
        let logs = l.unwrap();
        let v = vec![
            "topic/a".to_owned(),
            "topic/b".to_owned()
        ];
        logs.subscribe(&v)
        .await
        .unwrap();
        logs.unsubscribe(&v[0]).await.unwrap();
    }
}