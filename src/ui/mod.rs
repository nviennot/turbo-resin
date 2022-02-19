// SPDX-License-Identifier: GPL-3.0-or-later

mod move_z;
pub use move_z::*;


use lvgl::core::{Display, Screen, ObjExt};

pub fn new_screen<D,C>(display: &Display<D>, init_f: impl FnOnce(&mut Screen::<C>) -> C) -> Screen::<C> {
    let mut screen = Screen::<C>::new(display);
    let context = init_f(&mut screen);
    screen.context().replace(context);
    screen
}
