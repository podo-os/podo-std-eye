use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::thread;

use crate::common::ArcVideoReader;
use crate::frame::Frame;

use podo_core_driver::{AliveFlag, RuntimeError};
use serde::{Deserialize, Serialize};
use simple_socket::{PostServing, SocketServer};

pub struct EyeExportServerHandler {
    alive: AliveFlag,
    busy: AliveFlag,
    nodes: BTreeMap<String, ArcVideoReader>,
    inner: Mutex<Option<thread::JoinHandle<Result<(), RuntimeError>>>>,
}

impl EyeExportServerHandler {
    pub fn new(nodes: &BTreeMap<String, ArcVideoReader>) -> Self {
        Self {
            alive: AliveFlag::new(false),
            busy: AliveFlag::new(false),
            nodes: nodes
                .iter()
                .filter(|(_, r)| r.is_export())
                .map(|(n, r)| (n.clone(), r.clone()))
                .collect(),
            inner: Mutex::new(None),
        }
    }
}

impl EyeExportServerHandler {
    pub fn is_running(&self) -> bool {
        self.alive.is_running()
    }

    pub fn is_busy(&self) -> bool {
        self.busy.is_running()
    }

    pub fn start(&self) -> Result<(), RuntimeError> {
        if self.alive.is_running() || self.nodes.is_empty() {
            return Ok(());
        }

        let count = self.nodes.keys().map(|n| (n.clone(), 0)).collect();

        let server = EyeExportServer {
            alive: self.alive.clone(),
            busy: self.busy.clone(),
            count,
            inner: self.nodes.clone(),
        };

        let thread = thread::spawn(move || server.run());

        self.alive.start()?;
        self.inner.lock().unwrap().replace(thread);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), RuntimeError> {
        self.alive.stop().ok();
        match self.inner.lock().unwrap().take() {
            Some(thread) => thread.join().unwrap(),
            None => Ok(()),
        }
    }
}

impl Drop for EyeExportServerHandler {
    fn drop(&mut self) {
        self.alive.stop().ok();

        if let Some(thread) = self.inner.get_mut().unwrap().take() {
            thread.join().unwrap().unwrap();
        }
    }
}

pub struct EyeExportServer {
    alive: AliveFlag,
    busy: AliveFlag,

    count: BTreeMap<String, usize>,
    inner: BTreeMap<String, ArcVideoReader>,
}

impl EyeExportServer {
    fn run(mut self) -> Result<(), RuntimeError> {
        const IP_V4: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);
        const IP: IpAddr = IpAddr::V4(IP_V4);

        let socket = SocketAddr::new(IP, PORT);
        let backlog = Default::default();
        let server = SocketServer::try_new(socket, backlog)?;

        let alive = self.alive.clone();
        let busy = self.busy.clone();

        let handler = |req: EyeRequest| {
            let reader = match self.inner.get(&req.reader) {
                Some(reader) => reader,
                None => return EyeResponse::NoSuchReader(req.reader),
            };

            match req.typ {
                EyeRequestType::Start => {
                    *self.count.get_mut(&req.reader).unwrap() += 1;
                    reader.start().ok();
                    EyeResponse::Awk
                }
                EyeRequestType::Stop => {
                    *self.count.get_mut(&req.reader).unwrap() -= 1;
                    if self.count[&req.reader] == 0 {
                        reader.stop().ok();
                    }
                    EyeResponse::Awk
                }
                EyeRequestType::Get => {
                    let mut buffer = None;
                    match reader.get(&mut buffer) {
                        Ok(()) => EyeResponse::Frame(Ok(buffer.unwrap())),
                        Err(e) => EyeResponse::Frame(Err(format!("{:?}", e))),
                    }
                }
            }
        };

        server.run(handler, |server| {
            if server.has_connections() {
                busy.start().ok();
            } else {
                busy.stop().ok();
            }

            if alive.is_running() {
                PostServing::Yield
            } else {
                PostServing::Stop
            }
        })?;

        self.busy.stop().ok();
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct EyeRequest {
    pub reader: String,
    pub typ: EyeRequestType,
}

#[derive(Serialize, Deserialize)]
pub enum EyeRequestType {
    Start,
    Stop,
    Get,
}

#[derive(Serialize, Deserialize)]
pub enum EyeResponse {
    Frame(Result<Frame, String>),
    NoSuchReader(String),
    Awk,
}

pub const PORT: u16 = 9804;
