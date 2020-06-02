use crate::config::{Configurable, VideoMeta};

use podo_core_driver::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct VideoConfig {
    pub(crate) path: String,
    #[serde(flatten)]
    pub(crate) meta: VideoMeta,
}

impl Configurable for VideoConfig {
    #[inline]
    fn filename(&self, path: &PathBuf) -> Result<String, RuntimeError> {
        let mut path = path.clone();
        path.push(&self.path);
        match path.into_os_string().into_string() {
            Ok(path) => Ok(path),
            Err(e) => RuntimeError::expect_os(e),
        }
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
