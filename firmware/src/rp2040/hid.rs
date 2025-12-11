use cardboard_lib::hid::HidReport;
use defmt::{info, warn};
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_sync::{blocking_mutex::raw::RawMutex, signal::Signal};
use embassy_time::Timer;
use embassy_usb::class::hid::HidWriter;

pub async fn hid_task<
	Mutex: RawMutex,
	const KEYBOARD_PACKET_SIZE: usize,
	const MOUSE_PACKET_SIZE: usize,
	const CONSUMER_PACKET_SIZE: usize,
>(
	mut keyboard: HidWriter<'static, Driver<'static, USB>, KEYBOARD_PACKET_SIZE>,
	mut mouse: HidWriter<'static, Driver<'static, USB>, MOUSE_PACKET_SIZE>,
	mut consumer: HidWriter<'static, Driver<'static, USB>, CONSUMER_PACKET_SIZE>,
	signal: &'static Signal<
		Mutex,
		HidReport<KEYBOARD_PACKET_SIZE, MOUSE_PACKET_SIZE, CONSUMER_PACKET_SIZE>,
	>,
) {
	info!("HID task started.");

	Timer::after_secs(1).await;
	keyboard.ready().await;
	mouse.ready().await;
	consumer.ready().await;

	info!("HID ready.");

	loop {
		let report: HidReport<KEYBOARD_PACKET_SIZE, MOUSE_PACKET_SIZE, CONSUMER_PACKET_SIZE> =
			signal.wait().await;
		if let Some(keyboard_report) = report.keyboard {
			let result = keyboard.write(&keyboard_report[..]).await;
			if let Err(e) = result {
				warn!("Error writing keyboard report: {:?}", e);
			}
		}
		if let Some(mouse_report) = report.mouse {
			let result = mouse.write(&mouse_report[..]).await;
			if let Err(e) = result {
				warn!("Error writing mouse report: {:?}", e);
			}
		}
		if let Some(consumer_report) = report.consumer {
			let result = consumer.write(&consumer_report[..]).await;
			if let Err(e) = result {
				warn!("Error writing consumer report: {:?}", e);
			}
		}
	}
}

pub async fn hid_task_no_mouse<
	Mutex: RawMutex,
	const KEYBOARD_PACKET_SIZE: usize,
	const MOUSE_PACKET_SIZE: usize,
	const CONSUMER_PACKET_SIZE: usize,
>(
	mut keyboard: HidWriter<'static, Driver<'static, USB>, KEYBOARD_PACKET_SIZE>,
	mut consumer: HidWriter<'static, Driver<'static, USB>, CONSUMER_PACKET_SIZE>,
	signal: &'static Signal<
		Mutex,
		HidReport<KEYBOARD_PACKET_SIZE, MOUSE_PACKET_SIZE, CONSUMER_PACKET_SIZE>,
	>,
) {
	info!("HID task started.");

	Timer::after_secs(1).await;
	keyboard.ready().await;
	consumer.ready().await;

	info!("HID ready.");

	loop {
		let report: HidReport<KEYBOARD_PACKET_SIZE, MOUSE_PACKET_SIZE, CONSUMER_PACKET_SIZE> =
			signal.wait().await;
		if let Some(keyboard_report) = report.keyboard {
			let result = keyboard.write(&keyboard_report[..]).await;
			if let Err(e) = result {
				warn!("Error writing keyboard report: {:?}", e);
			}
		}
		if let Some(consumer_report) = report.consumer {
			let result = consumer.write(&consumer_report[..]).await;
			if let Err(e) = result {
				warn!("Error writing consumer report: {:?}", e);
			}
		}
	}
}
