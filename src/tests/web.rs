use crate::testlib::{Test, TestCtx};
use linux_futex::{Futex, Private};
use std::sync::Arc;
use std::sync::Mutex;

const REQUESTS: u32 = 100;

pub struct WebCtx {
    in_lock: Futex<Private>,
    out_lock: Futex<Private>,

    in_queue: Mutex<u32>,
    wk_queue: Mutex<u32>,
    out_queue: Mutex<u32>,
}

impl WebCtx {
    pub fn new() -> Self {
        WebCtx {
            in_lock: Futex::new(0),
            out_lock: Futex::new(0),

            in_queue: Mutex::new(REQUESTS),
            wk_queue: Mutex::new(0),
            out_queue: Mutex::new(0),
        }
    }
}

impl TestCtx for WebCtx {
    fn reset(&self) {
        self.in_lock.wake(i32::MAX);
        self.out_lock.wake(i32::MAX);

        *self.in_queue.lock().unwrap() = REQUESTS;
        *self.wk_queue.lock().unwrap() = 0;
        *self.out_queue.lock().unwrap() = 0;
    }

    fn main(self: &Arc<WebCtx>, test: &Arc<Test>) {
        test.spawn("input", 4, input_fn, self);
        test.spawn("workers", test.num_cpus(), worker_fn, self);
        test.spawn("output", 4, output_fn, self);

        test.join("output", "main");
    }
}

fn input_fn(test: &Arc<Test>, ctx: &Arc<WebCtx>) {
    while !test.stopped() {
        test.usleep(1000);

        let mut cnt = ctx.in_queue.lock().unwrap();
        if *cnt > 0 {
            *cnt -= 1;
        } else {
            break;
        }

        let mut cnt = ctx.wk_queue.lock().unwrap();
        *cnt += 1;

        test.wake(&ctx.in_lock, *cnt as i32, "input_fn");
    }
}

fn worker_fn(test: &Arc<Test>, ctx: &Arc<WebCtx>) {
    while !test.stopped() {
        let mut cnt = ctx.wk_queue.lock().unwrap();
        if *cnt > 0 {
            *cnt -= 1;
        } else {
            drop(cnt);
            test.wait(&ctx.in_lock, "worker_fn");
            continue;
        }

        drop(cnt);
        test.compute(32, 1024);

        let mut cnt = ctx.out_queue.lock().unwrap();
        *cnt += 1;

        test.wake(&ctx.out_lock, 1, "worker_fn");
    }
}

fn output_fn(test: &Arc<Test>, ctx: &Arc<WebCtx>) {
    while !test.stopped() {
        let cnt = ctx.out_queue.lock().unwrap();
        if *cnt == REQUESTS {
            break;
        } else if *cnt == 0 {
            drop(cnt);
            test.wait(&ctx.out_lock, "output_fn");
            continue;
        }
        drop(cnt);
        test.compute(8, 1024);
    }
}
