use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoopProxy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wake;

#[derive(Clone)]
pub struct Waker {
    proxy: Arc<Mutex<EventLoopProxy<Wake>>>,
}

impl Waker {
    pub(crate) fn new(proxy: EventLoopProxy<Wake>) -> Self {
        Self {
            proxy: Arc::new(Mutex::new(proxy)),
        }
    }

    pub fn wake(&self) {
        let proxy = self
            .proxy
            .lock()
            .expect("the waker of a window holds its event loop");
        let _ = proxy.send_event(Wake);
    }
}
