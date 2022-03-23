use std::collections::BTreeSet;
use std::collections::HashMap;
use std::time::Instant;

use crate::testlib::Test;

pub fn calibrate_compute() {
    let mut loops = 1;
    let mut size = 1;

    // size, loops
    let mut timings: HashMap<u64, HashMap<u64, u64>> = HashMap::new();
    let mut loops_set: BTreeSet<u64> = BTreeSet::new();
    let mut sizes_set: BTreeSet<u64> = BTreeSet::new();

    let test = Test::new();

    'outer: loop {
        'inner: loop {
            let now = Instant::now();
            test.compute(size, loops);
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
