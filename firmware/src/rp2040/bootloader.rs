use cardboard_lib::context::{Reboot, RebootToBootloader};
use embassy_rp::{rom_data::reset_to_usb_boot, watchdog::Watchdog};

pub struct EmbassyRp2040Reboot {
	pub watchdog: Watchdog,
}

pub struct EmbassyRp2040RebootToBootloader {}

impl Reboot for EmbassyRp2040Reboot {
	fn reboot(&mut self) -> ! {
		self.watchdog.trigger_reset();
		halt()
	}
}

impl RebootToBootloader for EmbassyRp2040RebootToBootloader {
	fn reboot_to_bootloader(&self) -> ! {
		reset_to_usb_boot(0, 0);
		halt()
	}
}

fn halt() -> ! {
	loop {
		cortex_m::asm::wfi();
	}
}
