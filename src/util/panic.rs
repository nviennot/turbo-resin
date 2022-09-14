
// SPDX-License-Identifier: GPL-3.0-or-later

#[alloc_error_handler]
fn oom(l: core::alloc::Layout) -> ! {
    panic!("Out of memory. Failed to allocate {} bytes", l.size());
}

#[inline(never)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    debug!("{}", info);
    loop {}
}
