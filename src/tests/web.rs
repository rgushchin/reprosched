use std::sync::Arc;
use linux_futex::{Futex, Private};
use crate::testlib::{Test, TestCtx};

pub struct WebCtx {
    in_lock: Futex<Private>,
    out_lock: Futex<Private>,
}

impl WebCtx {
    pub fn new() -> Self {
	WebCtx {
	    in_lock: Futex::new(0),
	    out_lock: Futex::new(0),
	}
    }
}

impl TestCtx for WebCtx {
    fn reset(&self) {
	self.in_lock.wake(i32::MAX);
	self.out_lock.wake(i32::MAX);
    }
}

pub fn main(test: &Arc<Test>, ctx: &Arc<WebCtx>) {
    test.spawn("input", 4, input_fn, ctx);
    test.spawn("output", 4, output_fn, ctx);
    test.spawn("workers", 4, worker_fn, ctx);

    test.join("input", "main");
}

fn input_fn(test: &Arc<Test>, ctx: &Arc<WebCtx>) {
    for _ in 0..1000 {
        test.usleep(1000);
        test.wake(&ctx.in_lock, 1, "input_fn");
    }
}

fn worker_fn(test: &Arc<Test>, ctx: &Arc<WebCtx>) {
    while !test.stopped() {
        test.wait(&ctx.in_lock, "worker_fn");
        test.compute(64, 2048);
        test.wake(&ctx.out_lock, 1, "worker_fn");
    }
}

fn output_fn(test: &Arc<Test>, ctx: &Arc<WebCtx>) {
    while !test.stopped() {
        test.wait(&ctx.out_lock, "output_fn");
        test.compute(8, 1024);
    }
}
