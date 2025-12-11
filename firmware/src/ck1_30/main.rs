#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
#![feature(generic_const_exprs)]

extern crate alloc;
extern crate cortex_m;
extern crate usbd_human_interface_device;

use alloc::vec;
use core::mem::MaybeUninit;
use embedded_alloc::LlffHeap;

use alloc::{boxed::Box, vec::Vec};
use cardboard::{
	get_serial_number,
	rp2040::{
		bootloader::{EmbassyRp2040Reboot, EmbassyRp2040RebootToBootloader},
		flash::{init_flash, FLASH_SIZE},
		usb::{init_usb, init_usb_no_mouse, usb_task, USB_SERIAL_PACKET_SIZE},
	},
	StaticCell,
};
use cardboard_lib::{
	command::{
		UpdateProfileCommand, Command, GetProfileCommand, GetSettingsCommand, GetStatusCommand,
		IdentifyCommand, RebootCommand, SetExternalTagsCommand, SetVirtualKeysCommand,
		UpdateSettingsCommand,
	},
	context::Context,
	device::{DeviceInfo, DeviceTypeId, DeviceVersion},
	embassy::{EmbassyFlashMemory, EmbassyKeypadHid, EmbassyTickClock},
	error::HeaplessSpscErrorLog,
	hid::{HidDevice, HidReport},
	input::{ColPin, KeyId, KeyMatrix, RowPin},
	profile::{KeyboardProfile, LayerTag},
	serial::BufferedReader,
	serialize::Readable,
	storage::{load_profile_from_flash, load_settings_from_flash, BlockFlashExt, FlashPartition},
	stream::{ReadAsync, ReadAsyncExt},
	TrackingAllocator,
};
use cardboard_lib::{
	embassy::{EmbassySerialPacketReader, EmbassySerialPacketWriter},
	time::Duration,
};
use embassy_executor::Spawner;
use embassy_rp::{
	gpio::{Input, Level, Output, Pin, Pull},
	peripherals::USB,
	usb::Driver,
	watchdog::Watchdog,
};
use embassy_usb::class::hid::HidWriter;
use fugit::ExtU64;
use uuid::Uuid;

use {defmt::*, defmt_rtt as _, panic_probe as _};

const HEAP_SIZE: usize = 96 * 1024; // 96 KB
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];
pub type Heap = LlffHeap;

#[global_allocator]
static ALLOCATOR: TrackingAllocator<Heap> = TrackingAllocator::new(Heap::empty());

const ROWS: usize = 5;
const COLS: usize = 6;

const VIRTUAL_KEY_BITFIELD_SIZE: usize = 4; // 32 bits

// profile flash storage
#[link_section = ".profile"]
static mut FLASH_DATA: MaybeUninit<[u8; FLASH_DATA_SIZE]> = MaybeUninit::uninit();
const FLASH_DATA_SIZE: usize = 500 * 1024; // 500 KB
const SETTINGS_SIZE: usize = 4 * 1024; // 4 KB
const PROFILE_SIZE: usize = FLASH_DATA_SIZE - SETTINGS_SIZE;

// hid
type KeyboardImpl = cardboard_lib::hid::NKROKeyboard;
type MouseImpl = cardboard_lib::hid::Mouse;
type ConsumerImpl = cardboard_lib::hid::ConsumerControl;

type Mutex = embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
type Signal<T> = embassy_sync::signal::Signal<Mutex, T>;
static HID_SIGNAL: Signal<
	HidReport<{ KeyboardImpl::SIZE }, { MouseImpl::SIZE }, { ConsumerImpl::SIZE }>,
> = Signal::new();
static PROFILE_CHANGED_SIGNAL: Signal<KeyboardProfile> = Signal::new();
static EXTERNAL_TAGS_CHANGED_SIGNAL: Signal<Vec<LayerTag>> = Signal::new();
static VIRTUAL_KEY_SIGNAL: Signal<[u8; VIRTUAL_KEY_BITFIELD_SIZE]> = Signal::new();

type Matrix = KeyMatrix<ROWS, COLS>;

type ContextFlashMemory = EmbassyFlashMemory<'static, FLASH_SIZE>;
type ContextSerialReader =
	BufferedReader<EmbassySerialPacketReader<'static, USB_SERIAL_PACKET_SIZE>>;
type ContextSerialWriter = EmbassySerialPacketWriter<'static, USB_SERIAL_PACKET_SIZE>;

type CommandContext = Context<
	ContextFlashMemory,
	ContextSerialReader,
	ContextSerialWriter,
	VIRTUAL_KEY_BITFIELD_SIZE,
	Heap,
	HeaplessSpscErrorLog<32>,
	EmbassyTickClock,
