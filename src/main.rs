use std::env;
use std::sync::Arc;
use std::time::Instant;

mod calibrate;
mod testlib;
mod tests;

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

fn run_test(args: &Vec<String>, options: &Vec<String>) {
    let mut test = testlib::Test::new();

    for opt in options.iter() {
        match opt.as_str() {
            "-v" => test.verbose = true,
            "-l" => test.debug_locking = true,
            _ => panic!("Invalid option specified. Supported: -v (verbose) -l (debug locking)"),
        }
    }
    tests::web::init(&mut test);

    let test = Arc::new(test);

    let mut timings: Vec<f64> = vec![];
    let mut flushes = 0;
    loop {
        test.reset();
        let now = Instant::now();
        tests::web::main(&test);
        test.join_all_threads();
        timings.push(now.elapsed().as_micros() as f64);

        let (avg, sigma3, pct) = process_timings(&timings);
        if timings.len() == 1 {
            println!("n {:2} {:10.0}", timings.len(), avg);
        } else {
            println!("n {:2} {:10.0} (± {:.1}%)", timings.len(), avg, pct);
        }

        // If the error is above 5%, restart, but not more than 10 times.
        // Otherwise, go on for 10 measurements.
        if pct > 5.0 {
            println!("--------------- discard ----------------");
            timings.drain(..);
            flushes += 1;
            if flushes < 10 {
                continue;
            } else {
                println!("No result: data is too noisy");
                break;
            }
        }

        if timings.len() == 10 {
            println!("========================================");
            println!("Result: {} ± {:<10.0} (± {:.1}%)", avg, sigma3, pct);
            break;
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
        "run" => run_test(&args, &options),
        _ => panic!("invalid command specified"),
    }
}
