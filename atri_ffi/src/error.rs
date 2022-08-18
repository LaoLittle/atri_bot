use crate::RustString;
use std::error::Error;
use std::mem::ManuallyDrop;

#[repr(C)]
pub struct FFIResult<T> {
    has_error: bool,
    value: ValueOrError<T>,
}

unsafe impl<T> Send for FFIResult<T> {}

unsafe impl<T> Sync for FFIResult<T> {}

#[repr(C)]
union ValueOrError<T> {
    value: ManuallyDrop<T>,
    error: ManuallyDrop<RustString>,
}

impl<T, E: Error> From<Result<T, E>> for FFIResult<T> {
    fn from(r: Result<T, E>) -> Self {
        match r {
            Ok(val) => Self {
                has_error: false,
                value: ValueOrError {
                    value: ManuallyDrop::new(val),
                },
            },
            Err(e) => Self {
                has_error: true,
                value: ValueOrError {
                    error: ManuallyDrop::new(RustString::from(format!("{}", e))),
                },
            },
        }
    }
}

impl<T> From<FFIResult<T>> for Result<T, String> {
    fn from(ffi: FFIResult<T>) -> Self {
        unsafe {
            if !ffi.has_error {
                Ok(ManuallyDrop::into_inner(ffi.value.value))
            } else {
                Err(ManuallyDrop::into_inner(ffi.value.error).into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use crate::error::FFIResult;

    #[test]
    fn result() {
        let result = usize::from_str("abc");
        let ffi = FFIResult::from(result);
        let result = Result::from(ffi);

        assert_eq!(result, Err("invalid digit found in string".into()));
    }
}