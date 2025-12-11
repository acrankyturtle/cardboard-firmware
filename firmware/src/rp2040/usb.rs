use cardboard_lib::{
	device::DeviceInfo,
	hid::{HidDevice},
	profile::{ConsumerControlEvent, KeyboardEvent, MouseEvent},
};
use defmt::info;
use embassy_rp::{
	bind_interrupts,
	peripherals::USB,
	usb::{Driver, InterruptHandler},
};
use embassy_usb::{
	class::{
		cdc_acm::{CdcAcmClass, Receiver},
		hid::HidWriter,
	},
	Builder, Config, UsbDevice,
};

use embassy_usb::class::cdc_acm::State as CdcAcmState;
use embassy_usb::class::hid::State as HidState;

use crate::StaticCell;

pub const USB_HID_KEYBOARD_PACKET_SIZE: usize = 32;
pub const USB_HID_MOUSE_PACKET_SIZE: usize = 32;
pub const USB_HID_CONSUMER_PACKET_SIZE: usize = 32;
pub const USB_SERIAL_PACKET_SIZE: usize = 64;

bind_interrupts!(struct Irqs {
	USBCTRL_IRQ => InterruptHandler<USB>;
});

#[embassy_executor::task]
pub async fn usb_task(mut usb: UsbDevice<'static, Driver<'static, USB>>) {
	info!("USB task started.");
	usb.run().await;
}

pub struct UsbDevices<
	const KEYBOARD_PACKET_SIZE: usize,
	const MOUSE_PACKET_SIZE: usize,
	const CONSUMER_PACKET_SIZE: usize,
> {
	pub keyboard_writer: HidWriter<'static, Driver<'static, USB>, KEYBOARD_PACKET_SIZE>,
	pub mouse_writer: HidWriter<'static, Driver<'static, USB>, MOUSE_PACKET_SIZE>,
	pub consumer_writer: HidWriter<'static, Driver<'static, USB>, CONSUMER_PACKET_SIZE>,
	pub serial_reader: Receiver<'static, Driver<'static, USB>>,
	pub serial_writer: embassy_usb::class::cdc_acm::Sender<'static, Driver<'static, USB>>,
	pub device: UsbDevice<'static, Driver<'static, USB>>,
}

pub fn init_usb<
	KeyboardImpl: HidDevice<KeyboardEvent>,
	MouseImpl: HidDevice<MouseEvent>,
	ConsumerImpl: HidDevice<ConsumerControlEvent>,
>(
	usb: USB,
	device_info: &DeviceInfo,
	serial_number: &'static str,
) -> UsbDevices<{ KeyboardImpl::SIZE }, { MouseImpl::SIZE }, { ConsumerImpl::SIZE }> {
	let mut usb_builder = get_usb_builder(usb, device_info, serial_number);

	let keyboard_writer = get_keyboard_writer::<KeyboardImpl>(&mut usb_builder);
	let mouse_writer = get_mouse_writer::<MouseImpl>(&mut usb_builder);
	let consumer_writer = get_consumer_writer::<ConsumerImpl>(&mut usb_builder);
	let serial_class = get_serial_class(&mut usb_builder);
	let (serial_writer, serial_reader) = serial_class.split();

	let usb_device = usb_builder.build();

	UsbDevices {
		keyboard_writer,
		mouse_writer,
		consumer_writer,
		serial_reader,
		serial_writer,
		device: usb_device,
	}
}

pub fn init_usb_no_mouse<
	KeyboardImpl: HidDevice<KeyboardEvent>,
	ConsumerImpl: HidDevice<ConsumerControlEvent>,
>(
	usb: USB,
	device_info: &DeviceInfo,
	serial_number: &'static str,
) -> UsbDevicesNoMouse<{ KeyboardImpl::SIZE }, { ConsumerImpl::SIZE }> {
	let mut usb_builder = get_usb_builder(usb, device_info, serial_number);

	let keyboard_writer = get_keyboard_writer::<KeyboardImpl>(&mut usb_builder);
	let consumer_writer = get_consumer_writer::<ConsumerImpl>(&mut usb_builder);
	let serial_class = get_serial_class(&mut usb_builder);
	let (serial_writer, serial_reader) = serial_class.split();

	let usb_device = usb_builder.build();

	UsbDevicesNoMouse {
		keyboard_writer,
		consumer_writer,
		serial_reader,
		serial_writer,
		device: usb_device,
	}
}

