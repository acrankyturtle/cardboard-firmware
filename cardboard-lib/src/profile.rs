extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Debug;
use defmt::Format;
use heapless::String as HeaplessString;
use num_enum::TryFromPrimitive;
use uuid::Uuid;

use crate::input::KeyId;
use crate::serialize::Readable;
use crate::state::TagList;
use crate::stream::{ReadAsync, ReadAsyncExt};

/// Maximum length for a layer tag name.
pub const MAX_LAYER_TAG_LEN: usize = 32;

const VERSION: u32 = 1;

#[derive(Default)]
pub struct KeyboardProfile {
	pub name: String,
	pub keys: Vec<DeviceKey>,
	pub virtual_keys: Vec<VirtualKey>,
	pub macros: Vec<Macro>,
}

impl Readable for KeyboardProfile {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let version = reader
			.read_u32()
			.await
			.ok_or("Failed to read profile version")?;
		if version != VERSION {
			return Err("Unsupported profile version");
		}

		let name = reader
			.read_string_u8()
			.await
			.ok_or("Failed to read profile name")?;
		let keys = reader
			.read_collection_u8()
			.await
			.ok_or("Failed to read keys")?;

		let virtual_keys = reader
			.read_collection_u8()
			.await
			.ok_or("Failed to read virtual_keys")?;
		if virtual_keys.len() > 32 {
			return Err("Number of virtual keys exceeds 32");
		}

		let macros = reader
			.read_collection_u16()
			.await
			.ok_or("Failed to read macros")?;

		Ok(KeyboardProfile {
			name,
			keys,
			virtual_keys,
			macros,
		})
	}
}

pub struct DeviceKey {
	pub id: KeyId,
	pub layers: DeviceLayers,
}

impl Readable for DeviceKey {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let id = KeyId::read_from(reader).await?;
		let layers: DeviceLayers = DeviceLayers::read_from(reader).await?;

		Ok(DeviceKey { id, layers })
	}
}

pub struct VirtualKey {
	pub layers: DeviceLayers,
}

impl Readable for VirtualKey {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let layers: DeviceLayers = DeviceLayers::read_from(reader).await?;

		Ok(VirtualKey { layers })
	}
}

pub struct DeviceLayers {
	pub layers: Vec<TaggedDeviceKeyLayer>,
	pub default_layer: DeviceKeyLayer,
}

impl DeviceLayers {
	pub fn get_active_layer(&self, tags: &TagList) -> &DeviceKeyLayer {
		match self.layers.iter().find(|layer| layer.is_match(tags)) {
			Some(layer) => &layer.layer,
			None => &self.default_layer,
		}
	}
}

impl Readable for DeviceLayers {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let layers = reader
			.read_collection_u8()
			.await
			.ok_or("Failed to read layers")?;
		let default_layer: DeviceKeyLayer = DeviceKeyLayer::read_from(reader).await?;

		Ok(DeviceLayers {
			layers,
			default_layer,
		})
	}
}

pub struct TaggedDeviceKeyLayer {
	pub tags: Vec<LayerTag>,
	pub match_type: TagMatchType,
	pub layer: DeviceKeyLayer,
}

impl TaggedDeviceKeyLayer {
	fn is_match(&self, tags: &TagList) -> bool {
		tags.matches(self.tags.as_slice(), &self.match_type)
	}
}

impl Readable for TaggedDeviceKeyLayer {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let tags = reader
			.read_collection_u8()
			.await
			.ok_or("Failed to read tags")?;

		let match_type = TagMatchType::read_from(reader).await?;

		let layer: DeviceKeyLayer = DeviceKeyLayer::read_from(reader).await?;

		Ok(TaggedDeviceKeyLayer {
			tags,
			match_type,
			layer,
		})
	}
}

pub struct DeviceKeyLayer {
	// TODO: remove this and modify state to keep track of active layer with something like Option<usize | ()>, where usize is the layer index, or where () is default layer
	pub id: LayerId,
	pub macros: Vec<MacroIndex>,
}

impl Readable for DeviceKeyLayer {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let layer_id = LayerId::read_from(reader).await?;
		let macros = reader
			.read_collection_u8()
			.await
			.ok_or("Failed to read macro bindings for key")?;

		Ok(DeviceKeyLayer {
			id: layer_id,
			macros,
		})
	}
}

pub struct Macro {
	pub id: MacroId,
	pub name: String,
	pub play_channel: Option<Channel>,
	pub cut_channels: Vec<Channel>,
	pub start_sequence: Sequence,
	pub loop_sequence: Sequence,
	pub end_sequence: Sequence,
}

impl Readable for Macro {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let id = MacroId::read_from(reader).await?;

