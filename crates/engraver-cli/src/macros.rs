/// Conditionally print based on silent mode
macro_rules! print_if {
    ($silent:expr, $($arg:tt)*) => {
        if !$silent {
            print!($($arg)*);
        }
    };
}

/// Conditionally println based on silent mode
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
