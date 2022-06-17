pub trait State<T> {
    fn put(&mut self, val: T);
    fn get(&self) -> &T;
}
