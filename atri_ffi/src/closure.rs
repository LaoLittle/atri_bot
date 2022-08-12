use crate::Managed;

#[repr(C)]
pub struct FFIFn<T, Arg> {
    closure: Managed,
    invoke: extern fn(*const (), Arg) -> T
}

impl<T, Arg> FFIFn<T, Arg> {
    pub fn from<F>(closure: F) -> Self
    where F: Fn(Arg) -> T
    {
        let ma = Managed::from_value(closure);

        Self {
            closure: ma,
            invoke: _invoke_fn::<F,T,Arg>
        }
    }

    pub fn invoke(&self, arg: Arg) -> T {
        (self.invoke)(self.closure.pointer, arg)
    }
}

pub struct FFIFnV {
    closure: Managed,
    invoke: extern fn(*const ())
}

extern fn _invoke_fn<F,T,Arg>(ptr: *const (), arg: Arg) -> T
    where F: Fn(Arg) -> T
{
    let f = unsafe { &*(ptr as *const F) };
    f(arg)
}