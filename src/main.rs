use linux_futex::{Futex, Private};
use ndarray::Array2;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::fs;
use std::num::ParseIntError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn split_to_words(raw: &str) -> Vec<(&str, usize)> {
    let mut words = Vec::new();
    let mut last = 0;

    for (index, matched) in
        raw.match_indices(|c: char| !(c.is_alphanumeric()) && c != '-' && c != '_')
    {
        if last != index {
            words.push((&raw[last..index], last));
        }
        if !matched.trim().is_empty() {
            words.push((matched, index));
        }
        last = index + matched.len();
    }

    if last < raw.len() {
        words.push((&raw[last..], last));
    }

    words
}

#[derive(Debug)]
struct ParseError {
    err: String,
    off: usize,
}

fn check(eq: bool, off: usize, err: &str) -> Result<(), ParseError> {
    if eq {
        Ok(())
    } else {
        Err(ParseError {
            err: err.to_string(),
            off: off,
        })
    }
}

#[derive(PartialEq, Debug)]
enum ParserState {
    InFunction,
    InRepeat,
    InStatement,
    InBlock,
    InArgs,
}

#[derive(PartialEq, Debug)]
enum InstType {
    Invalid,
    Compute,
    Sleep,
    Spawn,
    Stop,
    Join,
    Wake,
    Wait,
    Repeat,
}

impl Default for InstType {
    fn default() -> InstType {
        InstType::Invalid
    }
}

fn parse_arg<T: std::str::FromStr<Err = ParseIntError>>(
    n: usize,
    i: &Instruction,
) -> Result<T, ParseError> {
    match (i.raw_args[n].0).parse::<T>() {
        Ok(v) => Ok(v),
        Err(err) => {
            return Err(ParseError {
                err: err.to_string(),
                off: i.raw_args[0].1,
            })
        }
    }
}

#[derive(PartialEq, Debug)]
struct ComputeArgs {
    size: usize,
    loops: usize,
}

fn parse_compute_args(i: &mut Instruction) -> Result<(), ParseError> {
    check(
        i.raw_args.len() == 2,
        i.off,
        "compute() takes 2 arguments: loops and matrix size",
    )?;

    i.compute_args = Some(ComputeArgs {
        size: parse_arg(0, i)?,
        loops: parse_arg(1, i)?,
    });

    Ok(())
}

fn compute(args: &ComputeArgs) {
    let size = args.size;
    let loops = args.loops;

    let mut a = Array2::<f64>::zeros((size, size));
    let mut b = Array2::<f64>::zeros((size, size));
    let mut c = Array2::<f64>::zeros((size, size));

    for _ in 0..loops {
        for i in 0..size {
            for j in 0..size {
                a[[i, j]] = (i * j) as f64;
                b[[i, j]] = (i + j) as f64;
            }
        }

        for i in 0..size {
            for j in 0..size {
                for k in 0..size {
                    c[[i, j]] = a[[i, k]] + b[[k, j]];
                }
            }
        }
    }
}

#[derive(PartialEq, Debug)]
struct SleepArgs {
    ms: u64,
}

fn parse_sleep_args(i: &mut Instruction) -> Result<(), ParseError> {
    check(i.raw_args.len() == 1, i.off, "sleep() takes 1 argument")?;

    i.sleep_args = Some(SleepArgs {
        ms: parse_arg(0, i)?,
    });

    Ok(())
}

fn sleep(args: &SleepArgs) {
    thread::sleep(Duration::from_micros(args.ms));
}

#[derive(PartialEq, Debug)]
struct SpawnArgs {
    handle: String,
    count: u32,
    func: String,
}

fn parse_spawn_args(i: &mut Instruction) -> Result<(), ParseError> {
    check(
        i.raw_args.len() == 3,
        i.off,
        "spawn() takes 3 arguments: handle, count, func",
    )?;

    i.spawn_args = Some(SpawnArgs {
        handle: i.raw_args[0].0.clone(),
        count: parse_arg(1, i)?,
        func: i.raw_args[2].0.clone(),
    });

    Ok(())
}

fn spawn(args: &SpawnArgs, prog: &Arc<Prog>) {
    for _ in 0..args.count {
        let prog2 = prog.clone();
        let func = args.func.clone();

        let handle = thread::spawn(move || {
            exec(&prog2, &func);
        });

        let user_handle = &args.handle;
        let mut hash = prog.threads.lock().unwrap();
        let arr = hash.entry(user_handle.to_string()).or_insert(vec![]);
        arr.push(handle);
    }
}

