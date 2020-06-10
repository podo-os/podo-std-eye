use std::sync::Arc;

use podo_core_driver::*;
use podo_std_eye::EyeDriver;

#[no_mangle]
pub extern "C" fn make_driver(
    _: &dyn DriverRoot,
    path: &PathBuf,
    params: &DriverParams,
) -> Result<ArcDriver, RuntimeError> {
    let driver = EyeDriver::try_with_config_params(path, params)?;
    Ok(Arc::new(driver))
}
