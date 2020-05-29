use std::collections::HashMap;
use std::path::Path;

use crate::cam::{CamConfig, VideoCapture, VideoConfig};
use crate::common::{ArcVideoReader, VideoReader};

use opencv::imgproc::*;
use opencv::prelude::*;
use opencv::videoio;
use opencv::videoio::VideoCaptureTrait;
use podo_core_driver::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config(pub(crate) HashMap<String, OneConfig>);

#[derive(Debug, Deserialize)]
pub enum OneConfig {
    Cam(CamConfig),
    Video(VideoConfig),
}

impl OneConfig {
    pub(crate) fn spawn<P: AsRef<Path>>(self, path: P) -> Result<ArcVideoReader, RuntimeError> {
        let reader: Box<dyn VideoReader> = match self {
            crate::config::OneConfig::Cam(config) => {
                Box::new(VideoCapture::from_config(config, path)?)
            }
            crate::config::OneConfig::Video(config) => {
                Box::new(VideoCapture::from_config(config, path)?)
            }
        };
        Ok(reader.into())
    }
}

pub trait Configurable: Send + Sync {
    fn filename(&self, path: &PathBuf) -> Result<String, RuntimeError>;
    fn meta(&self) -> &VideoMeta;

    #[inline]
    fn spawn(&self, path: &PathBuf) -> Result<(videoio::VideoCapture, VideoColor), RuntimeError> {
        let preference = videoio::CAP_ANY;
        let meta = self.meta();

        let mut camera = videoio::VideoCapture::from_file(&self.filename(path)?, preference)?;
        {
            if let Some(codec) = meta.codec.as_ref() {
                if let 4 = codec.len() {
                    let codec = unsafe { &*(codec.as_bytes() as *const [u8] as *const [i8]) };
                    let (c1, c2, c3, c4) = (codec[0], codec[1], codec[2], codec[3]);
                    camera.set(
                        videoio::CAP_PROP_FOURCC,
                        videoio::VideoWriter::fourcc(c1, c2, c3, c4)?.into(),
                    )?;
                }
            }

            camera.set(videoio::CAP_PROP_FRAME_WIDTH, meta.width.into())?;
            camera.set(videoio::CAP_PROP_FRAME_HEIGHT, meta.height.into())?;
            camera.set(videoio::CAP_PROP_FPS, meta.fps.into())?;
        }
        let color = meta.color.clone().unwrap_or_default();
        match camera.is_opened()? {
            true => Ok((camera, color)),
            false => RuntimeError::expect("Failed to open VideoCapture"),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct VideoMeta {
    pub(crate) codec: Option<String>,

    pub color: Option<VideoColor>,

    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

#[derive(Copy, Clone, Debug, Deserialize)]
pub enum VideoColor {
    Grayscale,
    Color, // BGR
}

impl VideoColor {
    pub fn convert(&self, mut image: &mut Mat) -> Result<(), RuntimeError> {
        match self {
            Self::Grayscale => match image.channels()? {
                1 => Ok(()),
                3 => {
                    let origin = Mat::copy(image)?;
                    cvt_color(&origin, &mut image, COLOR_BGR2GRAY, 0)?;
                    Ok(())
                }
                _ => RuntimeError::unimplemented(),
            },
            Self::Color => match image.channels()? {
                1 => {
                    let origin = Mat::copy(image)?;
                    cvt_color(&origin, &mut image, COLOR_GRAY2BGR, 0)?;
                    Ok(())
                }
                3 => Ok(()),
                _ => RuntimeError::unimplemented(),
            },
        }
    }
}

impl Default for VideoColor {
    #[inline]
    fn default() -> Self {
        Self::Color
    }
}
