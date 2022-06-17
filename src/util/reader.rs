pub trait Reader<Value>
where
    Value: ?Sized,
{
    fn ask(&self) -> &Value;
}
