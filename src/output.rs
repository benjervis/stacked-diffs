pub fn step(msg: &str) {
    eprintln!("\x1b[1;36m==>\x1b[0m {msg}");
}

pub fn info(msg: &str) {
    eprintln!("    {msg}");
}

pub fn ok(msg: &str) {
    eprintln!("\x1b[1;32m✓\x1b[0m {msg}");
}

pub fn warn(msg: &str) {
    eprintln!("\x1b[1;33m⚠\x1b[0m  {msg}");
}

pub fn err_print(msg: &str) {
    eprintln!("\x1b[1;31m✗\x1b[0m {msg}");
}
