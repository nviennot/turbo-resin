// SPDX-License-Identifier: GPL-3.0-or-later

use core::cell::UnsafeCell;

pub struct SharedWithInterrupt<T>(UnsafeCell<T>);
impl<T> SharedWithInterrupt<T> {
    pub fn new(v: T) -> Self {
        Self(UnsafeCell::new(v))
    }
    pub fn lock<R>(&self, mut f: impl FnMut(&mut T) -> R) -> R {
        let mut_self = unsafe { &mut *self.0.get() };
        cortex_m::interrupt::free(|_| f(mut_self))
    }

    pub unsafe fn lock_from_interrupt<R>(&self, mut f: impl FnMut(&mut T) -> R) -> R {
        let mut_self = &mut *self.0.get();
        f(mut_self)
    }
}

unsafe impl<T> Sync for SharedWithInterrupt<T> {}
unsafe impl<T> Send for SharedWithInterrupt<T> {}
