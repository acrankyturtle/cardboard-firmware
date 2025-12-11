#![cfg_attr(not(test), no_std)]
#![feature(impl_trait_in_assoc_type)]
#![feature(generic_const_exprs)]

extern crate alloc;

use alloc::string::{String, ToString};

use cardboard_lib::device::DeviceId;

pub use static_cell::StaticCell;

pub mod rp2040;

static SERIAL_NUMBER: StaticCell<String> = StaticCell::new();

pub fn get_serial_number(device_id: &DeviceId) -> &'static str {
	SERIAL_NUMBER.init(device_id.to_string())
}
