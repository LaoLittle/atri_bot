use crate::message::at::At;
use crate::message::image::Image;
use crate::message::meta::{Anonymous, MessageMetadata, Reply};
use crate::message::{MessageChain, MessageValue};
use atri_ffi::ffi::ForFFI;
use atri_ffi::message::meta::{
    FFIAnonymous, FFIMessageMetadata, FFIReply, ANONYMOUS_FLAG, NONE_META, REPLY_FLAG,
};
use atri_ffi::message::{
    FFIAt, FFIMessageChain, FFIMessageValue, MessageValueUnion, AT, AT_ALL, IMAGE, TEXT,
};
use atri_ffi::{RustString, RustVec};
use std::mem::{ManuallyDrop, MaybeUninit};

impl ForFFI for MessageChain {
    type FFIValue = FFIMessageChain;

    fn into_ffi(self) -> Self::FFIValue {
        let v: Vec<FFIMessageValue> = self.value.into_iter().map(MessageValue::into_ffi).collect();

        let raw = RustVec::from(v);
        FFIMessageChain {
            meta: self.meta.into_ffi(),
            inner: raw,
        }
    }

    fn from_ffi(ffi: Self::FFIValue) -> Self {
        let v = ffi.inner.into_vec();
        let value = v.into_iter().map(MessageValue::from_ffi).collect();
        Self {
            meta: MessageMetadata::from_ffi(ffi.meta),
            value,
        }
    }
}

impl ForFFI for MessageValue {
    type FFIValue = FFIMessageValue;

    fn into_ffi(self) -> Self::FFIValue {
        match self {
            MessageValue::Text(s) => FFIMessageValue {
                t: TEXT,
                union: MessageValueUnion {
                    text: ManuallyDrop::new(RustString::from(s)),
                },
            },
            MessageValue::Image(img) => FFIMessageValue {
                t: IMAGE,
                union: MessageValueUnion {
                    image: ManuallyDrop::new(img.0),
                },
            },
            MessageValue::At(At { target, display }) => FFIMessageValue {
                t: AT,
                union: MessageValueUnion {
                    at: ManuallyDrop::new({
                        let display = RustString::from(display);
                        FFIAt { target, display }
                    }),
                },
            },
            MessageValue::AtAll => FFIMessageValue {
                t: AT_ALL,
                union: MessageValueUnion { at_all: () },
            },
            MessageValue::Unknown(ma) => FFIMessageValue {
                t: 255,
                union: MessageValueUnion {
                    unknown: ManuallyDrop::new(ma),
                },
            },
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        unsafe {
            match value.t {
                TEXT => Self::Text(ManuallyDrop::into_inner(value.union.text).into()),
                IMAGE => Self::Image(Image(ManuallyDrop::into_inner(value.union.image))),
                AT => {
                    let FFIAt { target, display } = ManuallyDrop::into_inner(value.union.at);
                    let display = String::from(display);

                    Self::At(At { target, display })
                }
                _ => Self::Unknown(ManuallyDrop::into_inner(value.union.unknown)),
            }
        }
    }
}

impl ForFFI for At {
    type FFIValue = FFIAt;

    fn into_ffi(self) -> Self::FFIValue {
        let At { target, display } = self;

        FFIAt {
            target,
            display: RustString::from(display),
        }
    }

    fn from_ffi(value: Self::FFIValue) -> Self {
        let FFIAt { target, display } = value;

        Self {
            target,
            display: String::from(display),
        }
    }
}

impl ForFFI for MessageMetadata {
    type FFIValue = FFIMessageMetadata;

    fn into_ffi(self) -> Self::FFIValue {
        let Self {
            seqs,
            rands,
            time,
            sender,
            anonymous,
            reply,
        } = self;

        let mut flags = NONE_META;

        let mut ffi_anonymous = MaybeUninit::uninit();
        if let Some(ano) = anonymous {
            flags |= ANONYMOUS_FLAG;
            ffi_anonymous.write(ano.into_ffi());
        }

        let mut ffi_reply = MaybeUninit::uninit();
        if let Some(reply) = reply {
            flags |= REPLY_FLAG;
            ffi_reply.write(reply.into_ffi());
        }

        FFIMessageMetadata {
            seqs: seqs.into(),
            rands: rands.into(),
            time,
            sender,
            flags,
            anonymous: ffi_anonymous,
            reply: ffi_reply,
        }
    }

    fn from_ffi(ffi: Self::FFIValue) -> Self {
        let FFIMessageMetadata {
            seqs,
            rands,
            time,
            sender,
            flags,
            reply,
            anonymous,
        } = ffi;

        unsafe {
            Self {
                seqs: seqs.into_vec(),
                rands: rands.into_vec(),
                time,
                sender,
                anonymous: if flags & ANONYMOUS_FLAG != 0 {
                    Some(Anonymous::from_ffi(anonymous.assume_init()))
                } else {
                    None
                },
                reply: if flags & REPLY_FLAG != 0 {
                    Some(Reply::from_ffi(reply.assume_init()))
                } else {
                    None
                },
            }
        }
    }
}

impl ForFFI for Reply {
    type FFIValue = FFIReply;

    fn into_ffi(self) -> Self::FFIValue {
        let Self {
            reply_seq,
            sender,
            time,
            elements,
        } = self;

        let ffi_value: Vec<FFIMessageValue> =
            elements.into_iter().map(|value| value.into_ffi()).collect();
        let raw = RustVec::from(ffi_value);

        FFIReply {
            reply_seq,
            sender,
            time,
            elements: raw,
        }
    }

    fn from_ffi(ffi: Self::FFIValue) -> Self {
        let FFIReply {
            reply_seq,
            sender,
            time,
            elements,
        } = ffi;

        let v = elements.into_vec();
        let value: Vec<MessageValue> = v.into_iter().map(MessageValue::from_ffi).collect();

        Self {
            reply_seq,
            sender,
            time,
            elements: value,
        }
    }
}

impl ForFFI for Anonymous {
    type FFIValue = FFIAnonymous;

    fn into_ffi(self) -> Self::FFIValue {
        let Self {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        } = self;

        let anon_id = RustVec::from(anon_id);
        let nick = RustString::from(nick);
        let color = RustString::from(color);

        FFIAnonymous {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        }
    }

    fn from_ffi(ffi: Self::FFIValue) -> Self {
        let FFIAnonymous {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        } = ffi;

        let anon_id = anon_id.into_vec();
        let nick = String::from(nick);
        let color = String::from(color);

        Self {
            anon_id,
            nick,
            portrait_index,
            bubble_index,
            expire_time,
            color,
        }
    }
}
