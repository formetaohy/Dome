use dome_gpu::{Gpu, GpuRequest, PowerPreference};
use std::time::{Duration, Instant};

#[test]
fn a_destroyed_device_streams_its_loss_without_clearing_it() {
    let gpu = Gpu::open(&GpuRequest {
        power: PowerPreference::LowPower,
    })
    .unwrap_or_else(|error| panic!("no device reports its loss: {error}"));
    let mut first = gpu.events();
    gpu.device().destroy();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if first.poll().iter().any(|event| event.device_lost()) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "a destroyed device reports its loss"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut second = gpu.events();
    assert!(
        second.poll().iter().any(|event| event.device_lost()),
        "a device loss stays visible to a later cursor"
    );
}

#[test]
fn a_lost_device_is_not_reused() {
    let request = GpuRequest {
        power: PowerPreference::LowPower,
    };
    let lost = Gpu::open(&request)
        .unwrap_or_else(|error| panic!("no device opens before its loss: {error}"));
    lost.device().destroy();
    let recovered = Gpu::open(&request)
        .unwrap_or_else(|error| panic!("no device opens after its loss: {error}"));
    assert!(
        !recovered
            .events()
            .poll()
            .iter()
            .any(|event| event.device_lost()),
        "a replacement device does not inherit the loss of its predecessor"
    );
}
