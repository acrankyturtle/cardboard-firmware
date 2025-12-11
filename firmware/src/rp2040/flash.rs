use cardboard_lib::{device::DeviceId, embassy::EmbassyFlashMemory};
use defmt::error;
use embassy_rp::{
	flash::{Async, Flash},
	peripherals::{DMA_CH0, FLASH},
};
use embassy_time::Timer;
use uuid::Uuid;

const FLASH_ADDR: *const u8 = 0x10000000 as *const u8;
pub const FLASH_SIZE: usize = 2 * 1024 * 1024; // 2 MB

pub async fn init_flash<const DATA_SIZE: usize>(
	flash_data: *const [u8; DATA_SIZE],
	flash: FLASH,
	dma_ch0: DMA_CH0,
) -> FlashStorage {
	// wait to initialize flash
	Timer::after_millis(10).await;
	let mut flash_memory = Flash::<_, Async, FLASH_SIZE>::new(flash, dma_ch0);
	let device_id = get_device_id(&mut flash_memory).unwrap();
	let flash =
		EmbassyFlashMemory::new(FLASH_ADDR, flash_data as *const u8, DATA_SIZE, flash_memory);

	FlashStorage { device_id, flash }
}

fn get_device_id(
	flash_memory: &mut Flash<'static, FLASH, Async, FLASH_SIZE>,
) -> Result<DeviceId, &'static str> {
	let mut bytes = [0u8; 8];
	flash_memory.blocking_unique_id(&mut bytes).map_err(|e| {
		error!("Failed to read unique ID from flash: {}", e);
		"Failed to read unique ID from flash"
	})?;

	let uuid = Uuid::new_v5(&Uuid::NAMESPACE_OID, &bytes);
	Ok(DeviceId::new(uuid))
}

pub struct FlashStorage {
	pub device_id: DeviceId,
	pub flash: EmbassyFlashMemory<'static, FLASH_SIZE>,
}
