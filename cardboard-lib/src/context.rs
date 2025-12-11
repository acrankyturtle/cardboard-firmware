use core::alloc::GlobalAlloc;

use crate::{
	TrackingAllocator,
	device::DeviceInfo,
	error::ErrorLog,
	profile::{KeyboardProfile, LayerTag},
	serial::SerialDrain,
	storage::{BlockFlash, BlockFlashExt, FlashPartition, PartitionedFlashMemory},
	stream::{ReadAsync, WriteAsync},
};
use alloc::vec::Vec;

/// Main context struct holding all runtime dependencies.
pub struct Context<
	Flash,
	SerialRx,
	SerialTx,
	const VIRTUAL_KEY_BITFIELD_BYTES: usize,
	Allocator,
	Errors,
	Clock,
> where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	pub device_info: &'static DeviceInfo,
	pub flash: Flash,
	pub settings_partition: FlashPartition<Flash>,
	pub profile_partition: FlashPartition<Flash>,
	pub update_profile_signal: &'static dyn UpdateProfileSignalTx,
	pub serial_rx: SerialRx,
	pub serial_tx: SerialTx,
	pub external_tags_signal: &'static dyn ExternalTagsSignalTx,
	pub virtual_keys_signal: &'static dyn VirtualKeySignalTx<VIRTUAL_KEY_BITFIELD_BYTES>,
	pub allocator: &'static TrackingAllocator<Allocator>,
	pub reboot: &'static mut dyn Reboot,
	pub bootloader: &'static dyn RebootToBootloader,
	pub errors: Errors,
	pub clock: &'static Clock,
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	#[allow(clippy::too_many_arguments)]
	pub fn new(
		device_info: &'static DeviceInfo,
		flash: Flash,
		settings_partition: FlashPartition<Flash>,
		profile_partition: FlashPartition<Flash>,
		update_profile_signal: &'static dyn UpdateProfileSignalTx,
		serial_rx: SerialRx,
		serial_tx: SerialTx,
		external_tags_signal: &'static dyn ExternalTagsSignalTx,
		virtual_keys_signal: &'static dyn VirtualKeySignalTx<VIRTUAL_KEY_BITFIELD_BYTES>,
		allocator: &'static TrackingAllocator<Allocator>,
		reboot: &'static mut dyn Reboot,
		bootloader: &'static dyn RebootToBootloader,
		errors: Errors,
		clock: &'static Clock,
	) -> Self {
		Self {
			device_info,
			flash,
			settings_partition,
			profile_partition,
			update_profile_signal,
			serial_rx,
			serial_tx,
			external_tags_signal,
			virtual_keys_signal,
			allocator,
			reboot,
			bootloader,
			errors,
			clock,
		}
	}
}

// Context capability traits - these define what features a context provides
// Commands use these as trait bounds to specify their requirements

pub trait ContextDeviceInfo {
	fn device_info(&self) -> &'static DeviceInfo;
}

pub trait ContextSerialRx {
	type SerialRx: ReadAsync + SerialDrain;
	fn serial_rx(&mut self) -> &mut Self::SerialRx;
}

pub trait ContextSerialTx {
	type SerialTx: WriteAsync;
	fn serial_tx(&mut self) -> &mut Self::SerialTx;
}

pub trait ContextSettingsFlash {
	type Flash: BlockFlash;
	fn settings_flash(&mut self) -> PartitionedFlashMemory<Self::Flash>;
}

pub trait ContextProfileFlash {
	type Flash: BlockFlash;
	fn profile_flash(&mut self) -> PartitionedFlashMemory<Self::Flash>;
}

pub trait ContextUpdateProfile {
	type UpdateProfileSignal: UpdateProfileSignalTx + ?Sized;
	fn profile_signal(&mut self) -> &Self::UpdateProfileSignal;
}

pub trait ContextTags {
	fn set_external_tags(&mut self, tags: Vec<LayerTag>);
}

pub trait ContextVirtualKeys<const VIRTUAL_KEY_BITFIELD_BYTES: usize> {
	fn set_virtual_keys(&mut self, state: [u8; VIRTUAL_KEY_BITFIELD_BYTES]);
}

