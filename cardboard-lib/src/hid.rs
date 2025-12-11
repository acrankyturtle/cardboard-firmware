use crate::input::KeyState;
use crate::profile::{ConsumerControlEvent, KeyboardEvent, KeyboardKey, MouseButton, MouseEvent};
use bitflags::bitflags;
use defmt::Format;

pub struct HidReport<const SIZE_K: usize, const SIZE_M: usize, const SIZE_C: usize> {
	pub keyboard: Option<[u8; SIZE_K]>,
	pub mouse: Option<[u8; SIZE_M]>,
	pub consumer: Option<[u8; SIZE_C]>,
}

pub trait ReportHid {
	fn report_keyboard(&mut self, report: &KeyboardEvent);
	fn report_mouse(&mut self, report: &MouseEvent);
	fn report_consumer(&mut self, report: &ConsumerControlEvent);
	fn flush(&mut self);
	fn reset(&mut self);
}

pub trait HidKeyboard {
	fn report(&mut self, event: &KeyboardEvent);
}

pub trait HidMouse {
	fn report(&mut self, event: &MouseEvent);
}

pub trait HidConsumerControl {
	fn report(&mut self, event: &ConsumerControlEvent);
}

pub trait HidDevice<I> {
	fn create_report(&mut self) -> Option<[u8; Self::SIZE]>;

	fn input(&mut self, input: &I);

	fn reset(&mut self);

	fn report_descriptor() -> &'static [u8];

	const SIZE: usize;
}
pub struct NKROKeyboard {
	state: [u8; NKROKeyboard::REPORT_SIZE],
}

impl NKROKeyboard {
	const REPORT_SIZE: usize = 17;
	pub fn new() -> Self {
		NKROKeyboard {
			state: [0; NKROKeyboard::REPORT_SIZE],
		}
	}
}

impl HidDevice<KeyboardEvent> for NKROKeyboard {
	fn create_report(&mut self) -> Option<[u8; NKROKeyboard::REPORT_SIZE]> {
		let mut report = [0; NKROKeyboard::REPORT_SIZE];
		report.copy_from_slice(&self.state);
		Some(report)
	}

	fn input(&mut self, input: &KeyboardEvent) {
		let (key, state) = match input {
			KeyboardEvent::KeyDown(k) => (k, KeyState::Pressed),
			KeyboardEvent::KeyUp(k) => (k, KeyState::Released),
		};

		let keycode = *key as u8;

		if (0xE0..=0xE7).contains(&keycode) {
			let modifiers: u8 = match key {
				KeyboardKey::LEFT_CONTROL => 1 << 0,
				KeyboardKey::LEFT_SHIFT => 1 << 1,
				KeyboardKey::LEFT_ALT => 1 << 2,
				KeyboardKey::LEFT_GUI => 1 << 3,
				KeyboardKey::RIGHT_CONTROL => 1 << 4,
				KeyboardKey::RIGHT_SHIFT => 1 << 5,
				KeyboardKey::RIGHT_ALT => 1 << 6,
				KeyboardKey::RIGHT_GUI => 1 << 7,
				_ => 0,
			};

			match state {
				KeyState::Pressed => {
					self.state[0] |= modifiers;
				}
				KeyState::Released => {
					self.state[0] &= !modifiers;
				}
			}

			return;
		}

		let byte_index = (keycode / 8) as usize + 1; // skip modifier byte
		let bit_index = (keycode % 8) as usize;

		match state {
			KeyState::Pressed => {
				self.state[byte_index] |= 1 << bit_index;
			}
			KeyState::Released => {
				self.state[byte_index] &= !(1 << bit_index);
			}
		}
	}

	fn reset(&mut self) {
		self.state = [0; NKROKeyboard::REPORT_SIZE];
	}

