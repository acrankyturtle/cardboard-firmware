use core::fmt::Display;

use alloc::{string::String, string::ToString, vec::Vec};
use defmt::Format;
use uuid::Uuid;

use crate::{
	command::CommandInfo,
	serialize::Writeable,
	stream::{WriteAsync, WriteAsyncExt},
};

#[derive(Copy, Clone, PartialEq)]
pub struct DeviceId(Uuid);

impl DeviceId {
	pub const fn new(id: Uuid) -> Self {
		DeviceId(id)
	}
}

impl Display for DeviceId {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		self.0.fmt(f)
	}
}

impl Writeable for DeviceId {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		writer.write_uuid(self.0).await
	}
}

#[derive(Copy, Clone, PartialEq)]
pub struct DeviceTypeId(Uuid);

impl DeviceTypeId {
	pub const fn new(id: Uuid) -> Self {
		DeviceTypeId(id)
	}
}

impl Writeable for DeviceTypeId {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		writer.write_uuid(self.0).await
	}
}

#[derive(Copy, Clone, PartialEq)]
pub struct DeviceVersion(u32);

impl DeviceVersion {
	pub const fn new(version: u32) -> Self {
		DeviceVersion(version)
	}
}

impl Writeable for DeviceVersion {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		writer.write_u32(self.0).await
	}
}

#[derive(Copy, Clone, PartialEq)]
pub struct DeviceVariant(u32);

impl DeviceVariant {
	pub const fn new(variant: u32) -> Self {
		DeviceVariant(variant)
	}
}

impl Writeable for DeviceVariant {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		writer.write_u32(self.0).await
	}
}

#[derive(Copy, Clone, PartialEq)]
pub struct CommandId(pub Uuid);

impl Display for CommandId {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		self.0.fmt(f)
	}
}

impl Format for CommandId {
	fn format(&self, fmt: defmt::Formatter) {
		self.0.to_string().format(fmt);
	}
}

pub struct DeviceInfo {
	pub id: DeviceId,
	pub name: &'static str,
	pub manufacturer: &'static str,
	pub r#type: DeviceTypeId,
	pub variant: Option<DeviceVariant>,
	pub version: DeviceVersion,
	pub commands: Vec<CommandInfo>,
}

impl Writeable for DeviceInfo {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		self.id.write_to(writer).await?;
		writer.write_string_u8(self.name).await?;
		writer.write_string_u8(self.manufacturer).await?;
		self.r#type.write_to(writer).await?;
		writer.write_option(self.variant).await?;
		self.version.write_to(writer).await?;
		writer.write_collection_u8(&self.commands).await?;
		Ok(())
	}
}

pub struct DeviceOptions {
	pub name: String,
	pub mouse_enabled: bool,
}

impl Default for DeviceOptions {
	fn default() -> Self {
		DeviceOptions {
			name: "Cardboard Device".to_string(),
			mouse_enabled: false,
		}
	}
}

impl Writeable for DeviceOptions {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		writer.write_string_u8(&self.name).await?;
		writer.write_bool(self.mouse_enabled).await?;
		Ok(())
	}
}
