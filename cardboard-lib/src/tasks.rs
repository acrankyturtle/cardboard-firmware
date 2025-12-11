use crate::command::Command;
use crate::context::{
	ContextErrorLog, ContextSerialRx, ExternalTagsSignalRx, RebootToBootloader,
	UpdateProfileSignalRx, VirtualKeySignalRx,
};
use crate::error::{Error, ErrorLog};
use crate::hid::ReportHid;
use crate::input::{KeyId, KeyState, UpdateMatrix};
use crate::profile::{ActionEvent, DebugEvent, KeyboardProfile, LayerEvent};
use crate::serial::SerialDrain;
use crate::state::KeyboardState;
use crate::stream::ReadAsyncExt;
use crate::time::Duration;
use alloc::boxed::Box;
use alloc::vec::Vec;
use defmt::{debug, info, warn};
use fugit::ExtU64;

pub async fn keypad_task<
	Clock: crate::time::Clock,
	Matrix: UpdateMatrix,
	Report: ReportHid,
	ProfileChanged: UpdateProfileSignalRx + 'static,
	ExternalTagsChanged: ExternalTagsSignalRx + 'static,
	const VIRTUAL_KEY_BITFIELD_BYTES: usize,
	VirtualKeysChanged: VirtualKeySignalRx<VIRTUAL_KEY_BITFIELD_BYTES> + 'static,
	Bootloader: RebootToBootloader,
>(
	clock: &Clock,
	mut matrix: Matrix,
	mut profile: KeyboardProfile,
	mut hid: Report,
	profile_changed: &'static ProfileChanged,
	tags_changed: &'static ExternalTagsChanged,
	virtual_keys_changed: &'static VirtualKeysChanged,
	bootloader_key: Option<KeyId>,
	bootloader: &'static Bootloader,
	interval: Duration,
) {
	info!("Keypad task started.");

	let mut state = KeyboardState::from(&profile);

	let mut key_actions = Vec::with_capacity(Matrix::SIZE);
	let mut macro_events = Vec::with_capacity(16);

	let mut previous_tick = clock.now();

	// check if bootloader key is pressed at startup
	if let Some(bootloader_key) = bootloader_key {
		matrix.update(0.millis(), &mut key_actions);
		if key_actions.iter().any(|k| k.key_id == bootloader_key) {
			info!("Rebooting into bootloader");
			bootloader.reboot_to_bootloader();
		}
	}

	loop {
		// check for profile change
		if let Some(new_profile) = profile_changed.try_get_changed_profile() {
			// hang onto the old external tags to apply them to the new profile
			let old_external_tags = state.get_external_tags().to_vec();
			profile = new_profile;
			state = KeyboardState::from(&profile);
			state.set_external_tags(old_external_tags);

			hid.reset();
			info!("Profile updated");
		}

		// check for external tags change
		if let Some(tags) = tags_changed.try_get_external_tags() {
			state.set_external_tags(tags);
		}

		// check for virtual keys
		if let Some(virtual_keys) = virtual_keys_changed.try_get_virtual_keys() {
			state.set_virtual_key_state(&virtual_keys);
		}

		let next_tick = previous_tick + interval;
		clock.at(next_tick).await;
		let now = clock.now();
		let dt = now - previous_tick;
		previous_tick = now;

		// read key matrix and update macro state with results
		key_actions.clear();
		matrix.update(dt, &mut key_actions);
		for key in key_actions.iter() {
			match key.action {
				KeyState::Pressed => {
					state.press_key(key.key_id);
					info!("Key pressed: {:?}", key.key_id);
				}
				KeyState::Released => {
					state.release_key(key.key_id);
				}
			}
		}

		// tick macros
		macro_events.clear();
		state.tick(dt, &mut macro_events);

		// process each macro event and update hid statess
		for macro_event in macro_events.drain(..) {
			match macro_event {
				ActionEvent::DebugAction(event) => match event {
					DebugEvent::Log(msg) => {
						info!("Debug event: {:?}", msg.as_str())
					}
				},
				ActionEvent::None => {}
				ActionEvent::Keyboard(event) => hid.report_keyboard(event),
				ActionEvent::Mouse(event) => hid.report_mouse(event),
				ActionEvent::ConsumerControl(event) => {
					hid.report_consumer(event);
				}
				ActionEvent::Layer(event) => match event {
					LayerEvent::Clear(layer) => state.remove_internal_tag(layer),
					LayerEvent::Set(layer) => state.add_internal_tag(layer),
				},
			}
		}

		hid.flush();
	}
}

pub async fn cmd_task<Clock: crate::time::Clock, Context: ContextErrorLog + ContextSerialRx>(
	clock: &Clock,
	mut cmds: Vec<Box<dyn Command<Context>>>,
	mut ctx: Context,
	serial_reset_timeout: Duration,
) {
	info!("Serial task started.");

	loop {
		let cmd_id = match ctx.serial_rx().read_u8().await {
			Some(cmd_id) => cmd_id,
			None => {
				continue;
			}
		};
		match read_cmd(cmd_id, &mut cmds, &mut ctx).await {
			Ok(_) => {
				info!("Command {} executed successfully", cmd_id);
			}
			Err(e) => {
				let error = Error {
					timestamp: clock.now(),
					message: e,
				};
				ctx.errors().push(error);

				warn!("Error: {}", e);

				let timeout_start = clock.now();
				while clock.now() - timeout_start < serial_reset_timeout {
					if !ctx.serial_rx().drop_packet().await {
						break;
					}
				}
			}
		}
	}
}

async fn read_cmd<Context: ContextSerialRx>(
	cmd_id: u8,
	cmds: &mut Vec<Box<dyn Command<Context>>>,
	ctx: &mut Context,
) -> Result<(), &'static str> {
	debug!("Serial message {} received", cmd_id);

	let cmd = match cmds.get_mut(cmd_id as usize) {
		Some(cmd) => cmd,
		None => {
			return Err("Invalid command ID")?;
		}
	};

	cmd.execute(ctx).await
}
