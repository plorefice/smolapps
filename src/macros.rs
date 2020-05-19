//! Utility macros.

#[cfg(feature = "log")]
#[macro_use]
mod log {
    #[allow(unused)]
    macro_rules! net_log {
        (trace, $($arg:expr),*) => { trace!($($arg),*); };
        (debug, $($arg:expr),*) => { debug!($($arg),*); };
    }
}

#[cfg(not(feature = "log"))]
#[macro_use]
mod log {
    #[allow(unused)]
    macro_rules! net_log {
        ($level:ident, $($arg:expr),*) => { $( let _ = $arg; )* }
    }
}

#[allow(unused)]
macro_rules! net_trace {
    ($($arg:expr),*) => (net_log!(trace, $($arg),*));
}

#[allow(unused)]
macro_rules! net_debug {
    ($($arg:expr),*) => (net_log!(debug, $($arg),*));
}
