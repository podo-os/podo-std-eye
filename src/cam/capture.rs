use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::queue::Queue;
use crate::common::VideoReader;
use crate::config::{Configurable, VideoColor, VideoMeta};
use crate::frame::Frame;

use chrono::prelude::*;
use opencv::prelude::*;
use opencv::videoio;
use podo_core_driver::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CamConfig {
    pub(crate) device: u16,
    pub(crate) export: Option<bool>,
    #[serde(flatten)]
    pub(crate) meta: VideoMeta,
}

impl Configurable for CamConfig {
    #[inline]
    fn filename(&self, _: &PathBuf) -> Result<String, RuntimeError> {
        Ok(format!("/dev/video{}", &self.device))
    }

    #[inline]
    fn meta(&self) -> &VideoMeta {
        &self.meta
    }

    #[inline]
    fn is_export(&self) -> bool {
        self.export.unwrap_or_default()
    }
}

struct Thread {
    camera: videoio::VideoCapture,
    color: VideoColor,

    queue: Arc<Queue>,
    alive: AliveFlag,
    us_per_frame: i64,
}

impl Thread {
    #[inline]
    fn new_thread<C>(
        queue: Arc<Queue>,
        alive: AliveFlag,
        config: &C,
        path: &PathBuf,
    ) -> Result<thread::JoinHandle<Result<(), RuntimeError>>, RuntimeError>
    where
        C: Configurable,
    {
        let (camera, color) = config.spawn(path)?;
        let us_per_frame = match config.meta().fps {
            0 => 0,
            _fps => (1_000_000_f64 / _fps as f64) as i64,
        };
        let this = Self {
            camera,
            color,
            queue,
            alive,
            us_per_frame,
        };
        let t = thread::spawn(move || this.inner_loop());
        Ok(t)
    }

    #[inline]
    fn inner_loop(self) -> Result<(), RuntimeError> {
        let color = self.color;
        let sync = self.us_per_frame > 0;
        let mut camera = self.camera;
        let result = loop {
            // normal shutdown
            if let false = self.alive.is_running() {
                break Ok(());
            }
            let timestamp = Utc::now();
            // unexpected shutdown
            if let Err(e) = self.queue.push_inner(
                |image| match camera.read(image as &mut Mat)? {
                    true => color.convert(&mut *image),
                    false => RuntimeError::expect("opencv::VideoCapture::read failed"),
                },
                timestamp,
                !sync,
            ) {
                break Err(e);
            }
            // spend unused time to sync
            if sync {
                let time_us = self.us_per_frame
                    - (Utc::now() - timestamp)
                        .num_microseconds()
                        .unwrap_or(self.us_per_frame);
                if time_us >= THRES_WAIT_US {
                    thread::sleep(Duration::from_micros((time_us - THRES_SKIP_US) as u64));
                }
            }
        };
        // graceful shutdown
        {
            self.alive.stop().ok();
            camera.release()?;
            drop(camera);
        }
        result
    }
}

const THRES_WAIT_US: i64 = 3_000;
const THRES_SKIP_US: i64 = 50;

pub struct VideoCapture<C>
where
    C: Configurable,
{
    queue: Arc<Queue>,
    alive: AliveFlag,
    thread: Mutex<Option<thread::JoinHandle<Result<(), RuntimeError>>>>,

    config: C,
    path: PathBuf,
}

impl<C> VideoCapture<C>
where
    C: Configurable,
{
    #[inline]
    pub fn from_config<P: AsRef<Path>>(config: C, path: P) -> Result<Self, RuntimeError> {
        let alive = AliveFlag::default();
        Ok(Self {
            queue: Arc::new(Queue::new(&alive, 2)?),
            alive,
            thread: Mutex::new(None),
            config,
            path: path.as_ref().to_path_buf(),
        })
    }
}

impl<C> VideoReader for VideoCapture<C>
where
    C: Configurable,
{
    fn start(&self) -> Result<(), RuntimeError> {
        self.alive.start()?;
        let t = Thread::new_thread(
            self.queue.clone(),
            self.alive.clone(),
            &self.config,
            &self.path,
        )?;
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
        self.config.is_export()
    }

    fn get(&self, frame: &mut Option<Frame>) -> Result<(), RuntimeError> {
        let frame = match frame.as_mut() {
            Some(frame) => frame,
            None => {
                frame.replace(Frame::new(self.config.meta().clone())?);
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

impl<C> Drop for VideoCapture<C>
where
    C: Configurable,
{
    fn drop(&mut self) {
        self.stop().unwrap()
    }
}
