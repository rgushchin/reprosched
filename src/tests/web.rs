use crate::testlib::{Test, ArcTest};

pub fn init(test: &mut Test) {
    test.add_futex("in_lock");
    test.add_futex("out_lock");
}

pub fn main(test: &ArcTest) {
    test.spawn("input", 4, input_fn);
    test.spawn("output", 4, output_fn);
    test.spawn("workers", 4, worker_fn);

    test.join("input", "main");

    test.stop();
}

fn input_fn(test: &ArcTest) {
    for _ in 0..1000 {
        test.usleep(1000);
        test.wake("in_lock", 1, "input_fn");
    }
}

fn worker_fn(test: &ArcTest) {
    while !test.stopped() {
        test.wait("in_lock", "worker_fn");
        test.compute(64, 2048);
        test.wake("out_lock", 1, "worker_fn");
    }
}

fn output_fn(test: &ArcTest) {
    while !test.stopped() {
        test.wait("out_lock", "output_fn");
        test.compute(8, 1024);
    }
}