>;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> () {
	unsafe { ALLOCATOR.inner.init(HEAP.as_ptr() as usize, HEAP_SIZE) };

	let p = embassy_rp::init(Default::default());

	let cmds: Vec<Box<dyn Command<CommandContext>>> = vec![
		// identify MUST be first
		/* 0x00 */ Box::new(IdentifyCommand {}),
		/* 0x01 */ Box::new(UpdateProfileCommand {}),
		/* 0x02 */ Box::new(GetProfileCommand {}),
		/* 0x03 */ Box::new(SetExternalTagsCommand {}),
		/* 0x04 */ Box::new(RebootCommand {}),
		/* 0x05 */ Box::new(GetStatusCommand {}),
		/* 0x06 */ Box::new(SetVirtualKeysCommand::<VIRTUAL_KEY_BITFIELD_SIZE> {}),
		/* 0x07 */ Box::new(UpdateSettingsCommand {}),
		/* 0x08 */ Box::new(GetSettingsCommand {}),
	];

	let key_ids: [KeyId; ROWS * COLS] = [
		KeyId::new(Uuid::parse_str("0661ee85-348b-5d93-b5e2-ac11cfa5344b").unwrap()),
		KeyId::new(Uuid::parse_str("87c4fd79-143b-576b-afa2-bea59e4cd02c").unwrap()),
		KeyId::new(Uuid::parse_str("1d652794-96a4-5c59-9948-afd441289317").unwrap()),
		KeyId::new(Uuid::parse_str("de57737c-e6c1-5818-bf94-d126ff5304a3").unwrap()),
		KeyId::new(Uuid::parse_str("85c20588-8148-5785-9e9f-44976e8dfef8").unwrap()),
		KeyId::new(Uuid::parse_str("b6ee974a-b405-5367-8c9f-e70a75045c37").unwrap()),
		KeyId::new(Uuid::parse_str("8a1052be-8165-5976-849b-511ce92f9956").unwrap()),
		KeyId::new(Uuid::parse_str("91206d06-70d4-5b75-9fdf-aad7f367fff5").unwrap()),
		KeyId::new(Uuid::parse_str("7abd3edf-f94c-522e-b2be-06a88bdb1cc9").unwrap()),
		KeyId::new(Uuid::parse_str("a32da69a-7f91-5f5a-87d2-dd5e4776b1c4").unwrap()),
		KeyId::new(Uuid::parse_str("3a801a21-1ef7-5803-bf42-ecd1e8444656").unwrap()),
		KeyId::new(Uuid::parse_str("c54ec31f-2381-5636-b0a5-edd448294b88").unwrap()),
		KeyId::new(Uuid::parse_str("16ad3daf-bd00-5168-885a-74008ce8de35").unwrap()),
		KeyId::new(Uuid::parse_str("da390fc5-5361-5af9-9398-d3823b81ecba").unwrap()),
		KeyId::new(Uuid::parse_str("1a549b65-43d5-5068-a3f5-59429946e404").unwrap()),
		KeyId::new(Uuid::parse_str("ec06b9a0-0713-5db1-862c-20fafd2b0764").unwrap()),
		KeyId::new(Uuid::parse_str("cbfef260-a498-599f-a6c0-8a6a51002b76").unwrap()),
		KeyId::new(Uuid::parse_str("852caff2-9ef9-59a3-ae41-e5eec3fa0d21").unwrap()),
		KeyId::new(Uuid::parse_str("96148043-9890-5767-a464-1b12f126da14").unwrap()),
		KeyId::new(Uuid::parse_str("7a30b4b5-f6b1-5aae-8cf5-f28bca7c1c13").unwrap()),
		KeyId::new(Uuid::parse_str("ab6039e8-38dc-5f91-b15c-6678def87cea").unwrap()),
		KeyId::new(Uuid::parse_str("0ef29fa7-07fb-5495-bb6f-33d164eda994").unwrap()),
		KeyId::new(Uuid::parse_str("e18caa6c-d922-558e-b146-0262173a28bd").unwrap()),
		KeyId::new(Uuid::parse_str("7b3285ea-4be6-5eae-9125-cec547fa3fb1").unwrap()),
		KeyId::new(Uuid::parse_str("4ade2cba-18d3-5fd0-a6d4-ba928bb47009").unwrap()),
		KeyId::new(Uuid::parse_str("474d0b39-6165-58e0-9745-2ca79493a9e8").unwrap()),
		KeyId::new(Uuid::parse_str("67fbbc39-8540-571c-a8e7-0a8bffbdc4c0").unwrap()),
		KeyId::new(Uuid::parse_str("00a68179-7585-5f08-89fd-c63464760575").unwrap()),
		KeyId::new(Uuid::parse_str("7b743c81-7260-5ae3-8c7e-fc451751a2c7").unwrap()),
		KeyId::new(Uuid::parse_str("15c56a3d-0f31-5ebd-bcf1-63aa968be49a").unwrap()),
	];

	let flash =
		init_flash::<FLASH_DATA_SIZE>(unsafe { FLASH_DATA.as_ptr() }, p.FLASH, p.DMA_CH0).await;

	let device_id = flash.device_id;
	let mut flash = flash.flash;

	let settings_partition = FlashPartition::new(0, SETTINGS_SIZE);
	let profile_partition = FlashPartition::new(SETTINGS_SIZE, PROFILE_SIZE);

	let settings: Settings = load_settings_from_flash(&mut flash.partition(&settings_partition))
		.await
		.unwrap_or_else(|_| Settings {
			mouse_enabled: true,
		});

	static DEVICE_INFO: StaticCell<DeviceInfo> = StaticCell::new();
	let device_info = DEVICE_INFO.init(DeviceInfo {
		id: device_id,
		name: "Cardboard",
		manufacturer: "cranky",
		r#type: DeviceTypeId::new(Uuid::from_u128(0x0407db48_ca74_5783_9b11_489637b7c615)),
		variant: None,
		version: DeviceVersion::new(0x00000001),
		commands: cmds.iter().map(|cmd| cmd.info()).collect(),
	});

	static CLOCK: StaticCell<EmbassyTickClock> = StaticCell::new();
	let clock = CLOCK.init(EmbassyTickClock {});

	let tick_interval = 1.millis();

	let bootloader_key = key_ids[0];

	let rows: [Box<dyn RowPin>; ROWS] = [
		p.PIN_28.degrade(),
		p.PIN_27.degrade(),
		p.PIN_26.degrade(),
		p.PIN_22.degrade(),
		p.PIN_21.degrade(),
	]
	.map(|pin| Box::new(Output::new(pin, Level::Low)) as Box<dyn RowPin>);

	let cols: [Box<dyn ColPin>; COLS] = [
		p.PIN_16.degrade(),
		p.PIN_17.degrade(),
		p.PIN_9.degrade(),
		p.PIN_18.degrade(),
		p.PIN_19.degrade(),
		p.PIN_20.degrade(),
	]
	.map(|pin| Box::new(Input::new(pin, Pull::Down)) as Box<dyn ColPin>);

	let debounce_time = 10.millis();
	let matrix = KeyMatrix::new(key_ids, rows, cols, debounce_time);

	let profile = match load_profile_from_flash(&mut flash.partition(&profile_partition)).await {
		Ok(profile) => {
			info!("Profile loaded from flash storage");
			profile
		}
		Err(err) => {
			warn!("Failed to load profile from flash storage. Falling back to empty profile. Error: {}", err);
			KeyboardProfile::default()
		}
	};

	let hid = EmbassyKeypadHid {
		keyboard: KeyboardImpl::new(),
		mouse: MouseImpl::new(),
		consumer: ConsumerImpl::new(),
		signal: &HID_SIGNAL,
	};

	let watchdog = Watchdog::new(p.WATCHDOG);

	static REBOOT: StaticCell<EmbassyRp2040Reboot> = StaticCell::new();
	let reboot = REBOOT.init(EmbassyRp2040Reboot { watchdog });

	static BOOTLOADER: StaticCell<EmbassyRp2040RebootToBootloader> = StaticCell::new();
	let bootloader = BOOTLOADER.init(EmbassyRp2040RebootToBootloader {});

	let serial_number = get_serial_number(&device_id);

	let serial_read_timeout = 100.millis();
	let serial_write_timeout = 1.secs();
	let serial_reset_timeout = 1.secs();

	let (serial_reader, serial_writer, usb_device) = if settings.mouse_enabled {
		let usb =
			init_usb::<KeyboardImpl, MouseImpl, ConsumerImpl>(p.USB, &device_info, serial_number);
		spawner
			.spawn(hid_task(
				usb.keyboard_writer,
				usb.mouse_writer,
				usb.consumer_writer,
				&HID_SIGNAL,
			))
			.unwrap();
		(usb.serial_reader, usb.serial_writer, usb.device)
	} else {
		let usb =
			init_usb_no_mouse::<KeyboardImpl, ConsumerImpl>(p.USB, &device_info, serial_number);
		spawner
			.spawn(hid_task_no_mouse(
				usb.keyboard_writer,
				usb.consumer_writer,
				&HID_SIGNAL,
			))
			.unwrap();
		(usb.serial_reader, usb.serial_writer, usb.device)
	};

	let serial_rx = EmbassySerialPacketReader::<{ USB_SERIAL_PACKET_SIZE }>::new(
		serial_reader,
		serial_read_timeout,
	);
	let serial_rx = BufferedReader::new(serial_rx);
	let serial_tx = EmbassySerialPacketWriter::<{ USB_SERIAL_PACKET_SIZE }>::new(
		serial_writer,
		serial_write_timeout,
	);

	let error_log = HeaplessSpscErrorLog::new();

	let ctx = CommandContext::new(
		device_info,
		flash,
		settings_partition,
		profile_partition,
		&PROFILE_CHANGED_SIGNAL,
		serial_rx,
		serial_tx,
		&EXTERNAL_TAGS_CHANGED_SIGNAL,
		&VIRTUAL_KEY_SIGNAL,
		&ALLOCATOR,
		reboot,
		bootloader,
		error_log,
		clock,
	);

	spawner.spawn(usb_task(usb_device)).unwrap();

	spawner
		.spawn(keypad_task(
			clock,
			matrix,
			profile,
			hid,
			&PROFILE_CHANGED_SIGNAL,
			&EXTERNAL_TAGS_CHANGED_SIGNAL,
			&VIRTUAL_KEY_SIGNAL,
			bootloader_key,
			bootloader,
			tick_interval,
		))
		.unwrap();

	spawner
		.spawn(cmd_task(clock, cmds, ctx, serial_reset_timeout))
		.unwrap();
}

