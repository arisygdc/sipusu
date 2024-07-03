use std::{collections::HashMap, env, path::PathBuf};
use bytes::{BufMut, BytesMut};
use tokio::{fs::{File, OpenOptions}, io::{self, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter}};
use crate::protocol::v5::{subscribe::Subscribe, ServiceLevel};
use super::{client::ClientID, DATA_STORE};

/// Always clone when use, this case do for pass the borrow checker. 
#[derive(Debug, Clone)]
pub struct ClientStore {
    path: PathBuf,
}

impl ClientStore {
    pub(super) async fn new(clid: &ClientID) -> io::Result<Self> {
        let mut dir = env::current_dir()?;
        dir.push(DATA_STORE);
        let clroot_exists = dir.exists();
        
        dir.push(clid.to_string());
        let cldir_exists = dir.exists();
        if !clroot_exists {
            tokio::fs::create_dir_all(&dir).await?;
        } else if !cldir_exists {
            tokio::fs::create_dir(&dir).await?;
        }

        let mut fopt = OpenOptions::new();
        fopt
            .write(true)
            .create(true);

        let mut path = dir.clone();
        path.push("subscribed");
        let f = fopt.open(path).await?;
        f.set_len(0).await?;

        Ok(Self { path: dir.to_owned() })
    }

    pub async fn subscribe(self, topics: &[Subscribe]) -> io::Result<()> {
        let f =  {
            let mut subs_path = self.path;
            subs_path.push("subscribed");

            let mut fopt = OpenOptions::new();
            fopt
                .read(true)
                .write(true)
                .append(true)
                .create(true)
                .open(subs_path)
                .await?
        };
        
        let mut reader = BufReader::new(f);
        let mut map = HashMap::new();
        let _readed = get_subscribed(&mut reader, &mut map).await?;
        
        for sub in topics.iter() {
            match map.get_mut(&sub.topic) {
                Some(v) => {
                    let not_equal = sub.max_qos.ne(&v);
                    if not_equal {
                        *v = sub.max_qos.clone();
                    }
                },
                None => {map.insert(sub.topic.clone(), sub.max_qos.clone());}
            }
        }

        // let seek_pos = match rewrite {
        //     true => SeekFrom::Start(0),
        //     false => SeekFrom::Start(readed as u64)
        // };

        let f = reader.into_inner();
        f.set_len(0).await?;
        
        let mut writer = BufWriter::new(f);
        write_subscribed(&mut writer, &mut map).await.unwrap();
        Ok(())
    }

    pub async fn unsubscribe(self, topics: &[String]) -> io::Result<()> {
        let f =  {
            let mut subs_path = self.path;
            subs_path.push("subscribed");

            let mut fopt = OpenOptions::new();
            fopt
                .read(true)
                .append(true)
                .write(true)
                .open(subs_path)
                .await?
        };

        let mut reader = BufReader::new(f);
        let mut map = HashMap::new();
        let _readed = get_subscribed(&mut reader, &mut map).await?;
        
        for sub in topics.iter() {
            map.remove(sub);
        }
        
        let f = reader.into_inner();
        f.set_len(0).await?;
        
        let mut writer = BufWriter::new(f);
        write_subscribed(&mut writer, &mut map).await.unwrap();
        Ok(())
    }
}

async fn get_subscribed(stream: &mut BufReader<File>, map: &mut HashMap<String, ServiceLevel>) -> io::Result<usize> {
    let mut buf = BytesMut::zeroed(1024);
    let mut readed = 0;

    // TODO: check when looping
    while let Ok(n) = stream.read(&mut buf).await {
        if n == 0 {
            break;
        }

        let r_buf = buf.split_to(n);
        let mut c = 0;
        for i in 0..n {
            let is_eq = r_buf[i] == 0xA;
            if !is_eq {
                continue;
            }

            let k = String::from_utf8(r_buf[c+1..i].to_vec())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

            let v = ServiceLevel::try_from(r_buf[c])
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid QoS"))?;

            map.insert(k, v);
            c = i + 1;
        }

        readed += n;
        if n != 1024 {
            break;
        }
        buf.reserve(n);
    }
    Ok(readed)
}

async fn write_subscribed(stream: &mut BufWriter<File>, map: &mut HashMap<String, ServiceLevel>) -> io::Result<usize> {
    let mut est_leng = 0;
    map.iter().for_each(|(each, _)| {
        est_leng += 2 + each.len()
    });

    let mut buf = BytesMut::with_capacity(est_leng);

    map.iter().for_each(|(each, slvl)| {
        buf.put_u8(slvl.code());
        buf.put(each.as_bytes());
        buf.put_u8(0x0A);
    });

    stream.write_all(&buf).await?;
    stream.flush().await?;
    Ok(est_leng)
}


#[cfg(test)]
mod tests {
    use crate::protocol::v5::{subscribe::Subscribe, ServiceLevel};

    use super::{ClientID, ClientStore};

    #[tokio::test]
    async fn log_path() {
        let clid = ClientID::new("raw_clid".to_owned());
        let l = ClientStore::new(&clid).await;
        let logs = l.unwrap();
        tokio::fs::remove_dir(logs.path).await.unwrap();
    }

    #[tokio::test]
    async fn append() {
        let clid = ClientID::new("raw_clid".to_owned());
        let l = ClientStore::new(&clid).await;
        let logs = l.unwrap();
        
        let v = vec![
            Subscribe{
                max_qos: ServiceLevel::QoS1,
                topic: "topic/a".to_owned()
            },Subscribe{
                max_qos: ServiceLevel::QoS1,
                topic: "topic/b".to_owned()
            }, Subscribe{
                max_qos: ServiceLevel::QoS1,
                topic: "topic/c".to_owned()
            },
        ];
        let subs_log = logs.clone();
        subs_log.subscribe(&v)
            .await
            .unwrap();
    
        let p = [v[2].topic.clone()];
        logs.unsubscribe(&p).await.unwrap();
    }
}