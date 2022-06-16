pub trait Reader<'a, Value>
where
    Value: ?Sized,
{
    fn ask() -> &'a Value;
}