fn get_usb_builder(
	usb: USB,
	device_info: &DeviceInfo,
	serial_number: &'static str,
) -> Builder<'static, Driver<'static, USB>> {
	let mut config = Config::new(0xF055, 0x6969);
	config.manufacturer = Some(device_info.manufacturer);
	config.product = Some(device_info.name);
	config.serial_number = Some(serial_number);

	let config_descriptor = {
		static BUF: StaticCell<[u8; 256]> = StaticCell::new();
		BUF.init([0; 256])
	};
	let bos_descriptor = {
		static BUF: StaticCell<[u8; 256]> = StaticCell::new();
		BUF.init([0; 256])
	};
	let msos_descriptor = {
		static BUF: StaticCell<[u8; 256]> = StaticCell::new();
		BUF.init([0; 256])
	};
	let control_buf = {
		static BUF: StaticCell<[u8; 256]> = StaticCell::new();
		BUF.init([0; 256])
	};

	let driver = Driver::new(usb, Irqs);

	Builder::new(
		driver,
		config,
		config_descriptor,
		bos_descriptor,
		msos_descriptor,
		control_buf,
	)
}

fn get_keyboard_writer<KeyboardImpl: HidDevice<KeyboardEvent>>(
	usb_builder: &mut Builder<'static, Driver<'static, USB>>,
) -> HidWriter<'static, Driver<'static, USB>, { KeyboardImpl::SIZE }> {
	let keyboard_hid_config = embassy_usb::class::hid::Config {
		report_descriptor: KeyboardImpl::report_descriptor(),
		request_handler: None,
		poll_ms: 1,
		max_packet_size: USB_HID_KEYBOARD_PACKET_SIZE as u16,
	};

	static STATE: StaticCell<HidState> = StaticCell::new();
	let state = STATE.init(HidState::new());
	HidWriter::new(usb_builder, state, keyboard_hid_config)
}

fn get_mouse_writer<MouseImpl: HidDevice<MouseEvent>>(
	usb_builder: &mut Builder<'static, Driver<'static, USB>>,
) -> HidWriter<'static, Driver<'static, USB>, { MouseImpl::SIZE }> {
	let mouse_hid_config = embassy_usb::class::hid::Config {
		report_descriptor: MouseImpl::report_descriptor(),
		request_handler: None,
		poll_ms: 1,
		max_packet_size: USB_HID_MOUSE_PACKET_SIZE as u16,
	};

	static STATE: StaticCell<HidState> = StaticCell::new();
	let state = STATE.init(HidState::new());
	HidWriter::new(usb_builder, state, mouse_hid_config)
}

fn get_consumer_writer<ConsumerImpl: HidDevice<ConsumerControlEvent>>(
	usb_builder: &mut Builder<'static, Driver<'static, USB>>,
) -> HidWriter<'static, Driver<'static, USB>, { ConsumerImpl::SIZE }> {
	let consumer_hid_config = embassy_usb::class::hid::Config {
		report_descriptor: ConsumerImpl::report_descriptor(),
		request_handler: None,
		poll_ms: 1,
		max_packet_size: USB_HID_CONSUMER_PACKET_SIZE as u16,
	};

	static STATE: StaticCell<HidState> = StaticCell::new();
	let state = STATE.init(HidState::new());
	HidWriter::new(usb_builder, state, consumer_hid_config)
}

fn get_serial_class(
	usb_builder: &mut Builder<'static, Driver<'static, USB>>,
) -> CdcAcmClass<'static, Driver<'static, USB>> {
	static STATE: StaticCell<embassy_usb::class::cdc_acm::State> = StaticCell::new();
	let state = STATE.init(CdcAcmState::new());
	CdcAcmClass::new(usb_builder, state, USB_SERIAL_PACKET_SIZE as u16)
}

pub struct UsbDevicesNoMouse<const KEYBOARD_PACKET_SIZE: usize, const CONSUMER_PACKET_SIZE: usize> {
	pub keyboard_writer: HidWriter<'static, Driver<'static, USB>, KEYBOARD_PACKET_SIZE>,
	pub consumer_writer: HidWriter<'static, Driver<'static, USB>, CONSUMER_PACKET_SIZE>,
	pub serial_reader: Receiver<'static, Driver<'static, USB>>,
	pub serial_writer: embassy_usb::class::cdc_acm::Sender<'static, Driver<'static, USB>>,
	pub device: UsbDevice<'static, Driver<'static, USB>>,
}
