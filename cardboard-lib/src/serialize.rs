use crate::stream::{ReadAsync, WriteAsync};

pub trait Readable {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized;
}

pub trait Writeable {
	async fn write_to<W: WriteAsync>(&self, writer: &mut W) -> Result<(), &'static str>;
}
