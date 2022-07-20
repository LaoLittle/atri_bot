use tokio::sync::broadcast::Receiver;

use crate::{Event, global_receiver};
use crate::plugin::ffi::event::FFIEvent;
use crate::plugin::future::FFIFuture;
use crate::plugin::Managed;

pub extern fn new_receiver() -> Managed {
    Managed::from_value(global_receiver())
}

pub extern fn receiver_receive(rx: *mut ()) -> FFIFuture<FFIEvent> {
    let fu = async {
        let rx = unsafe { &mut *(rx as *mut Receiver<Event>) };
        let e = rx.recv().await.expect("Why");
        e.into()
    };

    FFIFuture::from(fu)
}