		let name = reader
			.read_string_u8()
			.await
			.ok_or("Failed to read macro name")?;

		let play_channel = reader
			.read_option()
			.await
			.ok_or("Failed to read play channel")?;
		let cut_channels = reader
			.read_collection_u8()
			.await
			.ok_or("Failed to read cut channels")?;

		let start_sequence = Sequence::read_from(reader).await?;
		let loop_sequence = Sequence::read_from(reader).await?;
		let end_sequence = Sequence::read_from(reader).await?;

		Ok(Macro {
			id,
			name,
			play_channel,
			cut_channels,
			start_sequence,
			loop_sequence,
			end_sequence,
		})
	}
}

pub struct Sequence {
	pub actions: Vec<Action>,
}

impl Readable for Sequence {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let actions = reader
			.read_collection_u8()
			.await
			.ok_or("Failed to read actions")?;
		Ok(Sequence { actions })
	}
}

impl Default for Sequence {
	fn default() -> Self {
		Self { actions: vec![] }
	}
}

pub struct Action {
	pub predelay_ms: u64,
	pub action_event: ActionEvent,
}

impl Readable for Action {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let predelay_ms = reader
			.read_u64()
			.await
			.ok_or("Failed to read predelay ms")?;
		let action_event = ActionEvent::read_from(reader).await?;

		Ok(Action {
			predelay_ms,
			action_event,
		})
	}
}

#[derive(Clone, Debug)]
pub enum ActionEvent {
	None,
	Keyboard(KeyboardEvent),
	Mouse(MouseEvent),
	ConsumerControl(ConsumerControlEvent),
	Layer(LayerEvent),
	DebugAction(DebugEvent),
}

impl Readable for ActionEvent {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let discriminator = reader
			.read_u8()
			.await
			.ok_or("Failed to read discriminator")?;
		let value = match discriminator {
			0 => ActionEvent::None,
			1 => ActionEvent::Keyboard(KeyboardEvent::read_from(reader).await?),
			2 => ActionEvent::Mouse(MouseEvent::read_from(reader).await?),
			3 => ActionEvent::ConsumerControl(ConsumerControlEvent::read_from(reader).await?),
			4 => ActionEvent::Layer(LayerEvent::read_from(reader).await?),
			5 => ActionEvent::DebugAction(DebugEvent::read_from(reader).await?),
			_ => return Err("Invalid action event discriminator"),
		};

		Ok(value)
	}
}

pub enum TagMatchType {
	All,
	Any,
}

impl Readable for TagMatchType {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let match_type_byte = reader.read_u8().await.ok_or("Failed to read match type")?;
		match match_type_byte {
			0 => Ok(TagMatchType::All),
			1 => Ok(TagMatchType::Any),
			_ => Err("Invalid match type"),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerId(Uuid);

impl LayerId {
	pub const fn new(id: Uuid) -> Self {
		LayerId(id)
	}
}

impl Readable for LayerId {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let uuid = reader.read_uuid().await.ok_or("Failed to read LayerId")?;
		Ok(LayerId::new(uuid))
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroId(Uuid);

impl MacroId {
	pub const fn new(id: Uuid) -> Self {
		MacroId(id)
	}
}

impl Readable for MacroId {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let uuid = reader.read_uuid().await.ok_or("Failed to read MacroId")?;
		Ok(MacroId::new(uuid))
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Format)]
pub struct MacroIndex(u16);

impl MacroIndex {
	pub const fn new(index: u16) -> Self {
		MacroIndex(index)
	}

	pub fn get_index(&self) -> usize {
		self.0 as usize
	}
}

impl Readable for MacroIndex {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let index = reader.read_u16().await.ok_or("Failed to read MacroIndex")? as u16;
		Ok(MacroIndex(index))
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Channel(u8);

impl Channel {
	pub const fn new(id: u8) -> Self {
		Channel(id)
	}
}

impl Readable for Channel {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let id = reader
			.read_u8()
			.await
			.ok_or("Failed to read play channel")?;
		Ok(Channel::new(id))
	}
}

/// A layer tag identifier using stack-allocated string.
///
/// Layer tags are used to conditionally activate layers based on matching tags.
/// Uses heapless::String to avoid heap allocations for better embedded performance.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LayerTag(HeaplessString<MAX_LAYER_TAG_LEN>);

impl LayerTag {
	/// Create a new LayerTag from a heapless string.
	pub fn new(tag: HeaplessString<MAX_LAYER_TAG_LEN>) -> Self {
		LayerTag(tag)
	}

	/// Create a new LayerTag from a string slice.
	/// Truncates if the string exceeds MAX_LAYER_TAG_LEN.
	pub fn from_str(s: &str) -> Self {
		let mut tag = HeaplessString::new();
		// Truncate to max length if needed
		let len = s.len().min(MAX_LAYER_TAG_LEN);
		let _ = tag.push_str(&s[..len]);
		LayerTag(tag)
	}

	/// Get the tag as a string slice.
	pub fn as_str(&self) -> &str {
		self.0.as_str()
	}
}

impl Readable for LayerTag {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let str = match reader.read_string_u8().await {
			Some(s) => s,
			None => return Err("Failed to read LayerTag"),
		};
		Ok(LayerTag::from_str(&str))
	}
}

#[derive(Debug, Clone)]
pub enum KeyboardEvent {
	KeyDown(KeyboardKey),
	KeyUp(KeyboardKey),
}

impl Readable for KeyboardEvent {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let is_key_down = reader
			.read_bool()
			.await
			.ok_or("Failed to read is_key_down")?;