#[derive(PartialEq, Debug)]
struct StopArgs {}

fn parse_stop_args(i: &mut Instruction) -> Result<(), ParseError> {
    check(i.raw_args.len() == 0, i.off, "stop() takes 0 arguments")?;

    Ok(())
}

fn stop(prog: &Arc<Prog>) {
    prog.stop.store(true, Ordering::Relaxed);
    for (_, futex) in &prog.futexes {
        futex.wake(i32::MAX);
    }
}

#[derive(PartialEq, Debug)]
struct JoinArgs {
    handle: String,
}

fn parse_join_args(i: &mut Instruction) -> Result<(), ParseError> {
    check(
        i.raw_args.len() == 1,
        i.off,
        "join() takes 1 argument: handle",
    )?;

    i.join_args = Some(JoinArgs {
        handle: i.raw_args[0].0.clone(),
    });

    Ok(())
}

fn join(args: &JoinArgs, prog: &Arc<Prog>, func: &str) {
    let mut threads = prog.threads.lock().unwrap();
    let (_, vec) = threads.remove_entry(&args.handle).unwrap();
    if prog.debug_locking {
        println!("{}(): join {}", func, args.handle);
    }
    for handle in vec {
        handle.join().unwrap();
    }
}

#[derive(PartialEq, Debug)]
struct WakeArgs {
    handle: String,
    count: i32,
}

fn parse_wake_args(i: &mut Instruction) -> Result<(), ParseError> {
    check(
        i.raw_args.len() == 2,
        i.off,
        "wake() takes 2 argument: handle, count",
    )?;

    i.wake_args = Some(WakeArgs {
        handle: i.raw_args[0].0.clone(),
        count: parse_arg(1, i)?,
    });

    Ok(())
}

fn wake(args: &WakeArgs, prog: &Arc<Prog>, func: &str) {
    if prog.debug_locking {
        println!("{}(): wake {}", func, args.handle);
    }
    prog.futexes[&args.handle].wake(args.count);
}

#[derive(PartialEq, Debug)]
struct WaitArgs {
    handle: String,
}

fn parse_wait_args(i: &mut Instruction) -> Result<(), ParseError> {
    check(
        i.raw_args.len() == 1,
        i.off,
        "wait() takes 1 argument: handle",
    )?;

    i.wait_args = Some(WaitArgs {
        handle: i.raw_args[0].0.clone(),
    });

    Ok(())
}

fn wait(args: &WaitArgs, prog: &Arc<Prog>, func: &str) {
    if prog.debug_locking {
        println!("{}(): prepare to wait on {}", func, args.handle);
    }
    let _ = prog.futexes[&args.handle].wait(0);
    if prog.debug_locking {
        println!("{}(): woken up", func);
    }
}

#[derive(PartialEq, Debug)]
struct RepeatArgs {
    instr: usize,
    counter: i64,
}

fn parse_repeat_args(i: &mut Instruction) -> Result<(), ParseError> {
    check(
        i.raw_args.len() == 1,
        i.off,
        "repeat() takes 1 argument: number of iterations",
    )?;

    let counter = parse_arg(0, i)?;

    match &mut i.repeat_args {
        None => panic!(),
        Some(args) => args.counter = counter,
    }

    Ok(())
}

fn repeat(args: &RepeatArgs, ip: usize, repeats: &mut HashMap<usize, i64>) -> usize {
    if args.counter == -1 {
        return args.instr;
    } else {
        let counter = repeats.entry(ip).or_insert(args.counter);
        *counter -= 1;

        if *counter == 0 {
            repeats.remove(&ip);
            ip
        } else {
            args.instr
        }
    }
}

#[derive(PartialEq, Debug, Default)]
struct Instruction {
    op: InstType,
    raw_args: Vec<(String, usize)>,
    off: usize,
    compute_args: Option<ComputeArgs>,
    sleep_args: Option<SleepArgs>,
    spawn_args: Option<SpawnArgs>,
    stop_args: Option<StopArgs>,
    join_args: Option<JoinArgs>,
    wake_args: Option<WakeArgs>,
    wait_args: Option<WaitArgs>,
    repeat_args: Option<RepeatArgs>,
}

struct Prog {
    functions: HashMap<String, Vec<Instruction>>,
    threads: Mutex<HashMap<String, Vec<thread::JoinHandle<()>>>>,
    futexes: HashMap<String, Futex<Private>>,
    stop: AtomicBool,
    verbose: bool,
    debug_locking: bool,
}

