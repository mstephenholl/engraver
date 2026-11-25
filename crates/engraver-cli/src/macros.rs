/// Print unless `$silent`, which hides status output (`-q`, `--silent`, or
/// `quiet = true` in the configuration file)
macro_rules! print_if {
    ($silent:expr, $($arg:tt)*) => {
        if !$silent {
            print!($($arg)*);
        }
    };
}

/// Print a line unless `$silent`, as for `print_if!`
macro_rules! println_if {
    ($silent:expr) => {
        if !$silent {
            println!();
        }
    };
    ($silent:expr, $($arg:tt)*) => {
        if !$silent {
            println!($($arg)*);
        }
    };
}
