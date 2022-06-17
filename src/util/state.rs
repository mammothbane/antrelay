use hlist::Find;

pub trait State<T> {
    fn put(&mut self, val: T);
    fn get(&self) -> &T;
}

impl<X, I> State<X> for &mut dyn Find<X, I> {
    #[inline]
    fn put(&mut self, val: X) {
        *(*self).get_mut() = val;
    }

    #[inline]
    fn get(&self) -> &X {
        Find::get(*self)
    }
}
