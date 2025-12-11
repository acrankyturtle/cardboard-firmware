#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]
#![feature(type_alias_impl_trait)]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use critical_section::Mutex;

pub mod command;
pub mod context;
pub mod device;
pub mod error;
pub mod hid;
pub mod input;
pub mod profile;
pub mod serial;
pub mod serialize;
pub mod state;
pub mod storage;
pub mod stream;
pub mod tasks;
pub mod time;

#[cfg(all(not(test), feature = "embassy"))]
pub mod embassy;

#[cfg(test)]
mod test;

pub trait TrackedAllocator {
	fn current(&self) -> usize;
	fn max(&self) -> usize;
}

/// Tracking allocator wrapper that monitors heap usage.
///
/// Wraps any `GlobalAlloc` implementation and tracks current and maximum
/// allocation statistics using interrupt-safe critical sections.
pub struct TrackingAllocator<A: GlobalAlloc> {
	pub inner: A,
	current: Mutex<Cell<usize>>, // Current allocated bytes
	max: Mutex<Cell<usize>>,     // Maximum allocated bytes ever
}

impl<A: GlobalAlloc> TrackingAllocator<A> {
	pub const fn new(inner: A) -> Self {
		TrackingAllocator {
			inner,
			current: Mutex::new(Cell::new(0)),
			max: Mutex::new(Cell::new(0)),
		}
	}

	/// Get current allocated bytes
	pub fn current(&self) -> usize {
		critical_section::with(|cs| self.current.borrow(cs).get())
	}

	/// Get maximum allocated bytes
	pub fn max(&self) -> usize {
		critical_section::with(|cs| self.max.borrow(cs).get())
	}

	/// Reset min and max to current value
	pub fn reset_stats(&self) {
		critical_section::with(|cs| {
			let current = self.current.borrow(cs).get();
			self.max.borrow(cs).set(current);
		});
	}
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<A> {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let ptr = unsafe { self.inner.alloc(layout) };
		if !ptr.is_null() {
			let size = layout.size();
			critical_section::with(|cs| {
				let new_current = self.current.borrow(cs).get() + size;
				self.current.borrow(cs).set(new_current);
				self.max
					.borrow(cs)
					.set(self.max.borrow(cs).get().max(new_current));
			});
		}
		ptr
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		if !ptr.is_null() {
			let size = layout.size();
			critical_section::with(|cs| {
				let new_current = self.current.borrow(cs).get().saturating_sub(size);
				self.current.borrow(cs).set(new_current);
			});
		}
		unsafe { self.inner.dealloc(ptr, layout) };
	}

	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		let old_size = layout.size();
		let new_ptr = unsafe { self.inner.realloc(ptr, layout, new_size) };
		if !new_ptr.is_null() && !ptr.is_null() {
			critical_section::with(|cs| {
				let new_current = self.current.borrow(cs).get() - old_size + new_size;
				self.current.borrow(cs).set(new_current);
				self.max
					.borrow(cs)
					.set(self.max.borrow(cs).get().max(new_current));
			});
		}
		new_ptr
	}
}

impl<A: GlobalAlloc> TrackedAllocator for TrackingAllocator<A> {
	fn current(&self) -> usize {
		self.current()
	}

	fn max(&self) -> usize {
		self.max()
	}
}
