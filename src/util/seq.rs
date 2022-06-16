pub trait Seq {
    type Output;

    fn next() -> Self::Output;
}

pub struct Const<const C: u32>;

impl<const C: u32> Seq for Const<C> {
    type Output = u32;

    fn next() -> Self::Output {
        C
    }
}

#[macro_export]
macro_rules! seq {
    ($tyname:ident, $innerty:ty, $init:expr, $next:expr) => {
        $crate::seq!($tyname, $innerty, $innerty, $init, $next)
    };

    ($vis:vis $tyname:ident, $wrappedty:ty, $innerty:ty, $init:expr, $next:expr) => {
        ::paste::paste! {
            $vis struct $tyname($wrappedty);

            ::lazy_static::lazy_static! {
                static ref [< __ $tyname _INSTANCE >]: $tyname = $tyname($init);
            }

            impl $crate::util::Seq for $tyname {
                type Output = $innerty;

                fn next() -> Self::Output {
                    let val: &$wrappedty = &[< __ $tyname _INSTANCE >].0;
                    ($next)(val)
                }
            }
        }
    };
}

#[macro_export]
macro_rules! atomic_seq {
    ($vis:vis $name:ident) => {
        $crate::atomic_seq!($vis $name, ::std::sync::atomic::AtomicU8, u8);
    };

    ($vis:vis $name:ident, $aty:ty, $uty:ty) => {
        $crate::seq!($vis $name, $aty, $uty, { <$aty>::new(0) }, |atomic: &$aty| {
            let mut old = atomic.load(::std::sync::atomic::Ordering::Acquire);

            loop {
                match atomic.compare_exchange_weak(old, old + 1, ::std::sync::atomic::Ordering::Release, ::std::sync::atomic::Ordering::Relaxed) {
                    Ok(_) => return old,
                    Err(x) => old = x,
                }
            }
        });
    };
}
