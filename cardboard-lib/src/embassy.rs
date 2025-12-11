use alloc::vec::Vec;
use defmt::error;
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::{Input, Output};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_rp::{
	flash::{Async, ERASE_SIZE, Flash, WRITE_SIZE},
	peripherals::FLASH,
};

use embassy_sync::{blocking_mutex::raw::RawMutex, signal::Signal};
use embassy_time::Timer;
use embassy_usb::class::cdc_acm::{Receiver, Sender};

use crate::context::{ExternalTagsSignalRx, VirtualKeySignalTx};
use crate::hid::{HidDevice, HidReport, ReportHid};
use crate::profile::{ConsumerControlEvent, KeyboardEvent, MouseEvent};
use crate::serial::{SerialDrain, SerialPacketReader, SerialPacketSender};
use crate::storage::{BlockFlash, FlashPartition, PartitionedFlashMemory};
use crate::time::{Clock, Duration};
use crate::{
	context::{ExternalTagsSignalTx, UpdateProfileSignalRx, UpdateProfileSignalTx},
	input::{ColPin, RowPin},
	profile::{KeyboardProfile, LayerTag},
};

impl<M: RawMutex> UpdateProfileSignalTx for Signal<M, KeyboardProfile> {
	fn update_profile(&self, profile: KeyboardProfile) {
		self.signal(profile);
	}
}

impl<M: RawMutex> UpdateProfileSignalRx for Signal<M, KeyboardProfile> {
	fn try_get_changed_profile(&self) -> Option<KeyboardProfile> {
		self.try_take()
	}
}

impl<M: RawMutex> ExternalTagsSignalTx for Signal<M, Vec<LayerTag>> {
	fn set_external_tags(&self, tags: Vec<LayerTag>) {
		self.signal(tags);
	}
}

impl<M: RawMutex> ExternalTagsSignalRx for Signal<M, Vec<LayerTag>> {
	fn try_get_external_tags(&self) -> Option<Vec<LayerTag>> {
		self.try_take()
	}
}

impl<M: RawMutex, const SIZE: usize> VirtualKeySignalTx<SIZE> for Signal<M, [u8; SIZE]> {
	fn set_virtual_keys(&self, state: [u8; SIZE]) {
		self.signal(state);
	}
}

impl<M: RawMutex, const SIZE: usize> crate::context::VirtualKeySignalRx<SIZE>
	for Signal<M, [u8; SIZE]>
{
	fn try_get_virtual_keys(&self) -> Option<[u8; SIZE]> {
		self.try_take()
	}
}

impl RowPin for Output<'_> {
	fn set_high(&mut self) {
		self.set_high();
	}

	fn set_low(&mut self) {
		self.set_low();
	}
}

impl ColPin for Input<'_> {
	fn is_high(&self) -> bool {
		self.is_high()
	}
}

pub struct EmbassySerialPacketReader<'d, const SIZE: usize> {
	receiver: Receiver<'d, Driver<'d, USB>>,
	timeout: embassy_time::Duration,
}

pub struct EmbassySerialPacketWriter<'d, const SIZE: usize> {
	sender: Sender<'d, Driver<'d, USB>>,
	timeout: embassy_time::Duration,
}

impl<'d, const SIZE: usize> EmbassySerialPacketReader<'d, SIZE> {
	pub fn new(receiver: Receiver<'d, Driver<'d, USB>>, timeout: crate::time::Duration) -> Self {
		Self {
			receiver,
			timeout: embassy_time::Duration::from_millis(timeout.to_millis() as u64),
		}
	}
}

impl<'d, const SIZE: usize> EmbassySerialPacketWriter<'d, SIZE> {
	pub fn new(sender: Sender<'d, Driver<'d, USB>>, timeout: crate::time::Duration) -> Self {
		Self {
			sender,
			timeout: embassy_time::Duration::from_millis(timeout.to_millis() as u64),
		}
	}
}

impl<'d, const SIZE: usize> SerialPacketReader for EmbassySerialPacketReader<'d, SIZE> {
	async fn read_packet(&mut self, buf: &mut [u8]) -> Result<usize, &'static str> {
		let timer = Timer::after(self.timeout);

		let result = select(self.receiver.read_packet(buf), async { timer.await }).await;

		match result {
			Either::First(result) => result.map_err(|_| "Endpoint error"),
			Either::Second(_) => Err("Read timeout"),
		}
	}

	const SIZE: usize = SIZE;
}

impl<'d, const SIZE: usize> SerialDrain for EmbassySerialPacketReader<'d, SIZE> {
	async fn drop_packet(&mut self) -> bool {
		let mut buf = [0u8; SIZE];
		self.read_packet(&mut buf).await.is_ok()
	}
}

impl<'d, const SIZE: usize> SerialPacketSender for EmbassySerialPacketWriter<'d, SIZE> {
	async fn write_packet(&mut self, data: &[u8]) -> Result<(), &'static str> {
		let timer = Timer::after(self.timeout);
		let result =
			embassy_futures::select::select(self.sender.write_packet(data), async { timer.await })
				.await;

		match result {
			embassy_futures::select::Either::First(result) => result.map_err(|_| "Endpoint error"),
			embassy_futures::select::Either::Second(_) => Err("Write timeout"),
		}
	}
	const SIZE: usize = SIZE;
}

pub struct EmbassyFlashMemory<'d, const SIZE: usize> {
	flash_addr: *const u8,
	storage_addr: *const u8,
	length: usize,
	flash: Flash<'d, FLASH, Async, SIZE>,
}

