use crate::stream::{ReadAsync, WriteAsync};

pub trait SerialDrain {
	async fn drop_packet(&mut self) -> bool;
}

pub trait SerialPacketReader {
	async fn read_packet(&mut self, buf: &mut [u8]) -> Result<usize, &'static str>;
	const SIZE: usize;
}

pub trait SerialPacketSender {
	async fn write_packet(&mut self, data: &[u8]) -> Result<(), &'static str>;
	const SIZE: usize;
}

impl<T: SerialPacketSender> WriteAsync for T {
	async fn write_exact(&mut self, data: &[u8]) -> Result<(), &'static str> {
		let mut offset = 0;
		loop {
			let size = Self::SIZE.min(data.len() - offset);

			if size < 1 {
				break;
			}

			self.write_packet(&data[offset..offset + size]).await?;
			offset += size;
		}

		Ok(())
	}
}

pub struct BufferedReader<S: SerialPacketReader>
where
	[(); S::SIZE]:,
{
	buffer: SerialBuffer<{ S::SIZE }>,
	source: S,
}

impl<S: SerialPacketReader> BufferedReader<S>
where
	[(); S::SIZE]:,
{
	pub fn new(source: S) -> Self {
		Self {
			buffer: SerialBuffer::new(),
			source,
		}
	}

	async fn read_packet(&mut self) -> Result<(), &'static str> {
		let bytes_read = self.source.read_packet(&mut self.buffer.buffer).await?;
		self.buffer.skip = 0;
		self.buffer.length = bytes_read;
		Ok(())
	}
}

struct SerialBuffer<const SIZE: usize> {
	buffer: [u8; SIZE],
	skip: usize,
	length: usize,
}

impl<const SIZE: usize> SerialBuffer<SIZE> {
	fn new() -> Self {
		Self {
			buffer: [0; SIZE],
			skip: 0,
			length: 0,
		}
	}

	pub fn read_up_to(&mut self, to_fill: &mut [u8]) -> usize {
		let bytes_to_copy = self.length.min(to_fill.len());

		to_fill[..bytes_to_copy]
			.copy_from_slice(&self.buffer[self.skip..self.skip + bytes_to_copy]);

		self.skip += bytes_to_copy;
		self.length -= bytes_to_copy;
		bytes_to_copy
	}

	pub fn drop(&mut self) {
		self.skip = 0;
		self.length = 0;
	}
}

impl<S: SerialPacketReader> ReadAsync for BufferedReader<S>
where
	[(); S::SIZE]:,
{
	async fn read_exact(&mut self, to_fill: &mut [u8]) -> Result<(), &'static str> {
		let mut total_read = 0usize;

		total_read += self.buffer.read_up_to(to_fill);

		while total_read < to_fill.len() {
			self.read_packet().await?;
			let bytes_read = self.buffer.read_up_to(&mut to_fill[total_read..]);

			total_read += bytes_read;
		}

		Ok(())
	}
}

impl<S: SerialPacketReader + SerialDrain> SerialDrain for BufferedReader<S>
where
	[(); S::SIZE]:,
{
	async fn drop_packet(&mut self) -> bool {
		self.buffer.drop();
		self.source.drop_packet().await
	}
}

#[cfg(test)]
mod tests {
	use std::collections::VecDeque;

	use super::*;

	struct DummySerialPacketReader<'a, const SIZE: usize> {
		packets: VecDeque<&'a [u8]>,
	}

	impl<const SIZE: usize> SerialPacketReader for DummySerialPacketReader<'_, SIZE> {
		async fn read_packet(&mut self, buf: &mut [u8]) -> Result<usize, &'static str> {
			if let Some(packet) = self.packets.pop_front() {
				let size = packet.len();

				if buf.len() < size {
					return Err("Buffer too small");
				}

				buf[..size].copy_from_slice(packet);
				Ok(size)
			} else {
				Err("No more packets")
			}
		}
		const SIZE: usize = SIZE;
	}

	#[tokio::test]
	async fn read_single_packet() {
		const PACKET_SIZE: usize = 2;
		let packet: [u8; PACKET_SIZE] = [0x01, 0x02];
		let reader = DummySerialPacketReader::<PACKET_SIZE> {
			packets: VecDeque::from(vec![packet.as_slice()]),
		};
		let mut serial_reader = BufferedReader::new(reader);
		let mut buffer = [0u8; 2];

		serial_reader.read_exact(&mut buffer).await.unwrap();
		assert_eq!(buffer, [0x01, 0x02]);
	}

	#[tokio::test]
	async fn read_multiple_packets() {
		const PACKET_SIZE: usize = 2;
		let packet1: [u8; PACKET_SIZE] = [0x01, 0x02];
		let packet2: [u8; PACKET_SIZE] = [0x03, 0x04];
		let reader = DummySerialPacketReader::<PACKET_SIZE> {
			packets: VecDeque::from(vec![packet1.as_slice(), packet2.as_slice()]),
		};
		let mut serial_reader = BufferedReader::new(reader);
		let mut buffer = [0u8; 4];

		serial_reader.read_exact(&mut buffer).await.unwrap();
		assert_eq!(buffer, [0x01, 0x02, 0x03, 0x04]);
	}

	#[tokio::test]
	async fn read_partial_packet() {
		const PACKET_SIZE: usize = 4;
		let packet: [u8; PACKET_SIZE] = [0x01, 0x02, 0x03, 0x04];
		let reader = DummySerialPacketReader::<PACKET_SIZE> {
			packets: VecDeque::from(vec![packet.as_slice()]),
		};
		let mut serial_reader = BufferedReader::new(reader);
		let mut buffer = [0u8; 2];

		serial_reader.read_exact(&mut buffer).await.unwrap();
		assert_eq!(buffer, [0x01, 0x02]);
	}

	#[tokio::test]
	async fn read_partial_packet_twice() {
		const PACKET_SIZE: usize = 4;
		let packet: [u8; PACKET_SIZE] = [0x01, 0x02, 0x03, 0x04];
		let reader = DummySerialPacketReader::<PACKET_SIZE> {
			packets: VecDeque::from(vec![packet.as_slice()]),
		};
		let mut serial_reader = BufferedReader::new(reader);
		let mut buffer = [0u8; 2];

		serial_reader.read_exact(&mut buffer).await.unwrap();
		serial_reader.read_exact(&mut buffer).await.unwrap();
		assert_eq!(buffer, [0x03, 0x04]);
	}
}
