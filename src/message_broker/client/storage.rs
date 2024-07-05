#![allow(dead_code)]
use std::{collections::HashMap, env, path::PathBuf};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio::{fs::{File, OpenOptions}, io::{self, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter}};
use crate::{helper::time::sys_now, protocol::v5::{connect, subscribe::Subscribe, ServiceLevel}};
use super::{clobj::ClientID, DATA_STORE};

/// Always clone when use, this case do for pass the borrow checker. 
/// 
/// Still unoptimized
#[derive(Debug, Clone)]
pub struct ClientStore {
    path: PathBuf,
}

impl ClientStore {
    pub(super) async fn new(clid: &ClientID, mdata: &MetaData) -> io::Result<Self> {
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
            path.push("session");
            let f = fopt.open(&path).await?;
            f.set_len(0).await?;

            let mut writer = BufWriter::new(f);
            write_wall(
                &mut writer, 
                &[WALL{time: sys_now(), value: EventType::ClientCreated}]
            ).await?;
        }

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
            mdata.serialize(&mut buffer);
            f.write_all(&buffer).await?;
        }

        Ok(Self { path: dir.to_owned() })
    }

    pub(super) async fn load(clid: &ClientID) -> io::Result<Restored> {
        let mut dir = env::current_dir()?;
        dir.push(DATA_STORE);
        dir.push(clid.to_string());
        if !dir.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("no session for client id {}", clid)));
        }

        let mut fopt = OpenOptions::new();
        fopt.read(true);
        let mut buffer = BytesMut::zeroed(512);

        {
            let mut path = dir.clone();
            path.push("session");
            if path.is_file() {
                let f = fopt.open(&path).await?;
                let mut reader = BufReader::new(f);
                let wall = read_wall(&mut reader, &mut buffer).await?;
                let f = reader.into_inner();

                for i in (0..wall.len()).rev() {
                    // TODO: return error when session already restored
                    if let EventType::SetExpired(u) =  wall[i].value {
                        let is_expired = sys_now() >= u;
                        if is_expired {
                           return Err(io::Error::new(io::ErrorKind::TimedOut, "session expired")); 
                        }
                        
                        let mut writer = BufWriter::new(f);
                        write_wall(
                            &mut writer, 
                            &[WALL{time: sys_now(), value: EventType::ClientRestored}]
                        ).await?;

                        break;
                    }
                }
            }
        }

        
        let mdata = {
            let mut path = dir.clone();
            path.push("metadata");
            let mut f = fopt.open(&path).await?;
            let read_mdata = f.read(&mut buffer).await?;
            if read_mdata < 13 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "metadata structure is invalid"));
            }
            let mut b = buffer.split_off(read_mdata);
            MetaData::deserialize(&mut b)
        };
        
        let subbed: Vec<Subscribe> = {
            let mut path = dir.clone();
            path.push("subscribed");
            let f = fopt.open(&path).await?;
            let mut reader = BufReader::new(f);
            let mut maps = HashMap::new();
            get_subscribed(&mut reader, &mut maps).await?;
            maps.into_iter().map(|(k, v)| {
                Subscribe{
                    topic: k,
                    max_qos: v
                }
            }).collect()
        };
        Ok(Restored{
            mdata,
            subs: subbed
        })
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

    pub async fn log_session(self, wall: &[WALL]) -> io::Result<()> {
        let mut path = self.path;
        path.push("session");

        let mut fopt = OpenOptions::new();
        let f = fopt.append(true).open(path).await?;
        let mut writer = BufWriter::new(f);
        write_wall(&mut writer, wall).await?;
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

async fn get_subscribed(reader: &mut BufReader<File>, map: &mut HashMap<String, ServiceLevel>) -> io::Result<usize> {
    let mut buf = BytesMut::zeroed(1024);
    let mut readed = 0;

    // TODO: check when looping
    while let Ok(n) = reader.read(&mut buf).await {
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

async fn write_subscribed(writer: &mut BufWriter<File>, map: &mut HashMap<String, ServiceLevel>) -> io::Result<usize> {
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

    writer.write_all(&buf).await?;
    writer.flush().await?;
    Ok(est_leng)
}

pub(super) struct MetaData {
    pub(super) protocol_level: u8,
    pub(super) keep_alive_interval: u16,
    pub(super) expr_interval: u32,
    pub(super) receive_maximum: u16,
    pub(super) maximum_packet_size: u16,
    pub(super) topic_alias_maximum: u16,
    pub(super) user_properties: Vec<(String, String)>,
}

impl MetaData {
    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_u8(self.protocol_level);
        buffer.put_u16(self.keep_alive_interval);
        buffer.put_u32(self.expr_interval);
        buffer.put_u16(self.receive_maximum);
        buffer.put_u16(self.maximum_packet_size);
        buffer.put_u16(self.topic_alias_maximum);
        let prop_len = self.est_user_prop();

        // user_properties leng
        buffer.put_u32(prop_len as u32);

        // user_properties
        if prop_len > 1 {
            for up in &self.user_properties {
                serialize_string(buffer, &up.0);
                serialize_string(buffer, &up.1);
                buffer.put_u8(0x0A);
            }
        }

        // will leng
        // buffer.put_u32(self.est_will() as u32);

        // // will
        // if let Some(will) = &self.will {
        //     will.serialize(buffer);
        // }
    }

    fn est_user_prop(&self) -> usize {
        let mut prop_len = 0;
        self.user_properties.iter().for_each(|(k, v)|{
            prop_len += 2 + k.len() + 2 + v.len() + 1
        });
        prop_len
    }

    fn est_len(&self) -> usize {
        // base val | user_properties leng | user_properties 
        13 + 4 + self.est_user_prop()
    }

    // fn est_will(&self) -> usize {
    //     self.will.as_ref().map(|w| w.est_len()).unwrap_or_default()
    // }

    fn deserialize(buffer: &mut BytesMut) -> Self {
        let mut new = Self {
            protocol_level: buffer.get_u8(),
            keep_alive_interval: buffer.get_u16(),
            expr_interval: buffer.get_u32(),
            receive_maximum: buffer.get_u16(),
            maximum_packet_size: buffer.get_u16(),
            topic_alias_maximum: buffer.get_u16(),
            user_properties:  Vec::new(),
        };

        if buffer.len() < 2 {
            return new;
        }
        
        let uprop_len = buffer.get_u32();
        if buffer.len() < uprop_len as usize {
            return new;
        }
        
        new.user_properties = {
            let uprop_buf = buffer.split_to(uprop_len as usize);
            let mut prop = Vec::new();
            while uprop_buf.len() != 0 {
                prop.push((
                    deserialize_string(buffer),
                    deserialize_string(buffer)
                ));
                let _lf = buffer.get_u8();
            }
            prop
        };

        buffer.clear();
        new
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
        buffer.put(self.payload.as_slice());
    }

    fn est_len(&self) -> usize {
        2 + self.topic.len() + self.payload.len()
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

pub struct Restored {
    pub(super) mdata: MetaData,
    // pub(super) will: Option<Will>,
    pub(super) subs: Vec<Subscribe>
}

#[derive(Debug)]
enum EventType {
    ClientCreated,
    ClientDisconnected,
    DisconnectByServer,
    ClientRestored,
    SetExpired(u64),
}

pub struct WALL {
    time: u64,
    value: EventType,
}

impl WALL {
    fn deserialize(b: &mut Bytes) -> io::Result<Self> {
        let mut time = b.split_to(5);
        time.advance(1);
        let time = time.get_u64();
        let value = unsafe{String::from_utf8_unchecked(b.to_vec())};
        let value = value.trim();
        let mut part = value.split_ascii_whitespace();

        let part1 = part.next()
            .ok_or(
                io::Error::new(
                    io::ErrorKind::InvalidData,
                     "not registered eventt"
                )
            )?;

        let value = match part1 {
            "ClientCreated" => EventType::ClientCreated,
            "ClientDisconnected" => EventType::ClientDisconnected,
            "DisconnectByServer" => EventType::DisconnectByServer,
            "ClientRestored" => EventType::ClientRestored,
            "SetExpired" => EventType::SetExpired({
                let part2 = part.next().ok_or(io::Error::new(io::ErrorKind::InvalidData, "no value for set expiration"))?;
                part2.parse::<u64>().map_err(|_|io::Error::new(io::ErrorKind::InvalidData, "invalid expiration number"))?
            }),
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "not registered eventt"))
        };

        Ok(Self { time, value })
    }

    fn serialize(&self, buffer: &mut BytesMut) {
        buffer.put_u8(0x5B);
        buffer.put_u64(self.time);
        buffer.put_u8(0x5D);
        let val = match self.value {
            EventType::ClientCreated => "ClientCreated",
            EventType::ClientDisconnected => "ClientDisconnected",
            EventType::DisconnectByServer => "DisconnectByServer",
            EventType::ClientRestored => "ClientRestored",
            EventType::SetExpired(u) => &format!("SetExpired {}", u)
        };

        buffer.put(val.as_bytes());
        buffer.put_u8(0x0A);
    }
}

async fn read_wall(reader: &mut BufReader<File>, buffer: &mut BytesMut) -> io::Result<Vec<WALL>> {
    let mut wall = Vec::new();
    while let Ok(n) = reader.read(buffer).await {
        if n == 0 {
            break;
        }

        let read = buffer.split_to(n);
        
        for i in 0..read.len() {
            if read[i] != 0x5B {
                continue;
            }

            'j: for (j, v) in read.iter().enumerate() {
                if v == &0x0A {
                    let b = read[i..j].to_vec();
                    let mut b = Bytes::from(b);
                    wall.push(WALL::deserialize(&mut b)?);
                    break 'j;
                }
            }
            
        }
        buffer.clear();
    }    
    Ok(wall)
}

async fn write_wall(writer: &mut BufWriter<File>, wall: &[WALL]) -> io::Result<()> {
    let mut buffer = BytesMut::with_capacity(wall.len() * 64);

    for w in wall.iter() {
        w.serialize(&mut buffer);
    }

    writer.write_all(&buffer).await?;
    writer.flush().await?;
    Ok(())
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