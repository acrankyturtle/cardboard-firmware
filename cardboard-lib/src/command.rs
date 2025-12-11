use crate::context::ContextClock;
use crate::context::ContextErrorLog;
use crate::context::ContextSettingsFlash;
use crate::error::Error;
use crate::error::ErrorLog;
use crate::serialize::Writeable;
use crate::storage::BlockFlash;
use crate::storage::BlockFlashExt;
use crate::storage::PartitionedFlashMemory;
use crate::time::Clock;
use async_trait::async_trait;
use core::cmp::Ord;
use core::module_path;
use core::option_env;
use core::panic;
use core::result::Result;
use core::result::Result::Err;
use core::result::Result::Ok;
use defmt::{debug, error};

use alloc::boxed::Box;
use alloc::vec::Vec;
use uuid::uuid;

use crate::context::{ContextAllocator, ContextReboot};
use crate::context::{
	ContextDeviceInfo, ContextProfileFlash, ContextSerialRx, ContextSerialTx, ContextTags,
	ContextUpdateProfile, ContextVirtualKeys, UpdateProfileSignalTx,
};
use crate::device::{CommandId, DeviceInfo};
use crate::storage::load_profile_from_flash;
use crate::stream::{ReadAsync, ReadAsyncExt, WriteAsync, WriteAsyncExt};

const CHUNK_SIZE: usize = 64; // TODO: parameterize this. for now, we hack it to the USB packet size we currently use

#[async_trait(?Send)]
pub trait Command<Context> {
	fn info(&self) -> CommandInfo;
	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str>
	where
		Context: 'async_trait;
}

pub struct IdentifyCommand;

#[async_trait(?Send)]
impl<Context: ContextDeviceInfo + ContextSerialTx> Command<Context> for IdentifyCommand {
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff")),
			name: "Identify",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str>
	where
		Context: 'async_trait,
	{
		let response = IdentifyResponse {
			info: ctx.device_info(),
		};
		response.write_to(ctx.serial_tx()).await
	}
}

const SIZEOF_PROFILE_LENGTH: usize = 2; // size of u16

pub struct UpdateProfileCommand;

impl UpdateProfileCommand {
	async fn try_execute<
		Context: ContextSerialRx + ContextSerialTx + ContextProfileFlash + ContextUpdateProfile,
	>(
		ctx: &mut Context,
	) -> Result<(), (u8, &'static str)> {
		let len = ctx.serial_rx().read_u16().await.ok_or_else(|| {
			error!("Failed to read profile length");
			(0x10u8, "Failed to read profile length")
		})? as usize;

		debug!("Profile length: {}", len);

		// clear profile flash storage
		ctx.profile_flash().erase_at_least(len).or_else(|e| {
			error!("Failed to erase profile flash storage: {:?}", e);
			Err((0x20u8, e))
		})?;

		// write profile length to flash storage
		ctx.profile_flash()
			.write(0, &(len as u16).to_le_bytes())
			.or_else(|e| {
				error!("Failed to write profile length to flash storage: {:?}", e);
				Err((0x24u8, "Failed to write profile length to flash storage"))
			})?;

		copy_serial_to_flash(ctx, |c| c.profile_flash(), SIZEOF_PROFILE_LENGTH, len)
			.await
			.map_err(|e| match e {
				CopySerialToFlashError::SerialReadError(e) => {
					error!("Failed to read profile chunk from serial port: {:?}", e);
					(0x14u8, "Failed to read profile chunk from serial port")
				}
				CopySerialToFlashError::FlashWriteError(e) => {
					error!("Failed to write profile to flash storage: {:?}", e);
					(0x28u8, "Failed to write profile to flash storage")
				}
			})?;

		// deserialize profile from flash storage
		let profile = load_profile_from_flash(&mut ctx.profile_flash())
			.await
			.map_err(|e| {
				error!("Failed to load profile from flash storage: {:?}", e);
				(0x2Cu8, "Failed to load profile from flash storage")
			})?;

		// signal profile changed
		ctx.profile_signal().update_profile(profile);

		Ok(())
	}
}

#[async_trait(?Send)]
impl<Context: ContextSerialRx + ContextSerialTx + ContextProfileFlash + ContextUpdateProfile>
	Command<Context> for UpdateProfileCommand
{
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("45963fd8-73e2-50a0-ba69-69c3333dd8af")),
			name: "Set Keyboard Profile",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		let result = Self::try_execute(ctx).await;

		let response = match result {
			Ok(_) => 0xFF,
			Err((code, _)) => code,
		};

		ctx.serial_tx().write_u8(response).await.or_else(|e| {
			error!("Failed to write response to serial port: {:?}", e);
			Err("Failed to write response")
		})?;

		match result {
			Ok(_) => Ok(()),
			Err((_, msg)) => Err(msg),
		}
	}
}

