use crate::Managed;

#[repr(C)]
pub struct FFIFn<Arg, R> {
    closure: Managed,
    invoke: extern fn(*const (), Arg) -> R
}

impl<Arg,R> FFIFn<Arg,R> {
    pub fn from<F>(closure: F) -> Self
    where F: Fn(Arg) -> R
    {
        let ma = Managed::from_value(closure);

        Self {
            closure: ma,
            invoke: _invoke_fn::<F,Arg,R>
        }
    }

    pub fn invoke(&self, arg: Arg) -> R {
        (self.invoke)(self.closure.pointer, arg)
    }
}

pub struct FFIFnV {
    closure: Managed,
    invoke: extern fn(*const ())
}

extern fn _invoke_fn<F,Arg,R>(ptr: *const (), arg: Arg) -> R
    where F: Fn(Arg) -> R
{
    let f = unsafe { &*(ptr as *const F) };
    f(arg)
}