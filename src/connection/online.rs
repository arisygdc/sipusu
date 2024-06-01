// #![allow(unused)]
use std::{io, net::SocketAddr, sync::Arc};
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, sync::{mpsc, Mutex, MutexGuard}, task::JoinHandle};

use crate::server::SecuredStream;
type ConnectionHolders = Mutex<Vec<ConnectedLine>>;

pub struct Onlines {
    conns: Arc<ConnectionHolders>,
    rx: mpsc::Receiver<ConnectedLine>
}

impl Onlines {
    pub fn new() -> (Self, mpsc::Sender<ConnectedLine>) {
        let (tx, rx) = mpsc::channel(4);
        let conns = Arc::new(Mutex::new(Vec::with_capacity(8)));
        (Self { conns, rx }, tx)
    }

    pub async fn stream_connection(self) -> JoinHandle<()>  {
        let share_conns = self.conns.clone();
        let mut rx = self.rx;
        
        tokio::task::spawn(async move {
            while let Some(mut conn) = rx.recv().await {
                let mut writer = share_conns.lock().await;
                if let Err(e) = conn.write(b"[OK]").await {
                    eprintln!("[stream] {}", e.to_string());
                } else {
                    writer.push(conn);
                }
            }
        });

        let share_conns = self.conns.clone();
        let fut = async move {
            loop {
                let mut conns = share_conns.lock().await;
                let remover = iterate_stream(&mut conns).await;

                if remover.len() > 0 {
                    continue;
                }
                
                let remove_conn = share_conns.clone();
                tokio::task::spawn(async move {
                    let mut conns = remove_conn.lock().await;
                    for ridx in remover {
                        conns.remove(ridx);
                    }
                });
            }
        };

        tokio::spawn(fut)
    }
}

#[allow(dead_code)]
pub struct ConnectedLine {
    id: u32,
    socket: SecuredStream,
    addr: SocketAddr
}

impl ConnectedLine {
    pub fn new(id: u32, socket: SecuredStream, addr: SocketAddr) -> Self {
        Self { id, socket, addr }
    }

    pub async fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        self.socket.write(src).await
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>  {
        self.socket.read(buf).await
    }
}

/// return disconnected line
#[inline]
async fn iterate_stream<'grd>(conns: &mut MutexGuard<'grd, Vec<ConnectedLine>>) -> Vec<usize> {
    let mut remover = Vec::new();
    for (i, conn) in conns.iter_mut().enumerate() {
        let mut buf = BytesMut::new();
        match conn.read(&mut buf).await {
            Ok(0) => {
                // Connection was closed
                // remove_indices.push(i);
                remover.push(i)
            }
            Ok(n) => {
                // Handle received data
                println!("Received: {:?}", &buf[..n]);
            }
            Err(e) => {
                eprintln!("[listen] Error: {}", e);
                // remove_indices.push(i);
            }
        }
    }
    remover
}