pub struct GetProfileCommand;

#[async_trait(?Send)]
impl<Context: ContextSerialTx + ContextProfileFlash> Command<Context> for GetProfileCommand {
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("e8dfdb54-f01c-5f79-9bb7-7d8d0c0c82d1")),
			name: "Get Keyboard Profile",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		let is_valid = load_profile_from_flash(&mut ctx.profile_flash())
			.await
			.is_ok();
		ctx.serial_tx()
			.write_u8(if is_valid { 0xFF } else { 0x00 })
			.await?;

		let data = ctx.profile_flash().as_slice();
		let len = u16::from_le_bytes([data[0], data[1]]) as usize;
		ctx.serial_tx().write_u16(len as u16).await?;

		let mut profile_data = &data[SIZEOF_PROFILE_LENGTH..(SIZEOF_PROFILE_LENGTH + len)];

		// write profile to serial port in chunks
		while !profile_data.is_empty() {
			let size = profile_data.len().min(CHUNK_SIZE);
			ctx.serial_tx().write_exact(&profile_data[..size]).await?;
			profile_data = &profile_data[size..];
		}

		Ok(())
	}
}

pub struct SetExternalTagsCommand;

#[async_trait(?Send)]
impl<Context: ContextSerialRx + ContextSerialTx + ContextTags> Command<Context>
	for SetExternalTagsCommand
{
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("6d84630b-03ec-57f7-806e-b1c5dee4974d")),
			name: "Set External Tags",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		let tags = ctx
			.serial_rx()
			.read_collection_u8()
			.await
			.ok_or("Failed to read tags")?;
		ctx.set_external_tags(tags);
		ctx.serial_tx().write_u8(0xFF).await?;

		Ok(())
	}
}

pub struct RebootCommand;

#[async_trait(?Send)]
impl<Context: ContextReboot + ContextSerialRx + ContextSerialTx> Command<Context>
	for RebootCommand
{
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("6dce0823-d199-5abb-a56f-a85cdba61842")),
			name: "Enter Bootloader",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		const MODE_REBOOT: u8 = 0x10;
		const MODE_REBOOT_TO_BOOTLOADER: u8 = 0x20;

		let mode = ctx
			.serial_rx()
			.read_u8()
			.await
			.ok_or("Failed to read reboot mode")?;

		match mode {
			MODE_REBOOT => ctx.reboot(),
			MODE_REBOOT_TO_BOOTLOADER => ctx.reboot_to_bootloader(),
			_ => Err("Invalid reboot mode"),
		}
	}
}

pub struct GetStatusCommand;

#[async_trait(?Send)]
impl<Context: ContextSerialTx + ContextAllocator + ContextClock + ContextErrorLog> Command<Context>
	for GetStatusCommand
{
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("b14aadb5-53a2-5e69-b463-603efce7c199")),
			name: "Get Status",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		let allocator_current = ctx.allocator().current();
		let allocator_max = ctx.allocator().max();

		let response = StatusResponse {
			now: ctx.clock().now().ticks(),
			allocator_current,
			allocator_max,
			errors: ctx.errors().get_errors().cloned().collect(),
		};

		response.write_to(ctx.serial_tx()).await
	}
}

pub struct SetVirtualKeysCommand<const VIRTUAL_KEY_BITFIELD_BYTES: usize>
where
	[(); VIRTUAL_KEY_BITFIELD_BYTES]:;

impl<const VIRTUAL_KEY_BITFIELD_BYTES: usize> SetVirtualKeysCommand<VIRTUAL_KEY_BITFIELD_BYTES>
where
	[(); VIRTUAL_KEY_BITFIELD_BYTES]:,
{
	async fn execute<
		Context: ContextSerialRx + ContextSerialTx + ContextVirtualKeys<VIRTUAL_KEY_BITFIELD_BYTES>,
	>(
		&self,
		ctx: &mut Context,
	) -> Result<(), &'static str> {
		let mut buffer = [0u8; VIRTUAL_KEY_BITFIELD_BYTES];
		ctx.serial_rx().read_exact(&mut buffer).await?;
		ctx.set_virtual_keys(buffer);
		Ok(())
	}
}

