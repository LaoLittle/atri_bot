use std::mem;

// use crate::fun::drawmeme::drawmeme_listener;
use crate::fun::moli::moli_listener;

// mod drawmeme;
mod game;
mod moli;

pub fn handler() {
    // mem::forget(drawmeme_listener());

    mem::forget(moli_listener());
}
