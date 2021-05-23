// This executable is marked with `no_std`. To do so, a panic_handler and the
// eh_personality have been added. In addition, we both define _start and main
// and link using -nostartfiles.
// https://fasterthanli.me/series/making-our-own-executable-packer/part-12
//
// We also want to run tests on this crate, so all things listed above are
// enabled when tests are not; the test configuration is a standard
// configuration where the panic_handler, the eh_personality and the _start
// function are already provided

#![no_std]
#![no_main]
#![feature(lang_items, asm)]
#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[cfg(not(test))]
#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

// Pull in the system libc library for what crt0.o likely requires.
extern crate libc;

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    main()
}

#[cfg(not(test))]
#[no_mangle]
pub fn main() -> ! {
    loop {}
}

#[cfg(test)]
#[no_mangle]
pub fn main() -> i32 {
    0
}