impl<'d, const SIZE: usize> EmbassyFlashMemory<'d, SIZE> {
	pub fn new(
		flash_addr: *const u8,
		storage_addr: *const u8,
		length: usize,
		flash: Flash<'d, FLASH, Async, SIZE>,
	) -> Self {
		if storage_addr as usize % WRITE_SIZE != 0 {
			error!(
				"Base address is not write block aligned: {}",
				storage_addr as usize
			);
			panic!("Base address is not write block aligned");
		}

		if storage_addr as usize % ERASE_SIZE != 0 {
			error!(
				"Base address is not erase block aligned: {}",
				storage_addr as usize
			);
			panic!("Base address is not erase block aligned");
		}

		if length % WRITE_SIZE != 0 {
			error!("Length is not block aligned: {}", length);
			panic!("Length is not block aligned");
		}

		if length % ERASE_SIZE != 0 {
			error!("Length is not erase block aligned: {}", length);
			panic!("Length is not erase block aligned");
		}

		EmbassyFlashMemory {
			flash_addr,
			storage_addr,
			length,
			flash,
		}
	}

	fn get_flash_offset(&self) -> usize {
		self.storage_addr as usize - self.flash_addr as usize
	}
}

impl<'a, const SIZE: usize> BlockFlash for EmbassyFlashMemory<'a, SIZE> {
	fn as_slice(&self) -> &'static [u8] {
		unsafe { core::slice::from_raw_parts(self.storage_addr, self.length) }
	}

	fn erase(&mut self, offset: usize, length: usize) -> Result<(), &'static str> {
		let start = offset + self.get_flash_offset();
		let end = start + length;

		self.flash
			.blocking_erase(start as u32, end as u32)
			.map_err(|e| {
				error!("Error erasing flash memory: {:?}", e);
				match e {
					embassy_rp::flash::Error::OutOfBounds => "Erase out of bounds",
					embassy_rp::flash::Error::Unaligned => "Erase not block aligned",
					_ => "Error erasing flash memory",
				}
			})
	}

	fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), &'static str> {
		self.flash
			.blocking_write((self.get_flash_offset() + offset) as u32, data)
			.map_err(|e| {
				error!("Error writing to flash memory: {:?}", e);
				match e {
					embassy_rp::flash::Error::OutOfBounds => "Write out of bounds",
					embassy_rp::flash::Error::Unaligned => "Write not block aligned",
					_ => "Error writing to flash memory",
				}
			})
	}

	fn length(&self) -> usize {
		self.length
	}

	const ERASE_BLOCK_SIZE: usize = ERASE_SIZE;

	const WRITE_BLOCK_SIZE: usize = WRITE_SIZE;
}

pub struct EmbassyTickClock {}

impl Clock for EmbassyTickClock {
	fn now(&self) -> crate::time::Instant {
		from_embassy_instant(embassy_time::Instant::now())
	}

	async fn after(&self, duration: Duration) {
		Timer::after(to_embassy_duration(duration)).await;
	}

	async fn at(&self, instant: crate::time::Instant) {
		Timer::at(to_embassy_instant(instant)).await;
	}
}

fn from_embassy_instant(instant: embassy_time::Instant) -> crate::time::Instant {
	crate::time::Instant::from_ticks(instant.as_micros())
}

fn to_embassy_instant(instant: crate::time::Instant) -> embassy_time::Instant {
	embassy_time::Instant::from_micros(instant.ticks())
}

fn to_embassy_duration(duration: crate::time::Duration) -> embassy_time::Duration {
	embassy_time::Duration::from_micros(duration.to_micros() as u64)
}

pub struct EmbassyKeypadHid<
	HidKeyboard: HidDevice<KeyboardEvent> + 'static,
	HidMouse: HidDevice<MouseEvent> + 'static,
	HidConsumer: HidDevice<ConsumerControlEvent> + 'static,
	M: 'static + RawMutex,
> where
	[(); HidKeyboard::SIZE]:,
	[(); HidMouse::SIZE]:,
	[(); HidConsumer::SIZE]:,
{
	pub keyboard: HidKeyboard,
	pub mouse: HidMouse,
	pub consumer: HidConsumer,
	pub signal: &'static Signal<
		M,
		HidReport<{ HidKeyboard::SIZE }, { HidMouse::SIZE }, { HidConsumer::SIZE }>,
	>,
}

impl<
	HidKeyboard: HidDevice<KeyboardEvent>,
	HidMouse: HidDevice<MouseEvent>,
	HidConsumer: HidDevice<ConsumerControlEvent>,
	M: 'static + RawMutex,
> ReportHid for EmbassyKeypadHid<HidKeyboard, HidMouse, HidConsumer, M>
where
	[(); HidKeyboard::SIZE]:,
	[(); HidMouse::SIZE]:,
	[(); HidConsumer::SIZE]:,
{
	fn report_keyboard(&mut self, report: crate::profile::KeyboardEvent) {
		self.keyboard.input(report);
	}

	fn report_mouse(&mut self, report: crate::profile::MouseEvent) {
		self.mouse.input(report);
	}

	fn report_consumer(&mut self, report: crate::profile::ConsumerControlEvent) {
		self.consumer.input(report);
	}

	fn flush(&mut self) {
		let keyboard = self.keyboard.create_report();
		let mouse = self.mouse.create_report();
		let consumer = self.consumer.create_report();

		self.signal.signal(HidReport {
			keyboard,
			mouse,
			consumer,
		});
	}

	fn reset(&mut self) {
		self.keyboard.reset();
		self.mouse.reset();
		self.consumer.reset();

		self.signal.signal(HidReport {
			keyboard: None,
			mouse: None,
			consumer: None,
		});
	}
}
