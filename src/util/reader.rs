use hlist::Find;

pub trait Reader<Value>
where
    Value: ?Sized,
{
    fn ask(&self) -> &Value;
}

impl<X, I> Reader<X> for &dyn Find<X, I> {
    #[inline]
    fn ask(&self) -> &X {
        self.get()
    }
}