		if is_key_down {
			let event = KeyboardEvent::KeyDown(KeyboardKey::read_from(reader).await?);
			Ok(event)
		} else {
			let event = KeyboardEvent::KeyUp(KeyboardKey::read_from(reader).await?);
			Ok(event)
		}
	}
}

#[derive(Debug, Clone, Copy, TryFromPrimitive)]
#[repr(u8)]
pub enum KeyboardKey {
	A = 0x04,
	B = 0x05,
	C = 0x06,
	D = 0x07,
	E = 0x08,
	F = 0x09,
	G = 0x0A,
	H = 0x0B,
	I = 0x0C,
	J = 0x0D,
	K = 0x0E,
	L = 0x0F,
	M = 0x10,
	N = 0x11,
	O = 0x12,
	P = 0x13,
	Q = 0x14,
	R = 0x15,
	S = 0x16,
	T = 0x17,
	U = 0x18,
	V = 0x19,
	W = 0x1A,
	X = 0x1B,
	Y = 0x1C,
	Z = 0x1D,
	ONE = 0x1E,
	TWO = 0x1F,
	THREE = 0x20,
	FOUR = 0x21,
	FIVE = 0x22,
	SIX = 0x23,
	SEVEN = 0x24,
	EIGHT = 0x25,
	NINE = 0x26,
	ZERO = 0x27,
	ENTER = 0x28,
	ESCAPE = 0x29,
	BACKSPACE = 0x2A,
	TAB = 0x2B,
	SPACEBAR = 0x2C,
	MINUS = 0x2D,
	EQUALS = 0x2E,
	LEFT_BRACKET = 0x2F,
	RIGHT_BRACKET = 0x30,
	BACKSLASH = 0x31,
	POUND = 0x32,
	SEMICOLON = 0x33,
	QUOTE = 0x34,
	GRAVE_ACCENT = 0x35,
	COMMA = 0x36,
	PERIOD = 0x37,
	FORWARD_SLASH = 0x38,
	CAPS_LOCK = 0x39,
	F1 = 0x3A,
	F2 = 0x3B,
	F3 = 0x3C,
	F4 = 0x3D,
	F5 = 0x3E,
	F6 = 0x3F,
	F7 = 0x40,
	F8 = 0x41,
	F9 = 0x42,
	F10 = 0x43,
	F11 = 0x44,
	F12 = 0x45,
	PRINT_SCREEN = 0x46,
	SCROLL_LOCK = 0x47,
	PAUSE = 0x48,
	INSERT = 0x49,
	HOME = 0x4A,
	PAGE_UP = 0x4B,
	DELETE = 0x4C,
	END = 0x4D,
	PAGE_DOWN = 0x4E,
	RIGHT_ARROW = 0x4F,
	LEFT_ARROW = 0x50,
	DOWN_ARROW = 0x51,
	UP_ARROW = 0x52,
	KEYPAD_NUMLOCK = 0x53,
	KEYPAD_FORWARD_SLASH = 0x54,
	KEYPAD_ASTERISK = 0x55,
	KEYPAD_MINUS = 0x56,
	KEYPAD_PLUS = 0x57,
	KEYPAD_ENTER = 0x58,
	KEYPAD_ONE = 0x59,
	KEYPAD_TWO = 0x5A,
	KEYPAD_THREE = 0x5B,
	KEYPAD_FOUR = 0x5C,
	KEYPAD_FIVE = 0x5D,
	KEYPAD_SIX = 0x5E,
	KEYPAD_SEVEN = 0x5F,
	KEYPAD_EIGHT = 0x60,
	KEYPAD_NINE = 0x61,
	KEYPAD_ZERO = 0x62,
	KEYPAD_PERIOD = 0x63,
	KEYPAD_BACKSLASH = 0x64,
	APPLICATION = 0x65,
	//POWER = 0x66,
	KEYPAD_EQUALS = 0x67,
	F13 = 0x68,
	F14 = 0x69,
	F15 = 0x6A,
	F16 = 0x6B,
	F17 = 0x6C,
	F18 = 0x6D,
	F19 = 0x6E,
	F20 = 0x6F,
	F21 = 0x70,
	F22 = 0x71,
	F23 = 0x72,
	F24 = 0x73,