	fn report_descriptor() -> &'static [u8] {
		&[
			0x05, 0x01, // Usage Page (Generic Desktop)
			0x09, 0x06, // Usage (Keyboard)
			0xA1, 0x01, // Collection (Application)
			// Modifier byte (8 bits for Left Ctrl to Right GUI)
			0x75, 0x01, //   Report Size (1)
			0x95, 0x08, //   Report Count (8)
			0x05, 0x07, //   Usage Page (Key Codes)
			0x19, 0xE0, //   Usage Minimum (224: Left Control)
			0x29, 0xE7, //   Usage Maximum (231: Right GUI)
			0x15, 0x00, //   Logical Minimum (0)
			0x25, 0x01, //   Logical Maximum (1)
			0x81, 0x02, //   Input (Data, Variable, Absolute)
			// Key bitmap (16 bytes = 128 keys)
			0x75, 0x01, //   Report Size (1)
			0x95, 0x80, //   Report Count (128 bits = 16 bytes)
			0x05, 0x07, //   Usage Page (Key Codes)
			0x19, 0x00, //   Usage Minimum (0)
			0x29, 0x7F, //   Usage Maximum (127)
			0x15, 0x00, //   Logical Minimum (0)
			0x25, 0x01, //   Logical Maximum (1)
			0x81, 0x02, //   Input (Data, Variable, Absolute)
			// LED output report (5 LEDs + 3 padding bits)
			0x75, 0x01, //   Report Size (1)
			0x95, 0x05, //   Report Count (5)
			0x05, 0x08, //   Usage Page (LEDs)
			0x19, 0x01, //   Usage Minimum (1: Num Lock)
			0x29, 0x05, //   Usage Maximum (5: Kana)
			0x91, 0x02, //   Output (Data, Variable, Absolute)
			0x75, 0x03, //   Report Size (3)
			0x95, 0x01, //   Report Count (1)
			0x91, 0x03, //   Output (Constant)
			0xC0, // End Collection
		]
	}

	const SIZE: usize = NKROKeyboard::REPORT_SIZE;

	// const SIZE: usize = NKROKeyboard::REPORT_SIZE;
}

pub struct Mouse {
	buttons: HidMouseButtons,
	cursor: (i32, i32),
	scroll: (i32, i32),
}

impl Mouse {
	const REPORT_SIZE: usize = 5;

	pub fn new() -> Self {
		Mouse {
			buttons: HidMouseButtons::empty(),
			cursor: (0, 0),
			scroll: (0, 0),
		}
	}

	fn button_down(&mut self, button: HidMouseButtons) {
		self.buttons |= button;
	}

	fn button_up(&mut self, button: HidMouseButtons) {
		self.buttons &= !button;
	}

	fn move_cursor(&mut self, x: i32, y: i32) {
		self.cursor.0 += x;
		self.cursor.1 += y;
	}

	fn scroll(&mut self, x: i32, y: i32) {
		self.scroll.0 += x;
		self.scroll.1 += y;
	}
}

impl HidDevice<MouseEvent> for Mouse {
	fn create_report(&mut self) -> Option<[u8; Mouse::REPORT_SIZE]> {
		let buttons = self.buttons.bits();
		let x = self.cursor.0.clamp(-128, 127) as i8;
		let y = self.cursor.1.clamp(-128, 127) as i8;
		let scroll_x = self.scroll.0.clamp(-128, 127) as i8;
		let scroll_y = self.scroll.1.clamp(-128, 127) as i8;

		Some([buttons, x as u8, y as u8, scroll_x as u8, scroll_y as u8])
	}

	fn input(&mut self, input: &MouseEvent) {
		match input {
			MouseEvent::ButtonDown(button) => self.button_down(map_button(&button)),
			MouseEvent::ButtonUp(button) => self.button_up(map_button(&button)),
			MouseEvent::Move(m) => self.move_cursor(m.x, m.y),
			MouseEvent::Scroll(s) => self.scroll(s.x, s.y),
		}
	}

	fn reset(&mut self) {
		*self = Mouse::new();
	}

