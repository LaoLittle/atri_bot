use std::sync::Arc;

use crate::Bot;

#[derive(Debug, Clone)]
pub struct Group {
    id: i64,
    bot: Arc<Bot>,
}