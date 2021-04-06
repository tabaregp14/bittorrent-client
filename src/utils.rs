#[macro_export]
macro_rules! println_thread {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);

        println!("Thread [{}]: {}", std::thread::current().name().unwrap(), msg);
    }
}
