pub type Instant = fugit::Instant<u64, 1, 1_000_000>;
pub type Duration = fugit::Duration<u64, 1, 1_000_000>;

pub trait Clock {
	fn now(&self) -> Instant;
	async fn after(&self, duration: Duration);
	async fn at(&self, instant: Instant);

	// todo: output Instant and Duration types?
}