fn parse_sb(raw: &str) -> Result<Prog, ParseError> {
    let words = split_to_words(&raw);
    let mut state: Vec<ParserState> = Vec::new();
    let mut prog = Prog {
        functions: HashMap::new(),
        threads: Mutex::new(HashMap::new()),
        futexes: HashMap::new(),
        stop: AtomicBool::new(false),
        verbose: false,
        debug_locking: false,
    };
    let mut curr_func_name: Option<&str> = None;
    let mut curr_func: Vec<Instruction> = Vec::new();
    let mut curr_inst = Instruction {
        ..Default::default()
    };
    let mut repeat_inst: Vec<Instruction> = Vec::new();

    for (word, index) in words {
        if word == "fn" {
            check(
                state.len() == 0,
                index,
                "nested function definitions are not supported",
            )?;
            state.push(ParserState::InFunction);
        } else if word == "repeat" {
            check(
                state.len() >= 2 && state[state.len() - 1] == ParserState::InBlock,
                index,
                "repeat blocks should be nested into functions and/or other repeat blocks",
            )?;
            for i in (0..state.len() - 1).rev() {
                if state[i] == ParserState::InBlock {
                    continue;
                }
                if state[i] == ParserState::InFunction {
                    break;
                }
            }
            check(
                state[0] == ParserState::InFunction,
                index,
                "a repeat block can't be outside a function",
            )?;
            state.push(ParserState::InRepeat);
            repeat_inst.push(Instruction {
                op: InstType::Repeat,
                off: index,
                repeat_args: Some(RepeatArgs {
                    instr: curr_func.len(),
                    counter: 0,
                }),
                ..Default::default()
            });
        } else if word == "{" {
            check(
                state.first() == Some(&ParserState::InFunction),
                index,
                "a {{}} block can't be outside a function",
            )?;
            state.push(ParserState::InBlock);
        } else if word == "(" {
            check(
                state.first() == Some(&ParserState::InFunction),
                index,
                "a () block can't be outside a function",
            )?;
            state.push(ParserState::InArgs);
        } else if word == ")" {
            let last = state.pop();
            check(
                last == Some(ParserState::InArgs),
                index,
                "mismatched () parentness",
            )?;
        } else if word == "}" {
            let last = state.pop();
            check(
                last == Some(ParserState::InBlock),
                index,
                "mismatched {{}} parentness",
            )?;
            let last = state.pop();
            check(
                last == Some(ParserState::InRepeat) || last == Some(ParserState::InFunction),
                index,
                "mismatched {{}} parentness",
            )?;
            if last == Some(ParserState::InRepeat) {
                curr_func.push(repeat_inst.pop().unwrap());
            }
        } else if word == "," {
            let last = state.last();
            check(
                last == Some(&ParserState::InArgs),
                index,
                "a comma can't be outside of argument list",
            )?;
        } else if word == ";" {
            let last = state.pop();
            check(
                last == Some(ParserState::InStatement),
                index,
                "a semicolon can't be outside of a block",
            )?;
            curr_func.push(curr_inst);
            curr_inst = Instruction {
                ..Default::default()
            };
        } else {
            if state.last() == Some(&ParserState::InFunction) {
                if curr_func_name.is_some() {
                    prog.functions
                        .insert(curr_func_name.unwrap().to_string(), curr_func);
                }
                curr_func_name = Some(word);
                curr_func = Vec::new();
            } else if state.last() == Some(&ParserState::InBlock) {
                state.push(ParserState::InStatement);

                curr_inst.op = match word {
                    "compute" => InstType::Compute,
                    "sleep" => InstType::Sleep,
                    "join" => InstType::Join,
                    "spawn" => InstType::Spawn,
                    "stop" => InstType::Stop,
                    "wake" => InstType::Wake,
                    "wait" => InstType::Wait,
                    _ => InstType::Invalid,
                };

                check(
                    curr_inst.op != InstType::Invalid,
                    index,
                    "invalid instruction",
                )?;

                curr_inst.off = index;
            } else if state.last() == Some(&ParserState::InArgs) {
                if curr_inst.op == InstType::Invalid {
                    let off = repeat_inst.len() - 1;
                    let instr = &mut repeat_inst[off];
                    instr.raw_args.push((word.to_string(), index));
                } else {
                    curr_inst.raw_args.push((word.to_string(), index));
                }
            }
        }
    }

    if curr_func_name.is_some() {
        prog.functions
            .insert(curr_func_name.unwrap().to_string(), curr_func);
    }

    assert!(repeat_inst.len() == 0);

    check(
        prog.functions.contains_key("main"),
        usize::MAX,
        "main() function is missing",
    )?;

    Ok(prog)
}

