use std::mem;

use crate::event::listener::Listener;
use crate::event::GroupMessageEvent;
use crate::fun::drawmeme::drawmeme_listener;
use crate::fun::moli::moli_listener;
use crate::get_app;

mod drawmeme;
mod game;
mod moli;

pub fn handler() {
    mem::forget(drawmeme_listener());

    mem::forget(moli_listener());
}