	fn report_descriptor() -> &'static [u8] {
		&[
			0x05, 0x01, // Usage Page (Generic Desktop)
			0x09, 0x02, // Usage (Mouse)
			0xA1, 0x01, // Collection (Application)
			0x09, 0x01, //   Usage (Pointer)
			0xA1, 0x00, //   Collection (Physical)
			// Buttons (5 buttons supported)
			0x05, 0x09, //     Usage Page (Button)
			0x19, 0x01, //     Usage Minimum (Button 1)
			0x29, 0x05, //     Usage Maximum (Button 5)
			0x15, 0x00, //     Logical Minimum (0)
			0x25, 0x01, //     Logical Maximum (1)
			0x95, 0x05, //     Report Count (5)
			0x75, 0x01, //     Report Size (1)
			0x81, 0x02, //     Input (Data, Variable, Absolute)
			0x95, 0x03, //     Report Count (3)
			0x75, 0x01, //     Report Size (1)
			0x81, 0x03, //     Input (Constant) - Padding
			// X and Y Axes
			0x05, 0x01, //     Usage Page (Generic Desktop)
			0x09, 0x30, //     Usage (X)
			0x09, 0x31, //     Usage (Y)
			0x15, 0x81, //     Logical Minimum (-127)
			0x25, 0x7F, //     Logical Maximum (127)
			0x75, 0x08, //     Report Size (8)
			0x95, 0x02, //     Report Count (2)
			0x81, 0x06, //     Input (Data, Variable, Relative)
			// Vertical Wheel
			0x09, 0x38, //     Usage (Wheel)
			0x15, 0x81, //     Logical Minimum (-127)
			0x25, 0x7F, //     Logical Maximum (127)
			0x75, 0x08, //     Report Size (8)
			0x95, 0x01, //     Report Count (1)
			0x81, 0x06, //     Input (Data, Variable, Relative)
			// Horizontal Wheel
			0x09, 0x48, //     Usage (Horizontal Wheel)
			0x15, 0x81, //     Logical Minimum (-127)
			0x25, 0x7F, //     Logical Maximum (127)
			0x75, 0x08, //     Report Size (8)
			0x95, 0x01, //     Report Count (1)
			0x81, 0x06, //     Input (Data, Variable, Relative)
			0xC0, //   End Collection
			0xC0, // End Collection
		]
	}

	const SIZE: usize = Mouse::REPORT_SIZE;

	// const SIZE: usize = Mouse::REPORT_SIZE;
}

pub struct Scroll {
	buttons: HidMouseButtons,
	scroll: (i32, i32),
}

impl Scroll {
	const REPORT_SIZE: usize = 3;

	pub fn new() -> Self {
		Scroll {
			buttons: HidMouseButtons::empty(),
			scroll: (0, 0),
		}
	}

	fn button_down(&mut self, button: HidMouseButtons) {
		self.buttons |= button;
	}

	fn button_up(&mut self, button: HidMouseButtons) {
		self.buttons &= !button;
	}

	fn scroll(&mut self, x: i32, y: i32) {
		self.scroll.0 += x;
		self.scroll.1 += y;
	}
}

impl HidDevice<MouseEvent> for Scroll {
	fn create_report(&mut self) -> Option<[u8; Scroll::REPORT_SIZE]> {
		let buttons = self.buttons.bits();
		let scroll_x = self.scroll.0.clamp(-128, 127) as i8;
		let scroll_y = self.scroll.1.clamp(-128, 127) as i8;

		Some([buttons, scroll_x as u8, scroll_y as u8])
	}

	fn input(&mut self, input: &MouseEvent) {
		match input {
			MouseEvent::ButtonDown(button) => self.button_down(map_button(&button)),
			MouseEvent::ButtonUp(button) => self.button_up(map_button(&button)),
			MouseEvent::Move(_) => {}
			MouseEvent::Scroll(s) => self.scroll(s.x, s.y),
		}
	}

	fn reset(&mut self) {
		*self = Scroll::new();
	}

