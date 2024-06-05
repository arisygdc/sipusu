use std::{fs::File, io::{self, BufReader}, net::SocketAddr, path::{Path, PathBuf}, sync::Arc};
use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio::{net::{TcpListener, TcpStream, ToSocketAddrs}, task::JoinHandle};
use tokio_rustls::{rustls::{pki_types::{CertificateDer, PrivateKeyDer}, ServerConfig}, TlsAcceptor};
use crate::connection::handler::Proxy;

pub const TLS_CERT: &str = "/var/test_host/cert.pem";
pub const TLS_KEY: &str = "/var/test_host/key.pem";

pub struct Server {
    cert: Option<CertificatePath>,
    handler: Proxy,
}

impl Server {
    pub async fn new(cert: Option<CertificatePath>, handler: Proxy) -> Self {
        Self { cert, handler }
    }

    pub fn bind<A>(
        self,
        addr: A
    ) -> io::Result<JoinHandle<io::Result<()>>> 
    where
        A: ToSocketAddrs + Send + Sync + 'static
    {
        let handler = self.handler;
        let c_loader = match &self.cert {
            Some(cert) => cert, 
            None => return Ok(tokio::spawn(Self::bind_unsecure(addr, handler)))
        };

        let certs = c_loader.load_certs()?;
        let key = c_loader.load_keys()?;

        let tls_config = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)   
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

        let join_handle = tokio::spawn(Self::bind_secure(addr, handler, tls_config));
        Ok(join_handle)
    }

    async fn bind_secure<A, W>(
        addr: A,
        wire: W,
        tls_config: ServerConfig,
    ) -> io::Result<()> 
        where 
            A: ToSocketAddrs + Send,
            W: Wire + Send
    {
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let listener = TcpListener::bind(&addr).await?;
        
        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let acceptor = acceptor.clone();
            
            println!("[stream] incoming");
            wire.connect_with_tls(stream, peer_addr, acceptor).await;
        }
    }   

    async fn bind_unsecure<A, W>(
        addr: A,
        wire: W,
    ) -> io::Result<()> 
        where 
            A: ToSocketAddrs + Send,
            W: Wire + Send
    {
        println!("[tcp] unsecure");
        let listener = TcpListener::bind(&addr).await?;
        loop {
            let (stream, peer_addr) = listener.accept().await?;
            
            println!("[stream] incoming");
            wire.connect(stream, peer_addr).await;
        }
    }
}

pub struct CertificatePath {
    cert: PathBuf,
    private_key: PathBuf
}

impl Default for CertificatePath {
    fn default() -> Self {
        Self::new(TLS_CERT, TLS_KEY)
    }
}

impl CertificatePath {
    pub fn new(cert: &str, private_key: &str) -> Self {
        let cert = Path::new(&cert).to_owned(); 
        let private_key = Path::new(&private_key).to_owned();
        CertificatePath { cert, private_key }
    }

    fn load_certs(&self) -> io::Result<Vec<CertificateDer<'static>>> {
        let path = Path::new(&self.cert);
        certs(&mut BufReader::new(File::open(path)?)).collect()
    }

    fn load_keys(&self) -> io::Result<PrivateKeyDer<'static>> {
        let path = Path::new(&self.private_key);
        pkcs8_private_keys(&mut BufReader::new(File::open(path)?))
            .next()
            .unwrap()
            .map(Into::into)
    }
}

pub trait Wire {
    fn connect_with_tls(&self, stream: TcpStream, addr: SocketAddr, tls: TlsAcceptor) -> impl std::future::Future<Output = ()> + Send;
    fn connect(&self, stream: TcpStream, addr: SocketAddr) -> impl std::future::Future<Output = ()> + Send;
}