#[async_trait(?Send)]
impl<Context> Command<Context> for SetVirtualKeysCommand<1>
where
	Context: ContextSerialRx + ContextSerialTx + ContextVirtualKeys<1>,
{
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("162d99cc-5e8f-5879-97fc-c37fdb0f22a9")),
			name: "Set Virtual Key (8 keys)",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		self.execute(ctx).await
	}
}

#[async_trait(?Send)]
impl<Context> Command<Context> for SetVirtualKeysCommand<4>
where
	Context: ContextSerialRx + ContextSerialTx + ContextVirtualKeys<4>,
{
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("c1b2d3e4-f5a6-7b8c-9d0e-f1a2b3c4d5e6")),
			name: "Set Virtual Key (32 keys)",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		self.execute(ctx).await
	}
}

pub struct UpdateSettingsCommand;

impl UpdateSettingsCommand {
	async fn try_execute<Context: ContextSerialRx + ContextSerialTx + ContextSettingsFlash>(
		ctx: &mut Context,
	) -> Result<(), (u8, &'static str)> {
		let len = ctx.serial_rx().read_u16().await.ok_or_else(|| {
			error!("Failed to read settings length");
			(0x10u8, "Failed to read settings length")
		})? as usize;

		debug!("Settings length: {}", len);

		// clear settings flash storage
		ctx.settings_flash().erase_at_least(len).or_else(|e| {
			error!("Failed to erase settings flash storage: {:?}", e);
			Err((0x20u8, "Failed to erase settings flash storage"))
		})?;

		// write settings length to flash storage
		ctx.settings_flash()
			.write(0, &(len as u16).to_le_bytes())
			.or_else(|e| {
				error!("Failed to write settings length to flash storage: {:?}", e);
				Err((0x24u8, "Failed to write settings length to flash storage"))
			})?;

		copy_serial_to_flash(ctx, |c| c.settings_flash(), SIZEOF_SETTINGS_LENGTH, len)
			.await
			.map_err(|e| match e {
				CopySerialToFlashError::SerialReadError(e) => {
					error!("Failed to read settings chunk from serial port: {:?}", e);
					(0x14u8, "Failed to read settings chunk from serial port")
				}
				CopySerialToFlashError::FlashWriteError(e) => {
					error!("Failed to write settings to flash storage: {:?}", e);
					(0x28u8, "Failed to write settings to flash storage")
				}
			})?;

		Ok(())
	}
}

#[async_trait(?Send)]
impl<Context: ContextSerialRx + ContextSerialTx + ContextSettingsFlash> Command<Context>
	for UpdateSettingsCommand
{
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("a2460f18-32a8-5e57-b8c7-7adac7a096bd")),
			name: "Update Settings",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		let result = Self::try_execute(ctx).await;

		let response = match result {
			Ok(_) => 0xFF,
			Err((code, _)) => code,
		};

		ctx.serial_tx().write_u8(response).await.or_else(|e| {
			error!("Failed to write response to serial port: {:?}", e);
			Err("Failed to write response")
		})?;

		match result {
			Ok(_) => Ok(()),
			Err((_, msg)) => Err(msg),
		}
	}
}

const SIZEOF_SETTINGS_LENGTH: usize = 2; // size of u16
pub struct GetSettingsCommand;

#[async_trait(?Send)]
impl<Context: ContextSerialTx + ContextSettingsFlash> Command<Context> for GetSettingsCommand {
	fn info(&self) -> CommandInfo {
		CommandInfo {
			id: CommandId(uuid!("0062d411-70a5-55a5-a333-16706d62069f")),
			name: "Get Device Settings",
		}
	}

	async fn execute(&self, ctx: &mut Context) -> Result<(), &'static str> {
		let data = ctx.settings_flash().as_slice();
		let len = u16::from_le_bytes([data[0], data[1]]) as usize;
		ctx.serial_tx().write_u16(len as u16).await?;

		let mut settings_data = &data[SIZEOF_SETTINGS_LENGTH..(SIZEOF_SETTINGS_LENGTH + len)];

		// write to serial port in chunks
		while !settings_data.is_empty() {
			let size = settings_data.len().min(CHUNK_SIZE);
			ctx.serial_tx().write_exact(&settings_data[..size]).await?;
			settings_data = &settings_data[size..];
		}

		Ok(())
	}
}

pub struct IdentifyResponse<'a> {
	info: &'a DeviceInfo,
}

impl Writeable for IdentifyResponse<'_> {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		const VERSION: u32 = 1;
		writer.write_u32(VERSION).await?;
		self.info.write_to(writer).await
	}
}

