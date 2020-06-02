use std::thread::{sleep, yield_now};
use std::time::Duration;

use podo_core_driver::{Driver, DriverRunningState, DriverState};
use podo_std_eye::EyeDriver;

fn main() {
    let server = EyeDriver::try_with_config("assets/server.yaml").unwrap();
    println!("Started a server.");

    {
        let client = EyeDriver::try_with_config("assets/client.yaml").unwrap();

        let client_reader = client.get("main").unwrap();
        client_reader.start().unwrap();
        println!("Started a client eye.");

        sleep(Duration::from_secs(1));

        let mut frame = None;
        for _ in 0..3 {
            client_reader.get(&mut frame).unwrap();
            let frame = frame.as_ref().unwrap();

            dbg!(&frame.meta);
        }
    }
    println!("The eye may be stopped.");

    while server.status().unwrap() != DriverState::Running(DriverRunningState::Lazy) {
        yield_now();
    }
    println!("Gracefully stopped client eye.");
}
