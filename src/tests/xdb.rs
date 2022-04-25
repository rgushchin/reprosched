use crate::testlib::{Test, TestCtx};
use std::sync::Arc;
use std::sync::Mutex;

const REQUESTS: u32 = 60000;

pub struct XdbCtx {
    requests: Mutex<u32>,
}

impl XdbCtx {
    pub fn new() -> Self {
        XdbCtx {
            requests: Mutex::new(REQUESTS),
        }
    }
}

impl TestCtx for XdbCtx {
    fn reset(&self) {
        *self.requests.lock().unwrap() = REQUESTS;
    }

    fn main(self: &Arc<XdbCtx>, test: &Arc<Test>) {
        test.spawn("writers", 10, writer_fn, self);

        test.join("writers", "main");
    }
}

fn writer_fn(test: &Arc<Test>, ctx: &Arc<XdbCtx>) {
    while !test.stopped() {
        let mut cnt = ctx.requests.lock().unwrap();
        if *cnt > 0 {
            *cnt -= 1;
        } else {
            break;
        }

        test.compute(8, 10);
        drop(cnt);
        test.compute(8, 10);
    }
}
