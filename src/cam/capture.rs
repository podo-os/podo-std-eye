use std::cell::UnsafeCell;
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, RwLock,
};
use std::thread;
use std::time::Duration;

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
}

struct VideoCaptureThread {
    camera: videoio::VideoCapture,
    color: VideoColor,

    queue: Arc<VideoCaptureQueue>,
    running: AliveFlag,
    us_per_frame: i64,
}

impl VideoCaptureThread {
    #[inline]
    fn new_thread<C>(
        queue: Arc<VideoCaptureQueue>,
        running: AliveFlag,
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
            running,
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
            if let false = self.running.is_running() {
                break Ok(());
            }
            let timestamp = Utc::now();
            // unexpected shutdown
            if let Err(e) = self.queue.push_inner(
                |mat| match camera.read(mat)? {
                    true => color.convert(mat),
                    false => RuntimeError::expect("opencv::VideoCapture::read failed"),
                },
                timestamp,
                sync,
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
            self.running.stop().ok();
            camera.release()?;
            drop(camera);
        }
        result
    }
}

const THRES_WAIT_US: i64 = 3_000;
const THRES_SKIP_US: i64 = 50;

type VideoCaptureQueueBuffer = UnsafeCell<Vec<RwLock<(Mat, DateTime<Utc>)>>>;

struct VideoCaptureQueue {
    running: AliveFlag,
    buffer: VideoCaptureQueueBuffer,
    ptr: AtomicUsize,
    ptr_next_comsumed: AtomicUsize,
    size: usize,
}

unsafe impl Send for VideoCaptureQueue {}
unsafe impl Sync for VideoCaptureQueue {}

impl VideoCaptureQueue {
    #[inline]
    fn new(running: &AliveFlag, size: usize) -> Result<Self, RuntimeError> {
        Ok(Self {
            running: running.clone(),
            buffer: UnsafeCell::new(vec![]),
            ptr: AtomicUsize::new(0),
            ptr_next_comsumed: AtomicUsize::new(0),
            size,
        })
    }

    #[inline]
    fn push_inner<F>(
        &self,
        mut f: F,
        timestamp: DateTime<Utc>,
        sync: bool,
    ) -> Result<(), RuntimeError>
    where
        F: FnMut(&mut Mat) -> Result<(), RuntimeError>,
    {
        let ptr = self.ptr.load(Ordering::Relaxed);
        if !sync {
            let buffer_usable = self.size - 1;
            'sync: loop {
                let ptr_next_comsumed = self.ptr_next_comsumed.load(Ordering::Relaxed);
                // usable
                if ptr < ptr_next_comsumed + buffer_usable {
                    break 'sync;
                }
                // not yet
                thread::yield_now();
            }
        }

        let ptr = ptr % self.size;
        let buffer = unsafe { self.buffer.get().as_mut().unwrap() };
        match buffer.get(ptr) {
            Some(entity) => {
                let (mat, ts) = &mut *entity.write().unwrap();
                *ts = timestamp;
                f(mat)?;
            }
            None => {
                let mut mat = Mat::default()?;
                f(&mut mat)?;
                let entity = RwLock::new((mat, timestamp));
                buffer.insert(ptr, entity);
            }
        }
        self.ptr.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    #[inline]
    fn pop_inner(&self, frame: &mut Frame) -> Result<(), RuntimeError> {
        let buffer_usable = self.size - 1;
        let count_frame = frame.count;

        let ptr = loop {
            self.running.assert_running()?;
            let count_now = self.ptr.load(Ordering::Relaxed);
            // exceed the buffer
            if count_now > count_frame + buffer_usable {
                break count_now - buffer_usable;
            }
            // in the buffer
            if count_now > count_frame {
                break count_frame;
            }
            // not yet
            thread::yield_now();
        };
        self.ptr_next_comsumed.store(ptr + 1, Ordering::Relaxed);

        let buffer = unsafe { self.buffer.get().as_ref().unwrap() };
        let entity = buffer.get(ptr % self.size).unwrap();
        let (mat, timestamp) = &*entity.read().unwrap();
        mat.copy_to(&mut frame.data)?;
        frame.timestamp = *timestamp;
        frame.count = ptr + 1;
        Ok(())
    }
}

pub struct VideoCapture<C>
where
    C: Configurable,
{
    queue: Arc<VideoCaptureQueue>,
    running: AliveFlag,
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
        let running = AliveFlag::default();
        Ok(Self {
            queue: Arc::new(VideoCaptureQueue::new(&running, 2)?),
            running,
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
        self.running.start()?;
        let t = VideoCaptureThread::new_thread(
            self.queue.clone(),
            self.running.clone(),
            &self.config,
            &self.path,
        )?;
        self.thread.lock().unwrap().replace(t);
        Ok(())
    }

    #[inline]
    fn stop(&self) -> Result<(), RuntimeError> {
        self.running.stop().ok();
        match self.thread.lock().unwrap().take() {
            Some(thread) => match thread.join() {
                Ok(res) => res,
                Err(_) => RuntimeError::unexpected(),
            },
            None => RuntimeError::expect("VideoReader is not started yet"),
        }
    }

    fn get(&self, frame: &mut Option<Frame>) -> Result<(), RuntimeError> {
        let frame = match frame.as_mut() {
            Some(frame) => frame,
            None => {
                frame.replace(Frame::new(self.config.meta().clone())?);
                frame.as_mut().unwrap()
            }
        };
        match self.running.is_running() {
            true => self.queue.pop_inner(frame),
            false => match self.stop() {
                Ok(()) => RuntimeError::unreachable(),
                Err(e) => Err(e),
            },
        }
    }
}
