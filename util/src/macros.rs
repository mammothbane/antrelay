#[macro_export]
macro_rules! bootstrap {
    ($x:expr $( , $xs:expr )* $(,)?) => {
        eprintln!(concat!("[bootstrap] ", $x) $( , $xs )*)
    };
}

#[macro_export]
macro_rules! trace_catch {
    (parent: $parent:expr, $val:expr, $($rest:tt)*) => {
        if let Err(ref e) = $val {
            ::tracing::error!(parent: $parent, error = %e, $($rest)*);
        }
    };

    ($val:expr, $($rest:tt)*) => {
        if let Err(ref e) = $val {
            ::tracing::error!(error = %e, $($rest)*);
        }
    };
}
