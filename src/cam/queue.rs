use std::cell::UnsafeCell;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    RwLock,
};
use std::thread;

use crate::frame::{Frame, Image};

use chrono::prelude::*;
use opencv::prelude::*;
use podo_core_driver::*;

type QueueBuffer = UnsafeCell<Vec<RwLock<(Image, DateTime<Utc>)>>>;

pub struct Queue {
    alive: AliveFlag,
    buffer: QueueBuffer,
    ptr: AtomicUsize,
    ptr_next_comsumed: AtomicUsize,
    size: usize,
}

unsafe impl Send for Queue {}
unsafe impl Sync for Queue {}

impl Queue {
    #[inline]
    pub fn new(alive: &AliveFlag, size: usize) -> Result<Self, RuntimeError> {
        Ok(Self {
            alive: alive.clone(),
            buffer: UnsafeCell::new(vec![]),
            ptr: AtomicUsize::new(0),
            ptr_next_comsumed: AtomicUsize::new(0),
            size,
        })
    }

    #[inline]
    pub fn push_inner<F>(
        &self,
        mut f: F,
        timestamp: DateTime<Utc>,
        sync: bool,
    ) -> Result<(), RuntimeError>
    where
        F: FnMut(&mut Image) -> Result<(), RuntimeError>,
    {
        let ptr = self.wait(sync) % self.size;
        let buffer = unsafe { self.buffer.get().as_mut().unwrap() };
        match buffer.get(ptr) {
            Some(entity) => {
                let (image, ts) = &mut *entity.write().unwrap();
                *ts = timestamp;
                f(image)?;
            }
            None => {
                let mut image = Image::try_default()?;
                f(&mut image)?;
                let entity = RwLock::new((image, timestamp));
                buffer.insert(ptr, entity);
            }
        }
        self.ptr.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    #[cfg(feature = "simple-socket")]
    #[inline]
    pub fn push_inner_inplace(
        &self,
        image: Image,
        timestamp: DateTime<Utc>,
        sync: bool,
    ) -> Result<(), RuntimeError> {
        let ptr = self.wait(sync) % self.size;
        let buffer = unsafe { self.buffer.get().as_mut().unwrap() };
        match buffer.get(ptr) {
            Some(entity) => {
                let (image_last, ts) = &mut *entity.write().unwrap();
                *image_last = image;
                *ts = timestamp;
            }
            None => {
                let entity = RwLock::new((image, timestamp));
                buffer.insert(ptr, entity);
            }
        }
        self.ptr.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn wait(&self, sync: bool) -> usize {
        let ptr = self.ptr.load(Ordering::Relaxed);
        if sync {
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
        ptr
    }

    #[inline]
    pub fn pop_inner(&self, frame: &mut Frame) -> Result<(), RuntimeError> {
        let buffer_usable = self.size - 1;
        let count_frame = frame.count;

        let ptr = loop {
            self.alive.assert_running()?;
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
        mat.copy_to(&mut *frame.image)?;
        frame.timestamp = *timestamp;
        frame.count = ptr + 1;
        Ok(())
    }
}
