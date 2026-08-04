macro_rules! choose {
    ($left:expr, $right:expr) => {
        if $left { $right } else { 0 }
    };
}

pub fn run(enabled: bool) -> i32 {
    choose!(enabled, 7)
}