	MENU = 0x76,

	LEFT_CONTROL = 0xE0,
	LEFT_SHIFT = 0xE1,
	LEFT_ALT = 0xE2,
	LEFT_GUI = 0xE3,
	RIGHT_CONTROL = 0xE4,
	RIGHT_SHIFT = 0xE5,
	RIGHT_ALT = 0xE6,
	RIGHT_GUI = 0xE7,
}

impl Readable for KeyboardKey {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let value = reader.read_u8().await.ok_or("Failed to read key")?;
		Ok(KeyboardKey::try_from(value).or(Err("Failed to parse key"))?)
	}
}

#[derive(Clone, Debug)]
pub enum MouseEvent {
	ButtonDown(MouseButton),
	ButtonUp(MouseButton),
	Scroll(MouseScroll),
	Move(MouseMove),
}

impl Readable for MouseEvent {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let discriminator = reader
			.read_u8()
			.await
			.ok_or("Failed to read mouse event discriminator")?;
		let value = match discriminator {
			0 => MouseEvent::ButtonDown(MouseButton::read_from(reader).await?),
			1 => MouseEvent::ButtonUp(MouseButton::read_from(reader).await?),
			2 => MouseEvent::Scroll(MouseScroll::read_from(reader).await?),
			3 => MouseEvent::Move(MouseMove::read_from(reader).await?),
			_ => return Err("Invalid mouse event discriminator"),
		};

		Ok(value)
	}
}

#[derive(Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MouseButton {
	Left,
	Right,
	Middle,
	Back,
	Forward,
}

impl Readable for MouseButton {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let value = reader
			.read_u8()
			.await
			.ok_or("Failed to read mouse button")?;
		Ok(MouseButton::try_from(value).or(Err("Failed to parse mouse button"))?)
	}
}

#[derive(Clone, Debug)]
pub struct MouseScroll {
	pub x: i32,
	pub y: i32,
}

impl Readable for MouseScroll {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let x = reader
			.read_u16()
			.await
			.ok_or("Failed to read mouse scroll x")? as i32;
		let y = reader
			.read_u16()
			.await
			.ok_or("Failed to read mouse scroll y")? as i32;
		Ok(MouseScroll { x, y })
	}
}

#[derive(Clone, Debug)]
pub struct MouseMove {
	pub x: i32,
	pub y: i32,
}

impl Readable for MouseMove {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let x = reader
			.read_u32()
			.await
			.ok_or("Failed to read mouse move x")? as i32;
		let y = reader
			.read_u32()
			.await
			.ok_or("Failed to read mouse move y")? as i32;
		Ok(MouseMove { x, y })
	}
}

#[derive(Clone, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum ConsumerControlEvent {
	RECORD = 0xB2,
	FAST_FORWARD = 0xB3,
	REWIND = 0xB4,
	SCAN_NEXT_TRACK = 0xB5,
	SCAN_PREVIOUS_TRACK = 0xB6,
	STOP = 0xB7,
	EJECT = 0xB8,
	PLAY_PAUSE = 0xCD,
	MUTE = 0xE2,
	VOLUME_DECREMENT = 0xEA,
	VOLUME_INCREMENT = 0xE9,
	// todo: add more
}

impl Readable for ConsumerControlEvent {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let value = reader
			.read_u8()
			.await
			.ok_or("Failed to read consumer control event")?;
		Ok(ConsumerControlEvent::try_from(value)
			.or(Err("Failed to parse consumer control event"))?)
	}
}

#[derive(Clone, Debug)]
pub enum LayerEvent {
	Clear(LayerTag),
	Set(LayerTag),
}

impl Readable for LayerEvent {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let value = reader.read_bool().await.ok_or("Failed to read value")?;
		let tag = LayerTag::read_from(reader).await?;

		if value {
			Ok(LayerEvent::Clear(tag))
		} else {
			Ok(LayerEvent::Set(tag))
		}
	}
}

#[derive(Clone, Debug)]
pub enum DebugEvent {
	Log(String),
}

impl Readable for DebugEvent {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let log = reader
			.read_string_u8()
			.await
			.ok_or("Failed to read debug log")?;
		Ok(DebugEvent::Log(log))
	}
}
