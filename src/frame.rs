use crate::config::VideoMeta;

use chrono::{DateTime, Utc};
use opencv::prelude::Mat;
use podo_core_driver::RuntimeError;

#[derive(Debug)]
pub struct Frame {
    pub data: Mat,
    pub meta: VideoMeta,
    pub timestamp: DateTime<Utc>,

    pub(crate) count: usize,
}

impl Frame {
    pub fn new(meta: VideoMeta) -> Result<Self, RuntimeError> {
        Ok(Self {
            data: Mat::default()?,
            meta,
            timestamp: Utc::now(),
            count: 0,
        })
    }
}