	fn report_descriptor() -> &'static [u8] {
		&[
			0x05, 0x01, // Usage Page (Generic Desktop)
			0x09, 0x0E, // Usage (System Multi-Axis Controller)
			0xA1, 0x01, // Collection (Application)
			0x09, 0x01, //   Usage (Pointer)
			0xA1, 0x00, //   Collection (Physical)
			// Buttons (5 buttons supported)
			0x05, 0x09, //     Usage Page (Button)
			0x19, 0x01, //     Usage Minimum (Button 1)
			0x29, 0x05, //     Usage Maximum (Button 5)
			0x15, 0x00, //     Logical Minimum (0)
			0x25, 0x01, //     Logical Maximum (1)
			0x95, 0x05, //     Report Count (5)
			0x75, 0x01, //     Report Size (1)
			0x81, 0x02, //     Input (Data, Variable, Absolute)
			0x95, 0x03, //     Report Count (3)
			0x75, 0x01, //     Report Size (1)
			0x81, 0x03, //     Input (Constant) - Padding
			// Vertical Wheel
			0x09, 0x38, //     Usage (Wheel)
			0x15, 0x81, //     Logical Minimum (-127)
			0x25, 0x7F, //     Logical Maximum (127)
			0x75, 0x08, //     Report Size (8)
			0x95, 0x01, //     Report Count (1)
			0x81, 0x06, //     Input (Data, Variable, Relative)
			// Horizontal Wheel
			0x09, 0x48, //     Usage (Horizontal Wheel)
			0x15, 0x81, //     Logical Minimum (-127)
			0x25, 0x7F, //     Logical Maximum (127)
			0x75, 0x08, //     Report Size (8)
			0x95, 0x01, //     Report Count (1)
			0x81, 0x06, //     Input (Data, Variable, Relative)
			0xC0, //   End Collection
			0xC0, // End Collection
		]
	}

	const SIZE: usize = Scroll::REPORT_SIZE;

	// const SIZE: usize = Mouse::REPORT_SIZE;
}

pub(crate) fn map_button(key: &MouseButton) -> HidMouseButtons {
	match key {
		MouseButton::Left => HidMouseButtons::LEFT,
		MouseButton::Right => HidMouseButtons::RIGHT,
		MouseButton::Middle => HidMouseButtons::MIDDLE,
		MouseButton::Back => HidMouseButtons::BACK,
		MouseButton::Forward => HidMouseButtons::FORWARD,
	}
}

bitflags! {
	pub(crate) struct HidMouseButtons: u8 {
		const LEFT = 0b00000001;
		const RIGHT = 0b00000010;
		const MIDDLE = 0b00000100;
		const BACK = 0b00001000;
		const FORWARD = 0b00010000;
	}
}

const CONSUMER_CONTROL_REPORT_SIZE: usize = 32;

pub struct ConsumerControl {
	state: Option<[u8; CONSUMER_CONTROL_REPORT_SIZE]>,
}

impl ConsumerControl {
	pub fn new() -> Self {
		ConsumerControl { state: None }
	}

	fn get_state_or_new(&mut self) -> &mut [u8; CONSUMER_CONTROL_REPORT_SIZE] {
		self.state.get_or_insert([0; CONSUMER_CONTROL_REPORT_SIZE])
	}
}

impl HidDevice<ConsumerControlEvent> for ConsumerControl {
	fn create_report(&mut self) -> Option<[u8; CONSUMER_CONTROL_REPORT_SIZE]> {
		match self.state {
			Some(state) => {
				let mut report = [0; CONSUMER_CONTROL_REPORT_SIZE];
				report.copy_from_slice(&state);
				self.reset(); // cc device should be reset after generating a report
				Some(report)
			}
			None => None,
		}
	}

	fn input(&mut self, input: &ConsumerControlEvent) {
		let state = self.get_state_or_new();

		let cc = map_cc(&input);
		let usage = cc as u8;

		let byte_index = (usage / 8) as usize;
		let bit_index = (usage % 8) as usize;
		state[byte_index] |= 1 << bit_index;
	}

	fn reset(&mut self) {
		self.state = None;
	}

	fn report_descriptor() -> &'static [u8] {
		&[
			0x05, 0x0C, // Usage Page (Consumer)
			0x09, 0x01, // Usage (Consumer Control)
			0xA1, 0x01, // Collection (Application)
			// Bitmap for Consumer Usages (256 possible usages, 32 bytes)
			0x19, 0x00, // Usage Minimum (0)
			0x2A, 0xFF, 0x00, // Usage Maximum (255) - 16-bit due to consumer page range
			0x15, 0x00, // Logical Minimum (0)
			0x25, 0x01, // Logical Maximum (1)
			0x75, 0x01, // Report Size (1)
			0x95, 0x00, // Report Count (256)
			0x81, 0x02, // Input (Data, Variable, Absolute) - Consumer bitmap
			0xC0, // End Collection
		]
	}

	const SIZE: usize = CONSUMER_CONTROL_REPORT_SIZE;

	// const SIZE: usize = CONSUMER_CONTROL_REPORT_SIZE;
}

