use tokio::{io, runtime};
use tokio::runtime::Runtime;
use crate::plugin::error::FFIResult;
use crate::plugin::Managed;

mod bot;
mod group;

#[no_mangle]
extern fn get_runtime() -> Managed {
    let ma = Managed::from_static(crate::get_runtime());
    ma
}

#[no_mangle]
extern fn build_new_runtime(multi_thread: bool, worker_threads: usize, io: bool, time: bool) -> FFIResult<Managed> {
    let res = build_new_runtime0(multi_thread, worker_threads, io, time);

    res.into()
}

fn build_new_runtime0(multi_thread: bool, worker_threads: usize, io: bool, time: bool) -> io::Result<Managed> {
    let mut builder = if multi_thread {
        runtime::Builder::new_multi_thread()
    } else {
        runtime::Builder::new_current_thread()
    };

    builder.worker_threads(worker_threads);
    if io { builder.enable_io(); }
    if time { builder.enable_time(); }

    let rt = builder.build()?;
    Ok(Managed::from_value(rt))
}