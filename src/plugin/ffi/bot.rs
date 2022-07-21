use std::path::PathBuf;

use ricq::version::{get_version, Protocol};

use crate::Bot;
use crate::bot::BotConfiguration;
use crate::plugin::{Managed, RawString};
use crate::plugin::future::FFIFuture;

extern fn new_bot(id: i64, work_dir: RawString, protocol: u8) -> FFIFuture<Managed> {
    FFIFuture::from(
        async {
            let protocol = match protocol {
                0 => Protocol::IPad,
                1 => Protocol::AndroidPhone,
                2 => Protocol::AndroidWatch,
                3 => Protocol::MacOS,
                4 => Protocol::QiDian,
                _ => unreachable!()
            };

            let conf = BotConfiguration {
                work_dir: work_dir.to_string().map(PathBuf::from),
                version: get_version(protocol),
            };

            Managed::from_value(Bot::new(id, conf).await)
        }
    )
}