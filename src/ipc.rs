use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::net::UnixListener;
use tokio::sync::broadcast;
use anyhow::Result;

use crate::parser::Slide;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenterMessage {
    pub current_slide: usize,
    pub total_slides: usize,
    pub notes: Option<String>,
    pub title: Option<String>,
    pub timestamp: u64,
}

pub struct IpcServer {
    socket_path: PathBuf,
    sender: broadcast::Sender<PresenterMessage>,
}

impl IpcServer {
    pub fn new(socket_path: PathBuf) -> Self {
        let (sender, _) = broadcast::channel(100);
        Self { socket_path, sender }
    }

    pub fn sender(&self) -> broadcast::Sender<PresenterMessage> {
        self.sender.clone()
    }

    pub async fn run(&self) -> Result<()> {
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let sender = self.sender.clone();
                    tokio::spawn(async move {
                        use tokio::io::AsyncWriteExt;
                        
                        let mut receiver = sender.subscribe();
                        
                        while let Ok(msg) = receiver.recv().await {
                            if let Ok(json) = serde_json::to_string(&msg) {
                                let _ = stream.write_all(json.as_bytes()).await;
                                let _ = stream.write_all(b"\n").await;
                            }
                        }
                    });
                }
                Err(e) => {
                    log::error!("Accept error: {}", e);
                }
            }
        }
    }

    pub fn get_path(&self) -> &PathBuf {
        &self.socket_path
    }
}

pub struct IpcClient {
    socket_path: PathBuf,
}

impl IpcClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub async fn connect(&self) -> Result<PresenterStream> {
        Ok(PresenterStream::new(self.socket_path.clone()).await?)
    }
}

pub struct PresenterStream {
    receiver: broadcast::Receiver<PresenterMessage>,
}

impl PresenterStream {
    pub async fn new(socket_path: PathBuf) -> Result<Self> {
        use tokio::io::AsyncReadExt;
        
        let stream = tokio::net::UnixStream::connect(&socket_path).await?;
        let (sender, receiver) = broadcast::channel(100);
        
        tokio::spawn(async move {
            use tokio::io::BufReader;
            
            let mut reader = BufReader::new(stream).lines();
            
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(msg) = serde_json::from_str::<PresenterMessage>(&line) {
                    let _ = sender.send(msg);
                }
            }
        });

        Ok(Self { receiver })
    }

    pub async fn recv(&mut self) -> Option<PresenterMessage> {
        self.receiver.recv().await.ok()
    }
}

pub fn create_presenter_message(slide: &Slide, total: usize) -> PresenterMessage {
    PresenterMessage {
        current_slide: slide.index,
        total_slides: total,
        notes: slide.notes.clone(),
        title: slide.title.clone(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    }
}
