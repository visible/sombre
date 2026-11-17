use std::thread;
use std::time::Duration;

use crate::view;

pub fn watch() {
    thread::spawn(|| {
        let mut last = view::measured();
        loop {
            thread::sleep(Duration::from_millis(100));
            if sombre_harness::process::gone() {
                sombre_harness::process::bail();
            }
            let now = view::measured();
            if now == last {
                continue;
            }
            thread::sleep(Duration::from_millis(75));
            if view::measured() != now {
                continue;
            }
            last = now;
            view::resize();
        }
    });
}
