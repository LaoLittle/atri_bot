use crate::event::SharedEvent;

pub struct CustomEvent<T>(SharedEvent<T>);

impl<T> CustomEvent<T> {
    pub fn new(inner: T) -> Self {
        Self(SharedEvent::new(inner))
    }
}
