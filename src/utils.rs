use percent_encoding::percent_encode_byte;

#[macro_export]
macro_rules! println_thread {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);

        println!("Thread [{}]: {}", std::thread::current().name().unwrap(), msg);
    }
}

pub fn url_encode(bytes: &Vec<u8>) -> String {
    bytes.into_iter()
        .map(|b| percent_encode_byte(*b))
        .collect::<String>()
}
