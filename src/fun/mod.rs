use std::mem;

use crate::{get_app};
use crate::event::GroupMessageEvent;
use crate::event::listener::Listener;
use crate::fun::drawmeme::{drawmeme_listener};
use crate::fun::moli::moli_listener;

mod drawmeme;
mod game;
mod moli;

pub fn handler() {
    mem::forget(drawmeme_listener());

    mem::forget(moli_listener());
}