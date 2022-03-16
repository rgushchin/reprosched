use linux_futex::{Futex, Private};
use ndarray::Array2;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

pub struct Test {
    threads: Mutex<HashMap<String, Vec<thread::JoinHandle<()>>>>,
    futexes: HashMap<String, Futex<Private>>,
    pub stop: AtomicBool,
    pub verbose: bool,
    pub debug_locking: bool,
}

pub type ArcTest = Arc<Test>;

impl Test {
    pub fn stopped(&self) -> bool {
	self.stop.load(Ordering::Relaxed)
    }

    pub fn compute(&self, size: usize, loops: usize) {
	if self.stopped() {
	    return
	}
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

    pub fn spawn(self: &Arc<Test>,
        user_handle: &str,
        count: usize,
        func: fn(&Arc<Test>),
    ) {
	if self.stopped() {
	    return
	}
        for _ in 0..count {
            let self2 = self.clone();

            let handle = thread::spawn(move || {
                func(&self2);
            });

            let mut hash = self.threads.lock().unwrap();
            let arr = hash.entry(user_handle.to_string()).or_insert(vec![]);
            arr.push(handle);
        }
    }

    pub fn join(&self, handle: &str, caller: &str) {
	if self.stopped() {
	    return
	}
        let mut threads = self.threads.lock().unwrap();
        let (_, vec) = threads.remove_entry(handle).unwrap();
        if self.debug_locking {
            println!("{}(): join {}", caller, handle);
        }
        for handle in vec {
            handle.join().unwrap();
        }
    }

    pub fn add_futex(&mut self, handle: &str) {
	self.futexes.insert(handle.to_string(), Futex::new(0));
    }

    pub fn wake(self: &Arc<Test>, handle: &str, count: i32, caller: &str) {
        if self.debug_locking {
            println!("{}(): wake {:?}", caller, handle);
        }
	let lock = &self.futexes[handle];
        lock.wake(count);
    }

    pub fn wait(&self, handle: &str, caller: &str) {
	if self.stopped() {
	    return
	}
        if self.debug_locking {
            println!("{}(): prepare to wait on {:?}", caller, handle);
        }
	let lock = &self.futexes[handle];
        lock.wait(0).unwrap();
        if self.debug_locking {
            println!("{}(): woken up", caller);
        }
    }

    pub fn usleep(&self, us: u64) {
	if self.stopped() {
	    return
	}
        thread::sleep(Duration::from_micros(us));
    }

    pub fn new() -> Self {
        Test {
            threads: Mutex::new(HashMap::new()),
	    futexes: HashMap::new(),
            stop: AtomicBool::new(false),
            verbose: false,
            debug_locking: false,
        }
    }

    pub fn stop(&self) {
	if self.debug_locking {
            println!("stop()");
        }

        self.stop.store(true, Ordering::Relaxed);
	for (_, futex) in &self.futexes {
	    futex.wake(i32::MAX);
	}
    }

    pub fn reset(&self) {
        self.stop.store(false, Ordering::Relaxed);
    }

    pub fn join_all_threads(&self) {
        let mut threads = self.threads.lock().unwrap();

        if self.debug_locking {
            println!("main(): join all threads");
        }

        for (_, vec) in threads.drain() {
            for handle in vec {
                handle.join().unwrap();
            }
        }
    }
}
