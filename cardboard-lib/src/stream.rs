use crate::serialize::{Readable, Writeable};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use defmt::error;
use uuid::Uuid;

pub trait ReadAsync {
	async fn read_exact(&mut self, to_fill: &mut [u8]) -> Result<(), &'static str>;
}

pub trait WriteAsync {
	async fn write_exact(&mut self, data: &[u8]) -> Result<(), &'static str>;
}

pub trait ReadAsyncExt: ReadAsync {
	async fn read_bool(&mut self) -> Option<bool>;
	async fn read_u8(&mut self) -> Option<u8>;
	async fn read_u16(&mut self) -> Option<u16>;
	async fn read_u32(&mut self) -> Option<u32>;
	async fn read_u64(&mut self) -> Option<u64>;

	async fn read_utf8<'a>(&mut self, buf: &'a mut [u8]) -> Option<&'a str>;

	async fn read_uuid(&mut self) -> Option<Uuid>;
	async fn read_collection_u8<R: Readable>(&mut self) -> Option<Vec<R>>;
	async fn read_collection_u16<R: Readable>(&mut self) -> Option<Vec<R>>;
	async fn read_collection_u32<R: Readable>(&mut self) -> Option<Vec<R>>;
	async fn read_string_u8(&mut self) -> Option<String>;
	async fn read_string_u16(&mut self) -> Option<String>;
	async fn read_string_u32(&mut self) -> Option<String>;
	async fn read_option<R: Readable>(&mut self) -> Option<Option<R>>;
}

pub trait WriteAsyncExt: WriteAsync {
	async fn write_bool(&mut self, value: bool) -> Result<(), &'static str>;
	async fn write_u8(&mut self, value: u8) -> Result<(), &'static str>;
	async fn write_u16(&mut self, value: u16) -> Result<(), &'static str>;
	async fn write_u32(&mut self, value: u32) -> Result<(), &'static str>;
	async fn write_u64(&mut self, value: u64) -> Result<(), &'static str>;

	async fn write_utf8(&mut self, value: &str) -> Result<(), &'static str>;
	async fn write_uuid(&mut self, value: Uuid) -> Result<(), &'static str>;
	async fn write_collection_u8<R: Writeable>(&mut self, value: &[R]) -> Result<(), &'static str>;
	async fn write_collection_u16<R: Writeable>(&mut self, value: &[R])
	-> Result<(), &'static str>;
	async fn write_collection_u32<R: Writeable>(&mut self, value: &[R])
	-> Result<(), &'static str>;
	async fn write_string_u8(&mut self, value: &str) -> Result<(), &'static str>;
	async fn write_string_u16(&mut self, value: &str) -> Result<(), &'static str>;
	async fn write_string_u32(&mut self, value: &str) -> Result<(), &'static str>;
	async fn write_option<R: Writeable>(&mut self, r: Option<R>) -> Result<(), &'static str>;
}

impl<T: ReadAsync> ReadAsyncExt for T {
	async fn read_bool(&mut self) -> Option<bool> {
		let value = self.read_u8().await?;
		Some(value != 0)
	}
	async fn read_u8(&mut self) -> Option<u8> {
		let mut buf = [0];
		self.read_exact(&mut buf).await.ok()?;
		Some(buf[0])
	}
	async fn read_u16(&mut self) -> Option<u16> {
		let mut buf = [0; 2];
		self.read_exact(&mut buf).await.ok()?;
		Some(u16::from_le_bytes(buf))
	}

	async fn read_u32(&mut self) -> Option<u32> {
		let mut buf = [0; 4];
		self.read_exact(&mut buf).await.ok()?;
		Some(u32::from_le_bytes(buf))
	}

	async fn read_u64(&mut self) -> Option<u64> {
		let mut buf = [0; 8];
		self.read_exact(&mut buf).await.ok()?;
		Some(u64::from_le_bytes(buf))
	}

