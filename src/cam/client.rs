use std::net::SocketAddr;
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::thread;

use super::queue::Queue;
use crate::common::VideoReader;
use crate::config::VideoMeta;
use crate::export::{EyeRequest, EyeRequestType, EyeResponse, PORT};
use crate::frame::Frame;

use podo_core_driver::*;
use serde::Deserialize;
use simple_socket::SocketClient;

#[derive(Debug, Deserialize)]
pub struct ClientConfig {
    pub(crate) ip: String,
}

struct Thread {
    queue: Arc<Queue>,
    alive: AliveFlag,

    meta: mpsc::Sender<VideoMeta>,

    name: String,
    client: SocketClient<EyeRequest, EyeResponse>,
}

impl Thread {
    #[inline]
    fn new_thread(
        queue: Arc<Queue>,
        alive: AliveFlag,
        meta: mpsc::Sender<VideoMeta>,
        name: &str,
        config: &ClientConfig,
    ) -> Result<thread::JoinHandle<Result<(), RuntimeError>>, RuntimeError> {
        let ip = config.ip.parse()?;

        let socket = SocketAddr::new(ip, PORT);
        let client = SocketClient::try_new(socket)?;

        let this = Self {
            queue,
            alive,
            meta,
            name: name.to_string(),
            client,
        };
        let t = thread::spawn(move || this.inner_loop());
        Ok(t)
    }

    #[inline]
    fn inner_loop(mut self) -> Result<(), RuntimeError> {
        if let EyeResponse::NoSuchReader(name) = self.client.request(&EyeRequest {
            reader: self.name.clone(),
            typ: EyeRequestType::Start,
        })? {
            return RuntimeError::message(format!("No such reader: {}", name));
        }

        let mut uninit_meta = true;
        let result = loop {
            // normal shutdown
            if let false = self.alive.is_running() {
                break Ok(());
            }

            let frame = match self.client.request(&EyeRequest {
                reader: self.name.clone(),
                typ: EyeRequestType::Get,
            })? {
                // unexpected shutdown
                EyeResponse::Frame(Ok(frame)) => frame,
                EyeResponse::Frame(Err(_)) => break RuntimeError::expect("Internal error"),
                _ => unreachable!(),
            };

            if uninit_meta {
                self.meta.send(frame.meta)?;
                uninit_meta = false;
            }

            let image = frame.image;
            let timestamp = frame.timestamp;
            if let Err(e) = self.queue.push_inner_inplace(image, timestamp, false) {
                break Err(e);
            }
        };

        // graceful shutdown
        self.alive.stop().ok();

        match self.client.request(&EyeRequest {
            reader: self.name.clone(),
            typ: EyeRequestType::Stop,
        }) {
            Ok(_) => result,
            Err(e) => Err(e.into()),
        }
    }
}

pub struct ClientCapture {
    queue: Arc<Queue>,
    alive: AliveFlag,
    thread: Mutex<Option<thread::JoinHandle<Result<(), RuntimeError>>>>,

    meta: RwLock<Option<VideoMeta>>,

    name: String,
    config: ClientConfig,
}

impl ClientCapture {
    pub fn from_config(config: ClientConfig, name: &str) -> Result<Self, RuntimeError> {
        let alive = AliveFlag::default();
        Ok(Self {
            queue: Arc::new(Queue::new(&alive, 2)?),
            alive,
            thread: Mutex::new(None),
            meta: RwLock::new(None),
            name: name.to_string(),
            config,
        })
    }
}

impl ClientCapture {
    fn get_meta(&self) -> VideoMeta {
        loop {
            {
                if let Some(meta) = &*self.meta.read().unwrap() {
                    break meta.clone();
                }
            }
            std::thread::yield_now();
        }
    }
}

impl VideoReader for ClientCapture {
    fn start(&self) -> Result<(), RuntimeError> {
        self.alive.start()?;

        let (tx, rx) = mpsc::channel();

        let t = Thread::new_thread(
            self.queue.clone(),
            self.alive.clone(),
            tx,
            &self.name,
            &self.config,
        )?;

        let meta = rx.recv()?;
        *self.meta.write().unwrap() = Some(meta);

        self.thread.lock().unwrap().replace(t);
        Ok(())
    }

    #[inline]
    fn stop(&self) -> Result<(), RuntimeError> {
        self.alive.stop().ok();
        match self.thread.lock().unwrap().take() {
            Some(thread) => match thread.join() {
                Ok(res) => res,
                Err(_) => RuntimeError::unexpected(),
            },
            None => Ok(()),
        }
    }

    #[inline]
    fn is_running(&self) -> bool {
        self.alive.is_running()
    }

    #[inline]
    fn is_export(&self) -> bool {
        false
    }

    fn get(&self, frame: &mut Option<Frame>) -> Result<(), RuntimeError> {
        let frame = match frame.as_mut() {
            Some(frame) => frame,
            None => {
                frame.replace(Frame::new(self.get_meta())?);
                frame.as_mut().unwrap()
            }
        };
        match self.alive.is_running() {
            true => self.queue.pop_inner(frame),
            false => match self.stop() {
                Ok(()) => unreachable!(),
                Err(e) => Err(e),
            },
        }
    }
}

impl Drop for ClientCapture {
    fn drop(&mut self) {
        self.stop().unwrap()
    }
}
