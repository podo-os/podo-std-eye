use std::thread;

use podo_core_driver::RuntimeError;
use podo_std_eye::*;

#[test]
fn test_get_30_images_with_10_threads() -> Result<(), RuntimeError> {
    let driver = EyeDriver::try_with_config("assets/compound.yaml")?;

    for name in driver.names() {
        let reader = driver.get(name).unwrap();
        test_get_30_images_with_10_threads_per_reader(name, reader)?;
    }
    Ok(())
}

fn test_get_30_images_with_10_threads_per_reader(
    name: &str,
    reader: &ArcVideoReader,
) -> Result<(), RuntimeError> {
    reader.start()?;

    let mut ts = vec![];
    for _ in 0..10 {
        let reader = reader.clone();
        let t = thread::spawn::<_, Result<_, RuntimeError>>(move || {
            let mut buffer = None;
            let mut ts = vec![];
            for _ in 0..32 {
                reader.get(&mut buffer)?;
                // println!("{:?}", buffer);
                ts.push(buffer.as_ref().unwrap().timestamp);
            }
            Ok((ts[31] - ts[1]).num_milliseconds())
        });
        ts.push(t);
    }
    let num_threads_iters = (ts.len() * 30) as f64;
    let estimated_ms = ts
        .into_iter()
        .map(|t| Ok(t.join().unwrap()?))
        .collect::<Result<Vec<_>, RuntimeError>>()?
        .into_iter()
        .sum::<i64>() as f64;
    println!(
        "[{}] Elapsed FPS: {}",
        name,
        1000f64 * num_threads_iters / estimated_ms
    );
    reader.stop()
}
