use crate::config::{Configurable, VideoMeta};

use podo_core_driver::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RtspConfig {
    pub(crate) url: String,
    #[serde(flatten)]
    pub(crate) meta: VideoMeta,
}

impl Configurable for RtspConfig {
    #[inline]
    fn filename(&self, _: &PathBuf) -> Result<String, RuntimeError> {
        Ok(self.url.clone())
    }

    #[inline]
    fn meta(&self) -> &VideoMeta {
        &self.meta
    }

    #[inline]
    fn is_export(&self) -> bool {
        false
    }
}