#[embassy_executor::task]
async fn keypad_task(
	clock: &'static EmbassyTickClock,
	matrix: Matrix,
	profile: KeyboardProfile,
	hid: EmbassyKeypadHid<KeyboardImpl, MouseImpl, ConsumerImpl, Mutex>,
	profile_changed: &'static Signal<KeyboardProfile>,
	tags_changed: &'static Signal<Vec<LayerTag>>,
	virtual_keys_changed: &'static Signal<[u8; VIRTUAL_KEY_BITFIELD_SIZE]>,
	bootloader_key: KeyId,
	bootloader: &'static EmbassyRp2040RebootToBootloader,
	interval: Duration,
) {
	cardboard_lib::tasks::keypad_task(
		clock,
		matrix,
		profile,
		hid,
		profile_changed,
		tags_changed,
		virtual_keys_changed,
		Some(bootloader_key),
		bootloader,
		interval,
	)
	.await
}

#[embassy_executor::task]
async fn cmd_task(
	clock: &'static EmbassyTickClock,
	cmds: Vec<Box<dyn Command<CommandContext>>>,
	ctx: CommandContext,
	timeout: Duration,
) {
	cardboard_lib::tasks::cmd_task(clock, cmds, ctx, timeout).await;
}

#[embassy_executor::task]
async fn hid_task(
	keyboard: HidWriter<'static, Driver<'static, USB>, { KeyboardImpl::SIZE }>,
	mouse: HidWriter<'static, Driver<'static, USB>, { MouseImpl::SIZE }>,
	consumer: HidWriter<'static, Driver<'static, USB>, { ConsumerImpl::SIZE }>,
	signal: &'static Signal<
		HidReport<{ KeyboardImpl::SIZE }, { MouseImpl::SIZE }, { ConsumerImpl::SIZE }>,
	>,
) {
	cardboard::rp2040::hid::hid_task(keyboard, mouse, consumer, signal).await;
}
#[embassy_executor::task]
async fn hid_task_no_mouse(
	keyboard: HidWriter<'static, Driver<'static, USB>, { KeyboardImpl::SIZE }>,
	consumer: HidWriter<'static, Driver<'static, USB>, { ConsumerImpl::SIZE }>,
	signal: &'static Signal<
		HidReport<{ KeyboardImpl::SIZE }, { MouseImpl::SIZE }, { ConsumerImpl::SIZE }>,
	>,
) {
	cardboard::rp2040::hid::hid_task_no_mouse(keyboard, consumer, signal).await;
}

const SETTINGS_VERSION: u32 = 1;

struct Settings {
	mouse_enabled: bool,
}

impl Readable for Settings {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let version = reader
			.read_u32()
			.await
			.ok_or("Could not read settings version")?;

		if version != SETTINGS_VERSION {
			return Err("Unsupported settings version");
		}

		Ok(Self {
			mouse_enabled: reader
				.read_bool()
				.await
				.ok_or("Could not read mouse enabled")?,
		})
	}
}

// impl Writeable for Settings {
// 	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
// 		writer
// 			.write_u32(SETTINGS_VERSION)
// 			.await
// 			.map_err(|_| "Could not write settings version")?;
// 		writer
// 			.write_bool(self.mouse_enabled)
// 			.await
// 			.map_err(|_| "Could not write mouse enabled")
// 	}
// }
