use std::collections::btree_map::{Keys, Values};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::config::Config;
use crate::frame::Frame;

use podo_core_driver::*;

pub type ArcVideoReader = Arc<dyn VideoReader>;

pub trait VideoReader: Send + Sync {
    fn start(&self) -> Result<(), RuntimeError>;
    fn stop(&self) -> Result<(), RuntimeError>;

    fn is_running(&self) -> bool;

    fn get(&self, old: &mut Option<Frame>) -> Result<(), RuntimeError>;
}

pub struct EyeDriver {
    inner: BTreeMap<String, ArcVideoReader>,
}

impl From<BTreeMap<String, ArcVideoReader>> for EyeDriver {
    fn from(inner: BTreeMap<String, ArcVideoReader>) -> Self {
        Self { inner }
    }
}

impl Into<BTreeMap<String, ArcVideoReader>> for EyeDriver {
    fn into(self) -> BTreeMap<String, ArcVideoReader> {
        self.inner
    }
}

impl<'a> Into<BTreeMap<String, ArcVideoReader>> for &'a EyeDriver {
    fn into(self) -> BTreeMap<String, ArcVideoReader> {
        self.inner.clone()
    }
}

impl EyeDriver {
    #[inline]
    pub fn get(&self, name: &str) -> Option<&ArcVideoReader> {
        self.inner.get(name)
    }

    #[inline]
    pub fn names(&self) -> Keys<String, ArcVideoReader> {
        self.inner.keys()
    }

    #[inline]
    pub fn readers(&self) -> Values<String, ArcVideoReader> {
        self.inner.values()
    }
}

impl Driver for EyeDriver {
    fn status(&self) -> Result<DriverState, RuntimeError> {
        if self.inner.values().any(|r| r.is_running()) {
            Ok(DriverState::Running(DriverRunningState::default()))
        } else {
            Ok(DriverState::Idle)
        }
    }
}

impl EyeDriver {
    pub fn try_with_config<P: AsRef<Path>>(path: P) -> Result<Self, RuntimeError> {
        let params = serde_yaml::from_str(&fs::read_to_string(&path)?)?;
        let path = path.as_ref().parent().unwrap().to_path_buf();
        Self::try_with_config_params(path, &params)
    }

    pub(crate) fn try_with_config_params<P: AsRef<Path>>(
        path: P,
        params: &DriverParams,
    ) -> Result<Self, RuntimeError> {
        let driver = serde_yaml::from_value::<Config>(params.clone())?
            .0
            .into_iter()
            .map(|(name, config)| Ok((name, config.spawn(&path)?)))
            .collect::<Result<BTreeMap<_, _>, RuntimeError>>()?;
        Ok(EyeDriver::from(driver))
    }
}
