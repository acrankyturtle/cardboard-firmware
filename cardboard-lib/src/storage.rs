use crate::{profile::KeyboardProfile, serialize::Readable, stream::ReadAsyncExt};

pub trait BlockFlash {
	fn as_slice(&self) -> &'static [u8];
	fn erase(&mut self, offset: usize, length: usize) -> Result<(), &'static str>;
	fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), &'static str>;
	fn length(&self) -> usize;

	const ERASE_BLOCK_SIZE: usize;
	const WRITE_BLOCK_SIZE: usize;
}

// pub trait FlashMemory {
// 	fn as_slice(&self) -> &'static [u8];
// 	fn erase_all(&mut self) -> Result<(), &'static str>;
// 	fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), &'static str>;
// 	fn length(&self) -> usize;
// }

pub struct FlashPartition<Flash: BlockFlash + ?Sized> {
	start: usize,
	length: usize,
	_marker: core::marker::PhantomData<Flash>,
}

impl<Flash: BlockFlash> FlashPartition<Flash> {
	pub fn new(start: usize, length: usize) -> Self {
		Self {
			start,
			length,
			_marker: core::marker::PhantomData,
		}
	}
}

pub struct PartitionedFlashMemory<'a, Flash: BlockFlash + ?Sized> {
	flash: &'a mut Flash,
	partition: &'a FlashPartition<Flash>,
}

impl<'a, Flash: BlockFlash + ?Sized> PartitionedFlashMemory<'a, Flash> {
	pub fn new(flash: &'a mut Flash, partition: &'a FlashPartition<Flash>) -> Self {
		Self { flash, partition }
	}
}

impl<'a, Flash: BlockFlash + ?Sized> BlockFlash for PartitionedFlashMemory<'a, Flash> {
	fn as_slice(&self) -> &'static [u8] {
		let start = self.partition.start;
		let end = start + self.partition.length;
		&self.flash.as_slice()[start..end]
	}

	fn erase(&mut self, offset: usize, length: usize) -> Result<(), &'static str> {
		let start = self.partition.start + offset;
		self.flash.erase(start, length)
	}

	fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), &'static str> {
		let start = self.partition.start + offset;
		self.flash.write(start, data)
	}

	fn length(&self) -> usize {
		self.partition.length
	}

	const ERASE_BLOCK_SIZE: usize = Flash::ERASE_BLOCK_SIZE;

	const WRITE_BLOCK_SIZE: usize = Flash::WRITE_BLOCK_SIZE;
}

pub trait BlockFlashExt: BlockFlash {
	fn partition<'p>(
		&'p mut self,
		partition: &'p FlashPartition<Self>,
	) -> PartitionedFlashMemory<'p, Self> {
		PartitionedFlashMemory::new(self, partition)
	}

	fn erase_all(&mut self) -> Result<(), &'static str> {
		self.erase(0, self.length())
	}

	fn erase_at_least(&mut self, length: usize) -> Result<(), &'static str> {
		let erase_block_size = Self::ERASE_BLOCK_SIZE;
		let blocks_needed = (length + erase_block_size - 1) / erase_block_size;
		let erase_length = blocks_needed * erase_block_size;
		self.erase(0, erase_length)
	}
}

impl<T: BlockFlash> BlockFlashExt for T {}

pub async fn load_settings_from_flash<F: BlockFlash, Settings>(
	flash: &mut F,
) -> Result<Settings, &'static str>
where
	Settings: Readable,
{
	let mut data = flash.as_slice();
	let length = data
		.read_u16()
		.await
		.ok_or("Failed to read settings length")? as usize;
	data = &data[..length];
	Settings::read_from(&mut data).await
}

pub async fn save_settings_to_flash<F: BlockFlash>(
	flash: &mut F,
	settings: &[u8],
) -> Result<(), &'static str> {
	if settings.len() + 2 > flash.length() {
		return Err("Settings data exceeds flash memory length");
	}

	let length = settings.len();
	flash.erase_at_least(length)?;
	flash.write(0, &(length as u16).to_le_bytes())?;
	flash.write(2, settings)?;
	Ok(())
}

pub async fn load_profile_from_flash<F: BlockFlash>(
	flash: &mut F,
) -> Result<KeyboardProfile, &'static str> {
	let mut data = flash.as_slice();
	let length = data
		.read_u16()
		.await
		.ok_or("Failed to read profile length")? as usize;
	if data.len() < length {
		return Err("Profile data in flash is shorter than expected length");
	}
	data = &data[..length];

	KeyboardProfile::read_from(&mut data).await
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::test::test::*;

	#[tokio::test]
	async fn can_deserialize_cranky_profile() {
		let read_data = get_cranky_profile_data();

		let mut flash = FakeFlashMemory::new(Some(read_data), None);
		let result = load_profile_from_flash(&mut flash).await;
		assert!(
			result.is_ok(),
			"Failed to load profile from flash: {:?}",
			result.err().unwrap()
		);
	}
}
