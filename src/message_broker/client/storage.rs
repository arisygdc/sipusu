#![allow(dead_code)]
use std::{collections::HashMap, env, path::PathBuf};
use bytes::{Buf, BufMut, BytesMut};
use tokio::{fs::{File, OpenOptions}, io::{self, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter}};
use crate::protocol::v5::{connect, subscribe::Subscribe, ServiceLevel};
use super::{client::ClientID, DATA_STORE};

/// Always clone when use, this case do for pass the borrow checker. 
#[derive(Debug, Clone)]
pub struct ClientStore {
    path: PathBuf,
}

pub(super) struct MetaData {
    pub(super) protocol_level: u8,
    pub(super) keep_alive_interval: u16,
    pub(super) expr_interval: u32,
    pub(super) maximum_qos: u8,
    pub(super) receive_maximum: u16,
    pub(super) topic_alias_maximum: u16,
    pub(super) user_properties: Vec<(String, String)>,
}

impl MetaData {
    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_u8(self.protocol_level);
        buffer.put_u16(self.keep_alive_interval);
        buffer.put_u32(self.expr_interval);
        buffer.put_u8(self.maximum_qos);
        buffer.put_u16(self.receive_maximum);
        buffer.put_u16(self.topic_alias_maximum);
        buffer.put_bytes(0x0A, 2);
        for up in &self.user_properties {
            serialize_string(buffer, &up.0);
            serialize_string(buffer, &up.1);
        }
    }

    fn est_len(&self) -> usize {
        let mut est_len = 15;
        for up in self.user_properties.iter() {
            est_len += 2 + up.0.len() + 2 + up.1.len() + 1;
        }
        est_len
    }

    fn deserialize(buffer: &mut BytesMut) -> Self {
        Self {
            protocol_level: buffer.get_u8(),
            keep_alive_interval: buffer.get_u16(),
            expr_interval: buffer.get_u32(),
            maximum_qos: buffer.get_u8(),
            receive_maximum: buffer.get_u16(),
            topic_alias_maximum: buffer.get_u16(),
            user_properties: {
                let mut prop = Vec::new();
                while buffer.len() != 0 {
                    prop.push((
                        deserialize_string(buffer),
                        deserialize_string(buffer)
                    ))
                }
                prop
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Will {
    pub(super) property: Option<connect::Properties>,
    pub(super) topic: String,
    pub(super) payload: Vec<u8>
}

impl Will {
    fn serialize(&self, buffer: &mut BytesMut) {
        serialize_string(buffer, &self.topic);
        buffer.put_u8(0xA);
        buffer.put(self.payload.as_slice());
    }

    fn est_len(&self) -> usize {
        2 + self.topic.len() + 1 + self.payload.len()
    }

    fn deserialize(buffer: &mut BytesMut) -> Self {
        let mut new = Self::default();
        new.topic = deserialize_string(buffer);
        new.payload = buffer.to_vec();
        new
    }
}
fn serialize_string(buffer: &mut BytesMut, val: &str) {
    buffer.put_u16(val.len() as u16);
    buffer.put(val.as_bytes());
}

fn deserialize_string(buffer: &mut BytesMut) -> String {
    let len = buffer.get_u16();
    let b = buffer.split_to(len as usize);
    String::from_utf8(b.to_vec()).unwrap()
}

impl ClientStore {
    pub(super) async fn new(clid: &ClientID, mdata: &MetaData, will: Option<Will>) -> io::Result<Self> {
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

        {
            let mut path = dir.clone();
            path.push("subscribed");
            let f = fopt.open(&path).await?;
            f.set_len(0).await?;
        }

        let mut buffer = BytesMut::with_capacity(mdata.est_len());
        {
            let mut path = dir.clone();
            path.push("metadata");
            let mut f = fopt.open(&path).await?;
            f.set_len(0).await?;
            let mut buffer = BytesMut::with_capacity(mdata.est_len());
            mdata.serialize(&mut buffer);
            f.write_all(&buffer).await?;
        }
        {    
            let mut path = dir.clone();
            path.push("will");
            if path.exists() {
                tokio::fs::remove_file(&path).await?;
            }
            
            if let Some(w) = will {
                let mut f = fopt.open(&path).await?;
                unsafe {buffer.set_len(0)}
                w.serialize(&mut buffer);
                f.write_all(&buffer).await?;
            }
        }
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

// async fn write_metadata(writer: &mut BufWriter<File>, mdata: &MetaData) -> io::Result<usize> {
//     let mut buf = BytesMut::with_capacity(15);
//     mdata.expr_interval;

//     map.iter().for_each(|(each, slvl)| {
//         buf.put_u8(slvl.code());
//         buf.put(each.as_bytes());
//         buf.put_u8(0x0A);
//     });

//     writer.write_all(&buf).await?;
//     writer.flush().await?;
//     Ok(est_leng)
// }


// #[cfg(test)]
// mod tests {
//     use crate::protocol::v5::{subscribe::Subscribe, ServiceLevel};

//     use super::{ClientID, ClientStore};

//     #[tokio::test]
//     async fn log_path() {
//         let clid = ClientID::new("raw_clid".to_owned());
//         let l = ClientStore::new(&clid).await;
//         let logs = l.unwrap();
//         tokio::fs::remove_dir(logs.path).await.unwrap();
//     }

//     #[tokio::test]
//     async fn append() {
//         let clid = ClientID::new("raw_clid".to_owned());
//         let l = ClientStore::new(&clid).await;
//         let logs = l.unwrap();
        
//         let v = vec![
//             Subscribe{
//                 max_qos: ServiceLevel::QoS1,
//                 topic: "topic/a".to_owned()
//             },Subscribe{
//                 max_qos: ServiceLevel::QoS1,
//                 topic: "topic/b".to_owned()
//             }, Subscribe{
//                 max_qos: ServiceLevel::QoS1,
//                 topic: "topic/c".to_owned()
//             },
//         ];
//         let subs_log = logs.clone();
//         subs_log.subscribe(&v)
//             .await
//             .unwrap();
    
//         let p = [v[2].topic.clone()];
//         logs.unsubscribe(&p).await.unwrap();
//     }
// }