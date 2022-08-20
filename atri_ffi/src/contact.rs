use crate::ManagedCloneable;
use std::mem::ManuallyDrop;

#[repr(C)]
pub struct FFIMember {
    pub is_named: bool,
    pub inner: MemberUnion,
}

#[repr(C)]
pub union MemberUnion {
    pub named: ManuallyDrop<ManagedCloneable>,
    pub anonymous: ManuallyDrop<ManagedCloneable>,
}