pub trait ContextAllocator {
	fn allocator(&self) -> &TrackingAllocator<Self::A>;
	type A: GlobalAlloc;
}

pub trait ContextReboot {
	fn reboot(&mut self) -> !;
	fn reboot_to_bootloader(&mut self) -> !;
}

pub trait ContextErrorLog {
	fn errors(&mut self) -> &mut Self::Errors;
	type Errors: ErrorLog;
}

pub trait ContextClock {
	fn clock(&self) -> &impl crate::time::Clock;
}

// Trait implementations for Context

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextDeviceInfo
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync + SerialDrain,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	fn device_info(&self) -> &'static DeviceInfo {
		self.device_info
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextSerialRx
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync + SerialDrain,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	type SerialRx = SerialRx;
	fn serial_rx(&mut self) -> &mut Self::SerialRx {
		&mut self.serial_rx
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextSerialTx
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	type SerialTx = SerialTx;
	fn serial_tx(&mut self) -> &mut Self::SerialTx {
		&mut self.serial_tx
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextSettingsFlash
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	type Flash = Flash;

	fn settings_flash(&mut self) -> PartitionedFlashMemory<Flash> {
		self.flash.partition(&self.settings_partition)
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextProfileFlash
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	type Flash = Flash;

	fn profile_flash(&mut self) -> PartitionedFlashMemory<Flash> {
		self.flash.partition(&self.profile_partition)
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextUpdateProfile
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	type UpdateProfileSignal = dyn UpdateProfileSignalTx;
	fn profile_signal(&mut self) -> &Self::UpdateProfileSignal {
		self.update_profile_signal
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextTags
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	fn set_external_tags(&mut self, tags: Vec<LayerTag>) {
		self.external_tags_signal.set_external_tags(tags);
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextVirtualKeys<VIRTUAL_KEY_BITFIELD_BYTES>
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	fn set_virtual_keys(&mut self, state: [u8; VIRTUAL_KEY_BITFIELD_BYTES]) {
		self.virtual_keys_signal.set_virtual_keys(state);
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextAllocator
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	type A = Allocator;
	fn allocator(&self) -> &TrackingAllocator<Self::A> {
		self.allocator
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextReboot
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	fn reboot(&mut self) -> ! {
		self.reboot.reboot()
	}

	fn reboot_to_bootloader(&mut self) -> ! {
		self.bootloader.reboot_to_bootloader()
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextErrorLog
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	type Errors = Errors;
	fn errors(&mut self) -> &mut Self::Errors {
		&mut self.errors
	}
}

impl<Flash, SerialRx, SerialTx, const VIRTUAL_KEY_BITFIELD_BYTES: usize, Allocator, Errors, Clock>
	ContextClock
	for Context<Flash, SerialRx, SerialTx, VIRTUAL_KEY_BITFIELD_BYTES, Allocator, Errors, Clock>
where
	Flash: BlockFlash,
	SerialRx: ReadAsync,
	SerialTx: WriteAsync,
	Allocator: GlobalAlloc + 'static,
	Errors: ErrorLog,
	Clock: crate::time::Clock + 'static,
{
	fn clock(&self) -> &impl crate::time::Clock {
		self.clock
	}
}

// Signal traits for inter-task communication

pub trait UpdateProfileSignalTx {
	fn update_profile(&self, profile: KeyboardProfile);
}

pub trait UpdateProfileSignalRx {
	fn try_get_changed_profile(&self) -> Option<KeyboardProfile>;
}

pub trait ExternalTagsSignalTx {
	fn set_external_tags(&self, tags: Vec<LayerTag>);
}

pub trait ExternalTagsSignalRx {
	fn try_get_external_tags(&self) -> Option<Vec<LayerTag>>;
}

pub trait VirtualKeySignalTx<const SIZE: usize> {
	fn set_virtual_keys(&self, state: [u8; SIZE]);
}

pub trait VirtualKeySignalRx<const SIZE: usize> {
	fn try_get_virtual_keys(&self) -> Option<[u8; SIZE]>;
}

pub trait Reboot {
	fn reboot(&mut self) -> !;
}

pub trait RebootToBootloader {
	fn reboot_to_bootloader(&self) -> !;
}
