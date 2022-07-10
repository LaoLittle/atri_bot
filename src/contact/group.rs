use std::sync::Arc;
use crate::Bot;

#[derive(Debug)]
pub struct Group {
    id: i64,
    bot: Arc<Bot>
}