#[derive(Clone)]
pub struct CommandInfo {
	pub id: CommandId,
	pub name: &'static str,
	// TODO: add fingerprint boolean (if true, command must write id after cmd index to confirm command execution)
}

impl Writeable for CommandInfo {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		writer.write_uuid(self.id.0).await?;
		writer.write_string_u8(self.name).await?;
		Ok(())
	}
}

struct StatusResponse {
	pub now: u64,
	pub allocator_current: usize,
	pub allocator_max: usize,
	// WISH: pub mouse_enabled: bool,
	pub errors: Vec<Error>,
}

impl Writeable for StatusResponse {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		writer.write_u64(self.now).await?;
		writer.write_u32(self.allocator_current as u32).await?;
		writer.write_u32(self.allocator_max as u32).await?;
		// WISH: writer.write_bool(self.mouse_enabled).await?;
		writer.write_collection_u8(&self.errors).await?;
		Ok(())
	}
}

async fn copy_serial_to_flash<
	Context: ContextSerialRx + ContextSerialTx,
	Flash: BlockFlash,
	GetFlash: Fn(&mut Context) -> PartitionedFlashMemory<Flash>,
>(
	ctx: &mut Context,
	get_flash: GetFlash,
	offset: usize,
	length: usize,
) -> Result<(), CopySerialToFlashError> {
	let mut total_read = 0;
	let mut buf = [0; CHUNK_SIZE];
	while total_read < length {
		let remaining = length - total_read;
		let size = remaining.min(CHUNK_SIZE);
		let chunk = &mut buf[..size];
		ctx.serial_rx()
			.read_exact(chunk)
			.await
			.map_err(|e| CopySerialToFlashError::SerialReadError(e))?;

		debug!("Writing chunk: {} bytes", size);
		let mut flash = get_flash(ctx);
		flash
			.write(offset + total_read, chunk)
			.map_err(|e| CopySerialToFlashError::FlashWriteError(e))?;
		total_read += size;
	}

	Ok(())
}

enum CopySerialToFlashError {
	SerialReadError(&'static str),
	FlashWriteError(&'static str),
}

#[cfg(test)]
mod tests {
	use crate::storage::FlashPartition;
	use crate::test::test::*;

	use super::*;

	struct FakeContext {
		flash: FakeFlashMemory,
		partition: FlashPartition<FakeFlashMemory>,
		serial_tx: FakeContextSerialTx,
	}

	struct FakeContextSerialTx {
		serial_tx: FakeSerialTx,
	}

	impl ContextProfileFlash for FakeContext {
		type Flash = FakeFlashMemory;
		fn profile_flash(&mut self) -> PartitionedFlashMemory<Self::Flash> {
			PartitionedFlashMemory::new(&mut self.flash, &self.partition)
		}
	}

	impl ContextSerialTx for FakeContext {
		type SerialTx = FakeSerialTx;

		fn serial_tx(&mut self) -> &mut Self::SerialTx {
			&mut self.serial_tx.serial_tx
		}
	}

	struct FakeSerialTx {
		written: Vec<u8>,
	}

	impl WriteAsync for FakeSerialTx {
		async fn write_exact(&mut self, _data: &[u8]) -> Result<(), &'static str> {
			self.written.extend_from_slice(_data);
			Ok(())
		}
	}

	#[tokio::test]
	async fn get_profile_command_gets_cranky_profile() {
		let cranky_profile_data = get_cranky_profile_data();

		let cmd = GetProfileCommand;
		let mut ctx = FakeContext {
			flash: FakeFlashMemory::new(Some(cranky_profile_data), None),
			partition: FlashPartition::new(0, cranky_profile_data.len()),
			serial_tx: FakeContextSerialTx {
				serial_tx: FakeSerialTx {
					written: Vec::new(),
				},
			},
		};

		cmd.execute(&mut ctx).await.unwrap();

		let expected_num_bytes_written = 1 // is_valid
			+ cranky_profile_data.len(); // profile data

		assert_eq!(
			ctx.serial_tx.serial_tx.written.len(),
			expected_num_bytes_written
		);

		assert_eq!(ctx.serial_tx.serial_tx.written.len(), 2771);

		// check is_valid byte
		assert_eq!(ctx.serial_tx.serial_tx.written[0], 0xFF);

		// check length bytes
		let length_bytes = &ctx.serial_tx.serial_tx.written[1..3];
		let length = u16::from_le_bytes([length_bytes[0], length_bytes[1]]) as usize;
		assert_eq!(length, cranky_profile_data.len() - 2);
	}
}
