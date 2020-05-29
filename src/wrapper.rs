use std::sync::Arc;

use crate::common::EyeDriver;

use podo_core_driver::*;

#[no_mangle]
pub extern "C" fn make_driver(
    _: &dyn DriverRoot,
    path: &PathBuf,
    params: &DriverParams,
) -> Result<ArcDriver, RuntimeError> {
    let driver = EyeDriver::try_with_settings_params(path, params)?;
    Ok(Arc::new(driver))
}
