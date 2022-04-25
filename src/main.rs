use std::env;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

mod calibrate;
mod testlib;
mod tests;

use crate::testlib::TestCtx;

fn process_timings(data: &[f64]) -> (f64, f64, f64) {
    let n = data.len();
    let avg: f64 = data.iter().sum::<f64>() / n as f64;

    let mut s = 0.0;
    for &v in data {
        s += (v - avg) * (v - avg);
    }
    let stddev = (s / (n - 1) as f64).sqrt();
    let sigma3 = 3.0 * stddev;
    let pct = 100.0 * (sigma3) / avg;

    (avg, sigma3, pct)
}

fn run_test(test: testlib::Test, ctx: impl TestCtx) {
    let test = Arc::new(test);
    let ctx = Arc::new(ctx);

    let mut timings: Vec<f64> = vec![];
    let mut flushes = 0;
    let mut prev_pct = 100.0;
    loop {
        let now = Instant::now();
        ctx.main(&test);
        timings.push(now.elapsed().as_micros() as f64);

        test.stop();
        ctx.reset();
        test.join_all_threads();
        test.reset();

        let (avg, sigma3, pct) = process_timings(&timings);
        if timings.len() == 1 {
            println!("n {:2} {:10.0}", timings.len(), avg);
        } else {
            println!("n {:2} {:10.0} (± {:.1}%)", timings.len(), avg, pct);
        }

        // If the error is above 3%, restart, but not more than 10 times.
        // Otherwise, go on for 10 measurements.
        if pct > 3.0 {
            println!("--------------- discard ----------------");
            timings.drain(..);
            flushes += 1;
            if flushes >= 10 {
                println!("No result: data is too noisy");
                break;
            }
        }

        if (pct > prev_pct && timings.len() > 4) || timings.len() > 10 || pct < 0.5 {
            println!("========================================");
            println!(
                "Result: {:?} ± {:?} (± {:.1}%)",
                Duration::from_micros(avg as u64),
                Duration::from_micros(sigma3 as u64),
                pct
            );
            break;
        }

        prev_pct = pct;
    }
}

fn parse_test_args(args: &Vec<String>, options: &Vec<String>) {
    let mut test = testlib::Test::new();

    for opt in options.iter() {
        match opt.as_str() {
            "-v" => test.verbose = true,
            "-l" => test.debug_locking = true,
            _ => panic!("Invalid option specified. Supported: -v (verbose) -l (debug locking)"),
        }
    }

    for opt in args.iter() {
        match opt.as_str() {
            "web" => return run_test(test, tests::web::WebCtx::new()),
            "xdb" => return run_test(test, tests::xdb::XdbCtx::new()),
            _ => panic!("Invalid argument specified. Supported: web, xdb"),
        }
    }
}

fn syntax() {
    panic!("Syntax: reprosched <cmd: run, calibrate> [-option1] [-option2] [arg1] [arg2]");
}

fn main() {
    let argv: Vec<String> = env::args().collect();
    if argv.len() < 2 {
        syntax();
    }

    let mut cmd = String::new();
    let mut options: Vec<String> = vec![];
    let mut args: Vec<String> = vec![];

    for (i, v) in argv.iter().enumerate() {
        match i {
            0 => continue,
            1 => cmd = v.clone(),
            _ => {
                if v.starts_with("-") {
                    options.push(v.to_string());
                } else {
                    args.push(v.to_string());
                }
            }
        }
    }

    match cmd.as_str() {
        "calibrate" => calibrate::calibrate_compute(),
        "run" => parse_test_args(&args, &options),
        _ => panic!("invalid command specified"),
    }
}
