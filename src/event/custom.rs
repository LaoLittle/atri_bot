use crate::event::EventInner;

pub struct CustomEvent<T>(EventInner<T>);

impl<T> CustomEvent<T> {
    pub fn new(inner: T) -> Self {
        Self(EventInner::new(inner))
    }
}
