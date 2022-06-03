#[macro_export]
macro_rules! stream_unwrap {
    (parent: $parent:expr, $($args:tt)*) => {
        |s| s.filter_map(move |x| {
            $crate::trace_catch!(parent: $parent, x, $($args)*);

            x.ok()
        })
    };

    ($($args:tt)*) => {
        |s| s.filter_map(move |x| {
            $crate::trace_catch!(x, $($args)*);

            x.ok()
        })
    };
}

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

#[macro_export]
macro_rules! trip {
    (noclone $tripwire:expr) => {
        move |s| {
            ::futures::stream::StreamExt::take_until(
                s,
                Box::pin(async move { $tripwire.recv().await }),
            )
        }
    };

    ($tripwire:expr) => {{
        let trip = $tripwire.clone();
        $crate::trip!(noclone(trip))
    }};
}

#[macro_export]
macro_rules! split {
    () => {
        |s| $crate::util::splittable_stream(s, 1024)
    };
}