fn nice_err(raw: &str, err: ParseError) -> String {
    let off = if err.off > raw.len() {
        raw.len()
    } else {
        err.off
    };
    let mut line = 0;
    for c in raw[..off].chars() {
        if c == '\n' {
            line += 1;
        }
    }

    let before = &raw[..off];
    let after = &raw[off..];

    let before = match before.rfind("\n") {
        Some(off) => &before[off + 1..],
        None => before,
    };

    let mut pad: String = "".to_string();
    for c in before.chars() {
        match c {
            '\t' => pad.push('\t'),
            _ => pad.push(' '),
        }
    }

    let after = match after.find("\n") {
        Some(off) => &after[..off],
        None => after,
    };

    format!(
        "Error at line {}:\n{}{}\n{}^\n\"{}\"",
        line, before, after, pad, err.err
    )
}

fn parse_sb_file(path: &str) -> Result<Prog, Box<dyn Error>> {
    let raw = fs::read_to_string(path)?;
    let mut prog = match parse_sb(&raw) {
        Ok(p) => p,
        Err(e) => return Err(nice_err(&raw, e).into()),
    };

    // parse arguments
    for f in &mut prog.functions {
        for i in f.1 {
            let ret = match i.op {
                InstType::Invalid => panic!(),
                InstType::Compute => parse_compute_args(i),
                InstType::Spawn => parse_spawn_args(i),
                InstType::Stop => parse_stop_args(i),
                InstType::Repeat => parse_repeat_args(i),
                InstType::Sleep => parse_sleep_args(i),
                InstType::Join => parse_join_args(i),
                InstType::Wake => parse_wake_args(i),
                InstType::Wait => parse_wait_args(i),
            };

            match ret {
                Ok(_) => {}
                Err(e) => return Err(nice_err(&raw, e).into()),
            }

            i.raw_args.clear();
        }
    }

    // check wait/wake handles, create futexes
    let mut wake_handles: HashSet<String> = HashSet::new();
    let mut wait_handles: HashSet<String> = HashSet::new();

    for f in &mut prog.functions {
        for i in f.1 {
            match i.op {
                InstType::Wake => wake_handles.insert(i.wake_args.as_ref().unwrap().handle.clone()),
                InstType::Wait => wait_handles.insert(i.wait_args.as_ref().unwrap().handle.clone()),
                _ => false,
            };
        }
    }

    if wake_handles == wait_handles {
        for handle in wake_handles {
            prog.futexes.insert(handle, Futex::new(0));
        }
    } else {
        return Err("wake() and wait() handles do not match".into());
    }

    Ok(prog)
}

fn exec(prog: &Arc<Prog>, func: &str) {
    let mut ip = 0;
    let mut repeats: HashMap<usize, i64> = HashMap::new();
    let len = prog.functions[func].len();
    loop {
        if prog.stop.load(Ordering::Relaxed) {
            break;
        }
        let inst = &prog.functions[func][ip];
        match inst.op {
            InstType::Compute => compute(&inst.compute_args.as_ref().unwrap()),
            InstType::Spawn => spawn(&inst.spawn_args.as_ref().unwrap(), prog),
            InstType::Join => join(&inst.join_args.as_ref().unwrap(), prog, func),
            InstType::Repeat => ip = repeat(&inst.repeat_args.as_ref().unwrap(), ip, &mut repeats),
            InstType::Sleep => sleep(&inst.sleep_args.as_ref().unwrap()),
            InstType::Stop => stop(prog),
            InstType::Wake => wake(&inst.wake_args.as_ref().unwrap(), prog, func),
            InstType::Wait => wait(&inst.wait_args.as_ref().unwrap(), prog, func),
            InstType::Invalid => panic!(),
        }
        ip += 1;
        if ip == len {
            break;
        }
    }
}

fn exec_prog(prog: &Arc<Prog>) {
    exec(&prog, "main");

    let mut threads = prog.threads.lock().unwrap();

    if prog.debug_locking {
        println!("main(): final join");
    }

    for (_, vec) in threads.drain() {
        for handle in vec {
            handle.join().unwrap();
        }
    }
}

