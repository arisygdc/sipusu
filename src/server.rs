use std::{fs::File, io::{self, BufReader}, net::SocketAddr, path::{Path, PathBuf}, sync::Arc};
use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio::{net::{TcpListener, TcpStream, ToSocketAddrs}, task::JoinHandle};
use tokio_rustls::{rustls::{pki_types::{CertificateDer, PrivateKeyDer}, ServerConfig}, TlsAcceptor};

pub struct Server<H: Wire + Send + Sync + 'static> {
    cert: Option<CertificatePath>,
    handler: Arc<H>
}

impl<H: Wire + Send + Sync + 'static> Server<H> {
    pub fn new(handler: H, cert: Option<CertificatePath>) -> Self {
        let handler = Arc::new(handler);
        Self { cert, handler }
    }

    pub fn bind<A>(
        self,
        addr: A
    ) -> io::Result<JoinHandle<io::Result<()>>> 
    where
        A: ToSocketAddrs + Send + Sync + 'static
    {
        let c_loader = match &self.cert {
            Some(cert) => cert, 
            None => return Ok(tokio::spawn(self.bind_unsecure(addr)))
        };

        let certs = c_loader.load_certs()?;
        let key = c_loader.load_keys()?;

        let tls_config = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)   
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

        
        let join_handle = tokio::spawn(self.bind_secure(addr, tls_config));
        Ok(join_handle)
    }

    async fn bind_secure<A>(
        self,
        addr: A, 
        tls_config: ServerConfig
    ) -> io::Result<()> where A: ToSocketAddrs + Send {
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let listener = TcpListener::bind(&addr).await?;
        
        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let acceptor = acceptor.clone();
            
            println!("[stream] incoming");
            let handle: Arc<H> = self.handler.clone();
            handle.connect_with_tls(stream, peer_addr, acceptor).await;
        }
    }

    async fn bind_unsecure<A>(
        self,
        addr: A,
    ) -> io::Result<()> where A: ToSocketAddrs + Send {
        let listener = TcpListener::bind(&addr).await?;
        loop {
            let (stream, peer_addr) = listener.accept().await?;
            
            println!("[stream] incoming");
            let handle: Arc<H> = self.handler.clone();
            handle.connect(stream, peer_addr).await;
        }
    }
}

pub struct CertificatePath {
    cert: PathBuf,
    private_key: PathBuf
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