	async fn read_utf8<'a>(&mut self, buf: &'a mut [u8]) -> Option<&'a str> {
		self.read_exact(buf).await.ok()?;
		core::str::from_utf8(buf).ok()
	}

	async fn read_uuid(&mut self) -> Option<Uuid> {
		let mut buf = [0; 16];
		self.read_exact(&mut buf).await.ok()?;
		Uuid::from_slice_le(&buf).ok()
	}

	async fn read_collection_u8<R: Readable>(&mut self) -> Option<Vec<R>> {
		let num_items = self.read_u8().await? as usize;
		let mut items = Vec::with_capacity(num_items);
		for _ in 0..num_items {
			let item = match R::read_from(self).await {
				Ok(item) => item,
				Err(e) => {
					error!("Failed to read collection item: {}", e);
					return None;
				}
			};
			items.push(item);
		}
		Some(items)
	}

	async fn read_collection_u16<R: Readable>(&mut self) -> Option<Vec<R>> {
		let num_items = self.read_u16().await? as usize;
		let mut items = Vec::with_capacity(num_items);
		for _ in 0..num_items {
			let item = R::read_from(self).await.ok()?;
			items.push(item);
		}
		Some(items)
	}

	async fn read_collection_u32<R: Readable>(&mut self) -> Option<Vec<R>> {
		let num_items = self.read_u32().await? as usize;
		let mut items = Vec::with_capacity(num_items);
		for _ in 0..num_items {
			let item = R::read_from(self).await.ok()?;
			items.push(item);
		}
		Some(items)
	}

	async fn read_string_u8(&mut self) -> Option<String> {
		let length = self.read_u8().await?;
		let mut buf = vec![0; length as usize];
		self.read_exact(&mut buf).await.ok()?;
		let str = String::from_utf8(buf).ok()?;
		Some(str)
	}

	async fn read_string_u16(&mut self) -> Option<String> {
		let bytes = self.read_collection_u16::<u8>().await?;
		let str = String::from_utf8(bytes).ok()?;
		Some(str)
	}

	async fn read_string_u32(&mut self) -> Option<String> {
		let bytes = self.read_collection_u32::<u8>().await?;
		let str = String::from_utf8(bytes).ok()?;
		Some(str)
	}
	async fn read_option<R: Readable>(&mut self) -> Option<Option<R>> {
		let has_value = self.read_bool().await?;
		if has_value {
			let value = R::read_from(self).await.ok()?;
			Some(Some(value))
		} else {
			Some(None)
		}
	}
}

impl Readable for u8 {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		reader.read_u8().await.ok_or("Failed to read next byte")
	}
}

impl<T: WriteAsync> WriteAsyncExt for T {
	async fn write_bool(&mut self, value: bool) -> Result<(), &'static str> {
		self.write_u8(if value { 1 } else { 0 }).await
	}
	async fn write_u8(&mut self, value: u8) -> Result<(), &'static str> {
		self.write_exact(&[value]).await
	}

	async fn write_u16(&mut self, value: u16) -> Result<(), &'static str> {
		let data: [u8; 2] = value.to_le_bytes();
		self.write_exact(&data).await
	}

	async fn write_u32(&mut self, value: u32) -> Result<(), &'static str> {
		let data: [u8; 4] = value.to_le_bytes();
		self.write_exact(&data).await
	}

	async fn write_u64(&mut self, value: u64) -> Result<(), &'static str> {
		let data: [u8; 8] = value.to_le_bytes();
		self.write_exact(&data).await
	}

	async fn write_utf8(&mut self, value: &str) -> Result<(), &'static str> {
		self.write_exact(value.as_bytes()).await
	}

	async fn write_uuid(&mut self, value: Uuid) -> Result<(), &'static str> {
		let bytes = value.to_bytes_le();
		self.write_exact(&bytes).await
	}

	async fn write_collection_u8<R: Writeable>(&mut self, value: &[R]) -> Result<(), &'static str> {
		self.write_u8(value.len() as u8).await?;
		for item in value {
			item.write_to(self).await?;
		}
		Ok(())
	}

	async fn write_collection_u16<R: Writeable>(
		&mut self,
		value: &[R],
	) -> Result<(), &'static str> {
		self.write_u16(value.len() as u16).await?;
		for item in value {
			item.write_to(self).await?;
		}
		Ok(())
	}

	async fn write_collection_u32<R: Writeable>(
		&mut self,
		value: &[R],
	) -> Result<(), &'static str> {
		self.write_u32(value.len() as u32).await?;
		for item in value {
			item.write_to(self).await?;
		}
		Ok(())
	}

	async fn write_string_u8(&mut self, value: &str) -> Result<(), &'static str> {
		let bytes = value.as_bytes();
		self.write_u8(bytes.len() as u8).await?;
		self.write_exact(bytes).await
	}

	async fn write_string_u16(&mut self, value: &str) -> Result<(), &'static str> {
		let bytes = value.as_bytes();
		self.write_u16(bytes.len() as u16).await?;
		self.write_exact(bytes).await
	}

	async fn write_string_u32(&mut self, value: &str) -> Result<(), &'static str> {
		let bytes = value.as_bytes();
		self.write_u32(bytes.len() as u32).await?;
		self.write_exact(bytes).await
	}

	async fn write_option<R: Writeable>(&mut self, r: Option<R>) -> Result<(), &'static str> {
		if let Some(r) = r {
			self.write_u8(1).await?;
			r.write_to(self).await
		} else {
			self.write_u8(0).await
		}
	}
}

impl ReadAsync for &[u8] {
	async fn read_exact(&mut self, to_fill: &mut [u8]) -> Result<(), &'static str> {
		if to_fill.len() > self.len() {
			return Err("Not enough data to read");
		}

		to_fill.copy_from_slice(&self[..to_fill.len()]);
		*self = &self[to_fill.len()..];
		Ok(())
	}
}

impl<'a> WriteAsync for &'a mut [u8] {
	async fn write_exact(&mut self, data: &[u8]) -> Result<(), &'static str> {
		if data.len() > self.len() {
			return Err("Not enough space to write");
		}

		self[..data.len()].copy_from_slice(data);
		let tmp = core::mem::replace(self, &mut []);
		*self = &mut tmp[data.len()..];
		Ok(())
	}
}
