use std::error::Error;
use std::mem::ManuallyDrop;
use crate::RawString;


#[repr(C)]
pub struct FFIResult<T> {
    has_error: bool,
    result: ResultWithValue<T>,
}

impl<T, E: Error> From<Result<T, E>> for FFIResult<T> {
    fn from(r: Result<T, E>) -> Self {
        match r {
            Ok(val) => {
                Self {
                    has_error: false,
                    result: ResultWithValue { value: ManuallyDrop::new(val) },
                }
            }
            Err(e) => {
                Self {
                    has_error: true,
                    result: ResultWithValue { error: ManuallyDrop::new(e.into()) },
                }
            }
        }
    }
}

impl<T> From<FFIResult<T>> for Result<T, FFIError> {
    fn from(fr: FFIResult<T>) -> Self {
        let r = fr.result;
        if !fr.has_error {
            let val = unsafe { ManuallyDrop::into_inner(r.value) };
            Ok(val)
        } else {
            let err = unsafe { ManuallyDrop::into_inner(r.error) };
            Err(err)
        }
    }
}

#[repr(C)]
union ResultWithValue<T> {
    value: ManuallyDrop<T>,
    error: ManuallyDrop<FFIError>,
}

#[repr(C)]
pub struct FFIError {
    message: RawString,
    cause: RawString,
}

impl<E: Error> From<E> for FFIError {
    fn from(e: E) -> Self {
        let message = format!("{}", e);

        Self {
            message: RawString::from(message),
            cause: RawString::null(),
        }
    }
}