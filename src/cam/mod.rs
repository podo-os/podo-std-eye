mod capture;
#[cfg(feature = "simple-socket")]
mod client;
mod queue;
mod rtsp;
mod video;

pub use self::capture::{CamConfig, VideoCapture};
#[cfg(feature = "simple-socket")]
pub use self::client::{ClientCapture, ClientConfig};
pub use self::rtsp::RtspConfig;
pub use self::video::VideoConfig;