pub(crate) fn map_cc(key: &ConsumerControlEvent) -> Consumer {
	match key {
		ConsumerControlEvent::RECORD => Consumer::Record,
		ConsumerControlEvent::FAST_FORWARD => Consumer::FastForward,
		ConsumerControlEvent::REWIND => Consumer::Rewind,
		ConsumerControlEvent::SCAN_NEXT_TRACK => Consumer::ScanNextTrack,
		ConsumerControlEvent::SCAN_PREVIOUS_TRACK => Consumer::ScanPreviousTrack,
		ConsumerControlEvent::STOP => Consumer::Stop,
		ConsumerControlEvent::EJECT => Consumer::Eject,
		ConsumerControlEvent::PLAY_PAUSE => Consumer::PlayPause,
		ConsumerControlEvent::MUTE => Consumer::Mute,
		ConsumerControlEvent::VOLUME_DECREMENT => Consumer::VolumeDecrement,
		ConsumerControlEvent::VOLUME_INCREMENT => Consumer::VolumeIncrement,
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Format)]
#[repr(u16)]
pub(crate) enum Consumer {
	Unassigned = 0x00,
	ConsumerControl = 0x01,
	NumericKeyPad = 0x02,
	ProgrammableButtons = 0x03,
	Microphone = 0x04,
	Headphone = 0x05,
	GraphicEqualizer = 0x06,
	//0x07-0x1F Reserved
	Plus10 = 0x20,
	Plus100 = 0x21,
	AmPm = 0x22,
	//0x23-0x3F Reserved
	Power = 0x30,
	Reset = 0x31,
	Sleep = 0x32,
	SleepAfter = 0x33,
	SleepMode = 0x34,
	Illumination = 0x35,
	FunctionButtons = 0x36,
	//0x37-0x3F Reserved
	Menu = 0x40,
	MenuPick = 0x41,
	MenuUp = 0x42,
	MenuDown = 0x43,
	MenuLeft = 0x44,
	MenuRight = 0x45,
	MenuEscape = 0x46,
	MenuValueIncrease = 0x47,
	MenuValueDecrease = 0x48,
	//0x49-0x5F Reserved
	DataOnScreen = 0x60,
	ClosedCaption = 0x61,
	ClosedCaptionSelect = 0x62,
	VcrTv = 0x63,
	BroadcastMode = 0x64,
	Snapshot = 0x65,
	Still = 0x66,
	//0x67-0x7F Reserved
	Selection = 0x80,
	AssignSelection = 0x81,
	ModeStep = 0x82,
	RecallLast = 0x83,
	EnterChannel = 0x84,
	OrderMovie = 0x85,
	Channel = 0x86,
	MediaSelection = 0x87,
	MediaSelectComputer = 0x88,
	MediaSelectTV = 0x89,
	MediaSelectWWW = 0x8A,
	MediaSelectDVD = 0x8B,
	MediaSelectTelephone = 0x8C,
	MediaSelectProgramGuide = 0x8D,
	MediaSelectVideoPhone = 0x8E,
	MediaSelectGames = 0x8F,
	MediaSelectMessages = 0x90,
	MediaSelectCD = 0x91,
	MediaSelectVCR = 0x92,
	MediaSelectTuner = 0x93,
	Quit = 0x94,
	Help = 0x95,
	MediaSelectTape = 0x96,
	MediaSelectCable = 0x97,
	MediaSelectSatellite = 0x98,
	MediaSelectSecurity = 0x99,
	MediaSelectHome = 0x9A,
	MediaSelectCall = 0x9B,
	ChannelIncrement = 0x9C,
	ChannelDecrement = 0x9D,
	MediaSelectSAP = 0x9E,
	//0x9F Reserved
	VCRPlus = 0xA0,
	Once = 0xA1,
	Daily = 0xA2,
	Weekly = 0xA3,
	Monthly = 0xA4,
	//0xA5-0xAF Reserved
	Play = 0xB0,
	Pause = 0xB1,
	Record = 0xB2,
	FastForward = 0xB3,
	Rewind = 0xB4,
	ScanNextTrack = 0xB5,
	ScanPreviousTrack = 0xB6,
	Stop = 0xB7,
	Eject = 0xB8,
	RandomPlay = 0xB9,
	SelectDisc = 0xBA,
	EnterDisc = 0xBB,
	Repeat = 0xBC,
	Tracking = 0xBD,
	TrackNormal = 0xBE,
	SlowTracking = 0xBF,
	FrameForward = 0xC0,
	FrameBack = 0xC1,
	Mark = 0xC2,
	ClearMark = 0xC3,
	RepeatFromMark = 0xC4,
	ReturnToMark = 0xC5,
	SearchMarkForward = 0xC6,
	SearchMarkBackwards = 0xC7,
	CounterReset = 0xC8,
	ShowCounter = 0xC9,
	TrackingIncrement = 0xCA,
	TrackingDecrement = 0xCB,
	StopEject = 0xCC,
	PlayPause = 0xCD,
	PlaySkip = 0xCE,
	//0xCF-0xDF Reserved
	Volume = 0xE0,
	Balance = 0xE1,
	Mute = 0xE2,
	Bass = 0xE3,
	Treble = 0xE4,
	BassBoost = 0xE5,
	SurroundMode = 0xE6,
	Loudness = 0xE7,
	MPX = 0xE8,
	VolumeIncrement = 0xE9,
	VolumeDecrement = 0xEA,
	//0xEB-0xEF Reserved
	SpeedSelect = 0xF0,
	PlaybackSpeed = 0xF1,
	StandardPlay = 0xF2,
	LongPlay = 0xF3,
	ExtendedPlay = 0xF4,
	Slow = 0xF5,
	//0xF6-0xFF Reserved
	FanEnable = 0x100,
	FanSpeed = 0x101,
	LightEnable = 0x102,
	LightIlluminationLevel = 0x103,
	ClimateControlEnable = 0x104,
	RoomTemperature = 0x105,
	SecurityEnable = 0x106,
	FireAlarm = 0x107,
	PoliceAlarm = 0x108,
	Proximity = 0x109,
	Motion = 0x10A,
	DuressAlarm = 0x10B,
	HoldupAlarm = 0x10C,
	MedicalAlarm = 0x10D,
	//0x10E-0x14F Reserved
	BalanceRight = 0x150,
	BalanceLeft = 0x151,
	BassIncrement = 0x152,
	BassDecrement = 0x153,
	TrebleIncrement = 0x154,
	TrebleDecrement = 0x155,
	//0x156-0x15F Reserved
	SpeakerSystem = 0x160,
	ChannelLeft = 0x161,
	ChannelRight = 0x162,
	ChannelCenter = 0x163,
	ChannelFront = 0x164,
	ChannelCenterFront = 0x165,
	ChannelSide = 0x166,
	ChannelSurround = 0x167,
	ChannelLowFrequencyEnhancement = 0x168,
	ChannelTop = 0x169,
	ChannelUnknown = 0x16A,
	//0x16B-0x16F Reserved
	SubChannel = 0x170,
	SubChannelIncrement = 0x171,
	SubChannelDecrement = 0x172,
	AlternateAudioIncrement = 0x173,
	AlternateAudioDecrement = 0x174,
	//0x175-0x17F Reserved
	ApplicationLaunchButtons = 0x180,
	ALLaunchButtonConfigurationTool = 0x181,
	ALProgrammableButtonConfiguration = 0x182,
	ALConsumerControlConfiguration = 0x183,
	ALWordProcessor = 0x184,
	ALTextEditor = 0x185,
	ALSpreadsheet = 0x186,
	ALGraphicsEditor = 0x187,
	ALPresentationApp = 0x188,
	ALDatabaseApp = 0x189,
	ALEmailReader = 0x18A,
	ALNewsreader = 0x18B,
	ALVoicemail = 0x18C,
	ALContactsAddressBook = 0x18D,
	ALCalendarSchedule = 0x18E,
	ALTaskProjectManager = 0x18F,
	ALLogJournalTimecard = 0x190,
	ALCheckbookFinance = 0x191,
	ALCalculator = 0x192,
	ALAvCapturePlayback = 0x193,
	ALLocalMachineBrowser = 0x194,
	ALLanWanBrowser = 0x195,
	ALInternetBrowser = 0x196,
	ALRemoteNetworkingISPConnect = 0x197,
	ALNetworkConference = 0x198,
	ALNetworkChat = 0x199,
	ALTelephonyDialer = 0x19A,
	ALLogon = 0x19B,
	ALLogoff = 0x19C,
	ALLogonLogoff = 0x19D,
	ALTerminalLockScreensaver = 0x19E,
	ALControlPanel = 0x19F,
	ALCommandLineProcessorRun = 0x1A0,
	ALProcessTaskManager = 0x1A1,
	ALSelectTaskApplication = 0x1A2,
	ALNextTaskApplication = 0x1A3,
	ALPreviousTaskApplication = 0x1A4,
	ALPreemptiveHaltTaskApplication = 0x1A5,
	ALIntegratedHelpCenter = 0x1A6,
	ALDocuments = 0x1A7,
	ALThesaurus = 0x1A8,
	ALDictionary = 0x1A9,
	ALDesktop = 0x1AA,
	ALSpellCheck = 0x1AB,
	ALGrammarCheck = 0x1AC,
	ALWirelessStatus = 0x1AD,
	ALKeyboardLayout = 0x1AE,
	ALVirusProtection = 0x1AF,
	ALEncryption = 0x1B0,
	ALScreenSaver = 0x1B1,
	ALAlarms = 0x1B2,
	ALClock = 0x1B3,
	ALFileBrowser = 0x1B4,
	ALPowerStatus = 0x1B5,
	ALImageBrowser = 0x1B6,
	ALAudioBrowser = 0x1B7,
	ALMovieBrowser = 0x1B8,
	ALDigitalRightsManager = 0x1B9,
	ALDigitalWallet = 0x1BA,
	//0x-0x1BB Reserved
	ALInstantMessaging = 0x1BC,
	ALOemFeaturesTipsTutorialBrowser = 0x1BD,
	ALOemHelp = 0x1BE,
	ALOnlineCommunity = 0x1BF,
	ALEntertainmentContentBrowser = 0x1C0,
	ALOnlineShoppingBrowser = 0x1C1,
	ALSmartCardInformationHelp = 0x1C2,
	ALMarketMonitorFinanceBrowser = 0x1C3,
	ALCustomizedCorporateNewsBrowser = 0x1C4,
	ALOnlineActivityBrowser = 0x1C5,
	ALResearchSearchBrowser = 0x1C6,
	ALAudioPlayer = 0x1C7,
	//0x1C8-0x1FF Reserved
	GenericGUIApplicationControls = 0x200,
	ACNew = 0x201,
	ACOpen = 0x202,
	ACClose = 0x203,
	ACExit = 0x204,
	ACMaximize = 0x205,
	ACMinimize = 0x206,
	ACSave = 0x207,
	ACPrint = 0x208,
	ACProperties = 0x209,
	ACUndo = 0x21A,
	ACCopy = 0x21B,
	ACCut = 0x21C,
	ACPaste = 0x21D,
	ACSelectAll = 0x21E,
	ACFind = 0x21F,
	ACFindAndReplace = 0x220,
	ACSearch = 0x221,
	ACGoTo = 0x222,
	ACHome = 0x223,
	ACBack = 0x224,
	ACForward = 0x225,
	ACStop = 0x226,
	ACRefresh = 0x227,
	ACPreviousLink = 0x228,
	ACNextLink = 0x229,
	ACBookmarks = 0x22A,
	ACHistory = 0x22B,
	ACSubscriptions = 0x22C,
	ACZoomIn = 0x22D,
	ACZoomOut = 0x22E,
	ACZoom = 0x22F,
	ACFullScreenView = 0x230,
	ACNormalView = 0x231,
	ACViewToggle = 0x232,
	ACScrollUp = 0x233,
	ACScrollDown = 0x234,
	ACScroll = 0x235,
	ACPanLeft = 0x236,
	ACPanRight = 0x237,
	ACPan = 0x238,
	ACNewWindow = 0x239,
	ACTileHorizontally = 0x23A,
	ACTileVertically = 0x23B,
	ACFormat = 0x23C,
	ACEdit = 0x23D,
	ACBold = 0x23E,
	ACItalics = 0x23F,
	ACUnderline = 0x240,
	ACStrikethrough = 0x241,
	ACSubscript = 0x242,
	ACSuperscript = 0x243,
	ACAllCaps = 0x244,
	ACRotate = 0x245,
	ACResize = 0x246,
	ACFlipHorizontal = 0x247,
	ACFlipVertical = 0x248,
	ACMirrorHorizontal = 0x249,
	ACMirrorVertical = 0x24A,
	ACFontSelect = 0x24B,
	ACFontColor = 0x24C,
	ACFontSize = 0x24D,
	ACJustifyLeft = 0x24E,
	ACJustifyCenterH = 0x24F,
	ACJustifyRight = 0x250,
	ACJustifyBlockH = 0x251,
	ACJustifyTop = 0x252,
	ACJustifyCenterV = 0x253,
	ACJustifyBottom = 0x254,
	ACJustifyBlockV = 0x255,
	ACIndentDecrease = 0x256,
	ACIndentIncrease = 0x257,
	ACNumberedList = 0x258,
	ACRestartNumbering = 0x259,
	ACBulletedList = 0x25A,
	ACPromote = 0x25B,
	ACDemote = 0x25C,
	ACYes = 0x25D,
	ACNo = 0x25E,
	ACCancel = 0x25F,
	ACCatalog = 0x260,
	ACBuyCheckout = 0x261,
	ACAddToCart = 0x262,
	ACExpand = 0x263,
	ACExpandAll = 0x264,
	ACCollapse = 0x265,
	ACCollapseAll = 0x266,
	ACPrintPreview = 0x267,
	ACPasteSpecial = 0x268,
	ACInsertMode = 0x269,
	ACDelete = 0x26A,
	ACLock = 0x26B,
	ACUnlock = 0x26C,
	ACProtect = 0x26D,
	ACUnprotect = 0x26E,
	ACAttachComment = 0x26F,
	ACDeleteComment = 0x270,
	ACViewComment = 0x271,
	ACSelectWord = 0x272,
	ACSelectSentence = 0x273,
	ACSelectParagraph = 0x274,
	ACSelectColumn = 0x275,
	ACSelectRow = 0x276,
	ACSelectTable = 0x277,
	ACSelectObject = 0x278,
	ACRedoRepeat = 0x279,
	ACSort = 0x27A,
	ACSortAscending = 0x27B,
	ACSortDescending = 0x27C,
	ACFilter = 0x27D,
	ACSetClock = 0x27E,
	ACViewClock = 0x27F,
	ACSelectTimeZone = 0x280,
	ACEditTimeZones = 0x281,
	ACSetAlarm = 0x282,
	ACClearAlarm = 0x283,
	ACSnoozeAlarm = 0x284,
	ACResetAlarm = 0x285,
	ACSynchronize = 0x286,
	ACSendReceive = 0x287,
	ACSendTo = 0x288,
	ACReply = 0x289,
	ACReplyAll = 0x28A,
	ACForwardMsg = 0x28B,
	ACSend = 0x28C,
	ACAttachFile = 0x28D,
	ACUpload = 0x28E,
	ACDownloadSaveTargetAs = 0x28F,
	ACSetBorders = 0x290,
	ACInsertRow = 0x291,
	ACInsertColumn = 0x292,
	ACInsertFile = 0x293,
	ACInsertPicture = 0x294,
	ACInsertObject = 0x295,
	ACInsertSymbol = 0x296,
	ACSaveAndClose = 0x297,
	ACRename = 0x298,
	ACMerge = 0x299,
	ACSplit = 0x29A,
	ACDistributeHorizontally = 0x29B,
	ACDistributeVertically = 0x29C,
	//0x29D-0xFFFF Reserved
}

impl Default for Consumer {
	fn default() -> Self {
		Self::Unassigned
	}
}
