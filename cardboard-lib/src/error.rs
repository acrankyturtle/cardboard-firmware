use heapless::spsc::{Iter, Queue};

use crate::{
	serialize::Writeable,
	stream::{WriteAsync, WriteAsyncExt},
	time::Instant,
};

#[derive(Clone)]
pub struct Error {
	pub timestamp: Instant,
	pub message: &'static str,
}

impl Writeable for Error {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str> {
		writer.write_u64(self.timestamp.ticks()).await?;
		writer.write_string_u8(self.message).await?;
		Ok(())
	}
}

pub trait ErrorLog {
	fn push(&mut self, error: Error);
	fn get_errors(&self) -> Self::Iter<'_>;

	type Iter<'a>: Iterator<Item = &'a Error>
	where
		Self: 'a;
}

pub struct HeaplessSpscErrorLog<const N: usize> {
	queue: Queue<Error, N>,
}

impl<const N: usize> HeaplessSpscErrorLog<N> {
	pub const fn new() -> Self {
		Self {
			queue: Queue::new(),
		}
	}
}

impl<const N: usize> ErrorLog for HeaplessSpscErrorLog<N> {
	fn push(&mut self, error: Error) {
		let mut error = error;
		while let Err(e) = self.queue.enqueue(error) {
			error = e;
			self.queue.dequeue();
		}
	}

	fn get_errors(&self) -> Iter<Error> {
		self.queue.iter()
	}

	type Iter<'a> = Iter<'a, Error>;
}