fn process_timings(data: &[f64]) -> f64 {
    let n = data.len();
    let avg: f64 = data.iter().sum::<f64>() / n as f64;

    let mut s = 0.0;
    for &v in data {
        s += (v - avg) * (v - avg);
    }
    let stddev = (s / (n - 1) as f64).sqrt();
    let sigma3 = 3.0 * stddev;
    let pct = 100.0 * (sigma3) / avg;

    println!(
        "n {} average {:<10.0} ± {:<10.0} (± {:.1}%)",
        n, avg, sigma3, pct
    );

    pct
}

fn calibrate_compute() {
    let mut loops = 1;
    let mut size = 1;

    // size, loops
    let mut timings: HashMap<u64, HashMap<u64, u64>> = HashMap::new();
    let mut loops_set: BTreeSet<u64> = BTreeSet::new();
    let mut sizes_set: BTreeSet<u64> = BTreeSet::new();

    'outer: loop {
        'inner: loop {
            let now = Instant::now();
            compute(&ComputeArgs {
                loops: loops,
                size: size,
            });
            let elapsed = now.elapsed().as_micros();

            let entry = timings.entry(size as u64).or_insert(HashMap::new());
            entry.insert(loops as u64, elapsed as u64);

            loops_set.insert(loops as u64);
            sizes_set.insert(size as u64);

            if elapsed <= 5000000 {
                loops *= 2;
            } else {
                if loops > 1 {
                    loops = 1;
                    break 'inner;
                } else {
                    break 'outer;
                }
            }
        }

        size *= 2;
    }

    let mut str = format!("{:<4}: ", "size");
    for loops in &loops_set {
        str += &format!(" {:8}", loops);
    }
    println!("{}", str);

    for size in &sizes_set {
        let mut str = format!("{:<4}: ", size);

        for loops in &loops_set {
            if timings.contains_key(size) {
                if timings[size].contains_key(loops) {
                    str += &format!(" {:8}", timings[size][loops]);
                    continue;
                }
            }

            str += &format!(" {:>8}", "N/A");
        }

        println!("{}", str);
    }
}

fn dump_prog(prog: &Prog) {
    for (func, instr) in prog.functions.iter() {
        println!("fn {}() {{", func);
        for (off, inst) in instr.iter().enumerate() {
            let args = match inst.op {
                InstType::Invalid => panic!(),
                InstType::Compute => format!("{:?}", inst.compute_args),
                InstType::Sleep => format!("{:?})", inst.sleep_args),
                InstType::Spawn => format!("{:?}", inst.spawn_args),
                InstType::Stop => format!("{:?}", inst.stop_args),
                InstType::Join => format!("{:?}", inst.join_args),
                InstType::Wake => format!("{:?}", inst.wake_args),
                InstType::Wait => format!("{:?}", inst.wait_args),
                InstType::Repeat => format!("{:?}", inst.repeat_args),
            };
            println!("{:6}:  {:?} ({})", off, inst.op, args);
        }
        println!("}}\n");
    }
}

fn run_test(args: &Vec<String>, options: &Vec<String>) {
    let path = args.first().expect("");

    let prog = parse_sb_file(path);
    if let Err(e) = prog {
        println!("{}", e);
        return;
    }

    let mut prog = prog.unwrap();

    for opt in options.iter() {
        match opt.as_str() {
            "-v" => prog.verbose = true,
            "-l" => prog.debug_locking = true,
            _ => panic!("Invalid option specified. Supported: -v (verbose) -l (debug locking)"),
        }
    }

    if prog.verbose {
        dump_prog(&prog);
    }

    let prog = Arc::new(prog);

    let mut timings: Vec<f64> = vec![];
    let mut prev_pct = 0.0;
    let mut flushes = 0;
    loop {
        prog.stop.store(false, Ordering::Relaxed);
        let now = Instant::now();
        exec_prog(&prog);
        if prog.verbose {
            println!("{:?}", now.elapsed());
        }
        timings.push(now.elapsed().as_micros() as f64);
        if timings.len() % 10 == 0 {
            let pct = process_timings(&timings);
            // If the error goes down or at least is not growing
            // substantionally, go on for 100 measurements
            // Otherwise, restart, but not more than 10 times.
            if pct <= prev_pct + 3.0 {
                if timings.len() == 100 {
                    println!("----------------------------------------");
                    process_timings(&timings);
                    break;
                }
            } else {
                timings.drain(..);
                flushes += 1;
                if flushes >= 10 {
                    break;
                }
            }
            prev_pct = pct;
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
        "calibrate" => calibrate_compute(),
        "run" => run_test(&args, &options),
        _ => panic!("invalid command specified"),
    }
}
