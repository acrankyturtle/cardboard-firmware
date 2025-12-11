use crate::serialize::Readable;
use crate::stream::{ReadAsync, ReadAsyncExt};
use crate::time::Duration;
use alloc::boxed::Box;
use alloc::vec::Vec;
use uuid::Uuid;

#[cfg(not(test))]
use crate::alloc::string::ToString;
#[cfg(not(test))]
use defmt::Format;

pub trait RowPin {
	fn set_high(&mut self);
	fn set_low(&mut self);
}

pub trait ColPin {
	fn is_high(&self) -> bool;
}

pub trait UpdateMatrix {
	fn update(&mut self, dt: Duration, output: &mut Vec<KeyboardAction>);
	const SIZE: usize;
}

pub struct KeyMatrix<const ROWS: usize, const COLS: usize>
where
	[(); ROWS * COLS]:,
{
	rows: [Box<dyn RowPin>; ROWS],
	cols: [Box<dyn ColPin>; COLS],
	keys: [InputKey; ROWS * COLS],
}

impl<const ROWS: usize, const COLS: usize> KeyMatrix<ROWS, COLS>
where
	[(); ROWS * COLS]:,
{
	pub fn new(
		key_ids: [KeyId; ROWS * COLS],
		rows: [Box<dyn RowPin>; ROWS],
		cols: [Box<dyn ColPin>; COLS],
		debounce_time: Duration,
	) -> Self {
		assert_eq!(key_ids.len(), ROWS * COLS);
		Self {
			rows,
			cols,
			keys: key_ids.map(|key_id| InputKey {
				id: key_id,
				prev_actual_state: KeyState::Released,
				prev_reported_state: KeyState::Released,
				keydown_time: Duration::from_ticks(0),
				debounce_time,
			}),
		}
	}

	pub fn update(&mut self, dt: Duration, output: &mut Vec<KeyboardAction>) {
		for (r, row_pin) in self.rows.iter_mut().enumerate() {
			row_pin.set_high();

			for (c, col_pin) in self.cols.iter_mut().enumerate() {
				let state = match col_pin.is_high() {
					true => KeyState::Pressed,
					false => KeyState::Released,
				};
				let key = self.keys.get_mut(Self::get_key_index(r, c)).unwrap();
				let maybe_event = key.update(state, dt);

				if let Some(event) = maybe_event {
					output.push(KeyboardAction {
						action: event,
						key_id: key.id,
					});
				}
			}

			row_pin.set_low();
		}
	}

	fn get_key_index(r: usize, c: usize) -> usize {
		r * COLS + c
	}
}

impl<const ROWS: usize, const COLS: usize> UpdateMatrix for KeyMatrix<ROWS, COLS>
where
	[(); ROWS * COLS]:,
{
	fn update(&mut self, dt: Duration, output: &mut Vec<KeyboardAction>) {
		self.update(dt, output);
	}
	const SIZE: usize = ROWS * COLS;
}

pub struct InputKey {
	id: KeyId,
	prev_actual_state: KeyState,
	prev_reported_state: KeyState,
	keydown_time: Duration,
	debounce_time: Duration,
}

impl InputKey {
	pub fn id(&self) -> KeyId {
		self.id
	}

	pub fn update(&mut self, state: KeyState, dt: Duration) -> Option<KeyState> {
		let prev_actual_state = self.prev_actual_state;
		self.keydown_time += dt;

		match (prev_actual_state, state) {
			(KeyState::Released, KeyState::Pressed) => {
				if self.prev_reported_state == KeyState::Released {
					self.keydown_time = Duration::from_ticks(0);
				}
				self.prev_actual_state = KeyState::Pressed;
			}
			(KeyState::Pressed, KeyState::Released) => {
				self.prev_actual_state = KeyState::Released;
			}
			_ => {}
		}

		let prev_reported_state = self.prev_reported_state;
		let new_state = match (self.prev_reported_state, self.prev_actual_state) {
			(KeyState::Pressed, KeyState::Released) => {
				if self.keydown_time < self.debounce_time {
					// debouncing
					KeyState::Pressed
				} else {
					KeyState::Released
				}
			}
			_ => self.prev_actual_state,
		};

		self.prev_reported_state = new_state;

		if new_state != prev_reported_state {
			Some(new_state)
		} else {
			None
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyId(Uuid);

impl KeyId {
	pub const fn new(id: Uuid) -> Self {
		KeyId(id)
	}
}

impl Readable for KeyId {
	async fn read_from<R: ReadAsync>(reader: &mut R) -> Result<Self, &'static str>
	where
		Self: Sized,
	{
		let uuid = reader.read_uuid().await.ok_or("Failed to read KeyId")?;
		Ok(KeyId::new(uuid))
	}
}

#[cfg(not(test))]
impl Format for KeyId {
	fn format(&self, fmt: defmt::Formatter) {
		self.0.to_string().format(fmt);
	}
}

#[derive(Debug, Clone, Copy)]
pub struct KeyboardAction {
	pub action: KeyState,
	pub key_id: KeyId,
}

impl KeyboardAction {
	pub fn pressed(key_id: KeyId) -> Self {
		Self {
			action: KeyState::Pressed,
			key_id,
		}
	}

	pub fn released(key_id: KeyId) -> Self {
		Self {
			action: KeyState::Released,
			key_id,
		}
	}
}

impl Default for KeyboardAction {
	fn default() -> Self {
		Self {
			action: KeyState::Released,
			key_id: KeyId(Uuid::nil()),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(test), derive(Format))]
pub enum KeyState {
	Pressed,
	Released,
}

#[cfg(test)]
mod tests {
	use alloc::rc::Rc;
	use core::cell::RefCell;

	use super::*;

	#[test]
	fn key_same_state_returns_none() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Released,
			prev_reported_state: KeyState::Released,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(0),
		};

		let result = input_key.update(KeyState::Released, Duration::from_ticks(1));

		assert_eq!(result, None);
	}

	#[test]
	fn key_pressed_returns_pressed() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Released,
			prev_reported_state: KeyState::Released,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(0),
		};

		let result = input_key.update(KeyState::Pressed, Duration::from_ticks(1));

		assert_eq!(result, Some(KeyState::Pressed));
	}

	#[test]
	fn key_released_returns_released() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Pressed,
			prev_reported_state: KeyState::Pressed,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(0),
		};

		let result = input_key.update(KeyState::Released, Duration::from_ticks(1));

		assert_eq!(result, Some(KeyState::Released));
	}

	#[test]
	fn key_pressed_and_released_returns_released() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Released,
			prev_reported_state: KeyState::Released,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(0),
		};

		_ = input_key.update(KeyState::Pressed, Duration::from_ticks(1));
		let result = input_key.update(KeyState::Released, Duration::from_ticks(1));

		assert_eq!(result, Some(KeyState::Released));
	}

	#[test]
	fn key_held_returns_no_actions() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Pressed,
			prev_reported_state: KeyState::Pressed,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(0),
		};
		let result = input_key.update(KeyState::Pressed, Duration::from_ticks(1));

		assert_eq!(result, None);
	}

	#[test]
	fn key_held_no_dt_returns_no_actions() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Pressed,
			prev_reported_state: KeyState::Pressed,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(0),
		};
		let result = input_key.update(KeyState::Pressed, Duration::from_ticks(0));

		assert_eq!(result, None);
	}

	#[test]
	fn key_press_and_release_is_debounced() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Released,
			prev_reported_state: KeyState::Released,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(5),
		};
		_ = input_key.update(KeyState::Pressed, Duration::from_ticks(0));
		let result = input_key.update(KeyState::Released, Duration::from_ticks(1));

		assert_eq!(result, None);
	}

	#[test]
	fn key_press_and_released_after_debounce_time_is_released() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Released,
			prev_reported_state: KeyState::Released,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(5),
		};
		_ = input_key.update(KeyState::Pressed, Duration::from_ticks(0));
		let result = input_key.update(
			KeyState::Released,
			input_key.debounce_time + Duration::from_ticks(1),
		);

		assert_eq!(result, Some(KeyState::Released));
	}

	#[test]
	fn key_press_and_released_after_debounce_time_and_multiple_updates_is_released() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Released,
			prev_reported_state: KeyState::Released,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(5),
		};
		_ = input_key.update(KeyState::Pressed, Duration::from_ticks(0));
		_ = input_key.update(KeyState::Pressed, Duration::from_ticks(1));
		_ = input_key.update(KeyState::Released, Duration::from_ticks(1));
		let result = input_key.update(KeyState::Released, input_key.debounce_time);

		assert_eq!(result, Some(KeyState::Released));
	}

	#[test]
	fn key_press_and_release_and_press_during_debounce_doesnt_reset_debounce_time() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let mut input_key = InputKey {
			id: key_id,
			prev_actual_state: KeyState::Released,
			prev_reported_state: KeyState::Released,
			keydown_time: Duration::from_ticks(0),
			debounce_time: Duration::from_ticks(5),
		};
		_ = input_key.update(KeyState::Pressed, Duration::from_ticks(0));
		_ = input_key.update(KeyState::Released, Duration::from_ticks(3));
		_ = input_key.update(KeyState::Pressed, Duration::from_ticks(1));
		let result = input_key.update(
			KeyState::Released,
			input_key.debounce_time - Duration::from_ticks(1),
		);

		assert_eq!(result, Some(KeyState::Released));
	}

	pub struct MockKeyMatrixState<const ROWS: usize, const COLS: usize> {
		// the physical state of keys: true = pressed, false = released
		key_states: [[bool; COLS]; ROWS],
		// current state of row pins: true = high, false = low
		row_states: [bool; ROWS],
	}

	impl<const ROWS: usize, const COLS: usize> MockKeyMatrixState<ROWS, COLS> {
		pub fn new() -> Self {
			Self {
				key_states: [[false; COLS]; ROWS],
				row_states: [false; ROWS],
			}
		}

		pub fn set_key_states(&mut self, states: [[bool; COLS]; ROWS]) {
			self.key_states = states;
		}

		pub fn set_key(&mut self, row: usize, col: usize, pressed: bool) {
			if row < ROWS && col < COLS {
				self.key_states[row][col] = pressed;
			}
		}

		pub fn get_key(&self, row: usize, col: usize) -> bool {
			if row < ROWS && col < COLS {
				self.key_states[row][col]
			} else {
				false
			}
		}

		fn set_row_state(&mut self, row: usize, high: bool) {
			if row < ROWS {
				self.row_states[row] = high;
			}
		}

		fn get_row_state(&self, row: usize) -> bool {
			if row < ROWS {
				self.row_states[row]
			} else {
				false
			}
		}
	}

	pub struct MockRowPin<const ROWS: usize, const COLS: usize> {
		row_index: usize,
		state: Rc<RefCell<MockKeyMatrixState<ROWS, COLS>>>,
	}

	impl<const ROWS: usize, const COLS: usize> MockRowPin<ROWS, COLS> {
		pub fn new(row_index: usize, state: Rc<RefCell<MockKeyMatrixState<ROWS, COLS>>>) -> Self {
			Self { row_index, state }
		}
	}

	impl<const ROWS: usize, const COLS: usize> RowPin for MockRowPin<ROWS, COLS> {
		fn set_high(&mut self) {
			self.state.borrow_mut().set_row_state(self.row_index, true);
		}

		fn set_low(&mut self) {
			self.state.borrow_mut().set_row_state(self.row_index, false);
		}
	}

	pub struct MockColPin<const ROWS: usize, const COLS: usize> {
		col_index: usize,
		state: Rc<RefCell<MockKeyMatrixState<ROWS, COLS>>>,
	}

	impl<const ROWS: usize, const COLS: usize> MockColPin<ROWS, COLS> {
		pub fn new(col_index: usize, state: Rc<RefCell<MockKeyMatrixState<ROWS, COLS>>>) -> Self {
			Self { col_index, state }
		}
	}

	impl<const ROWS: usize, const COLS: usize> ColPin for MockColPin<ROWS, COLS> {
		fn is_high(&self) -> bool {
			let state = self.state.borrow();

			// Check if any row that is currently high has a pressed key in this column
			for row in 0..ROWS {
				if state.get_row_state(row) && state.get_key(row, self.col_index) {
					return true;
				}
			}
			false
		}
	}

	pub fn create_mock_matrix<const ROWS: usize, const COLS: usize>() -> (
		Rc<RefCell<MockKeyMatrixState<ROWS, COLS>>>,
		[Box<dyn RowPin>; ROWS],
		[Box<dyn ColPin>; COLS],
	) {
		let state = Rc::new(RefCell::new(MockKeyMatrixState::new()));

		// Create row pins
		let rows: [Box<dyn RowPin>; ROWS] = std::array::from_fn(|i| {
			Box::new(MockRowPin::new(i, Rc::clone(&state))) as Box<dyn RowPin>
		});

		// Create column pins
		let cols: [Box<dyn ColPin>; COLS] = std::array::from_fn(|i| {
			Box::new(MockColPin::new(i, Rc::clone(&state))) as Box<dyn ColPin>
		});

		(state, rows, cols)
	}

	struct OldMockRowPin {}

	impl RowPin for OldMockRowPin {
		fn set_high(&mut self) {}

		fn set_low(&mut self) {}
	}

	struct OldMockColPin {
		state: Rc<RefCell<bool>>,
	}

	impl ColPin for OldMockColPin {
		fn is_high(&self) -> bool {
			*self.state.borrow()
		}
	}

	#[test]
	fn empty_matrix_returns_no_actions() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let row_pin = Box::new(OldMockRowPin {});
		let state = Rc::new(RefCell::new(false));
		let col_pin = Box::new(OldMockColPin {
			state: state.clone(),
		});
		let debounce_time = Duration::from_ticks(0);

		let mut matrix = KeyMatrix::new([key_id], [row_pin], [col_pin], debounce_time);

		let dt = Duration::from_ticks(1);
		let output = &mut Vec::new();

		matrix.update(dt, output);

		assert_eq!(output.len(), 0);
	}

	#[test]
	fn pressed_key_returns_pressed_action() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let row_pin = Box::new(OldMockRowPin {});
		let state = Rc::new(RefCell::new(true));
		let col_pin = Box::new(OldMockColPin {
			state: state.clone(),
		});
		let debounce_time = Duration::from_ticks(0);

		let mut matrix = KeyMatrix::new([key_id], [row_pin], [col_pin], debounce_time);

		let dt = Duration::from_ticks(1);
		let output = &mut Vec::new();
		matrix.update(dt, output);

		assert_eq!(output.len(), 1);
		assert_eq!(output[0].action, KeyState::Pressed);
	}

	#[test]
	fn subsequent_updates_dont_return_pressed_actions() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let row_pin = Box::new(OldMockRowPin {});
		let state = Rc::new(RefCell::new(true));
		let col_pin = Box::new(OldMockColPin {
			state: state.clone(),
		});
		let debounce_time = Duration::from_ticks(0);

		let mut matrix = KeyMatrix::new([key_id], [row_pin], [col_pin], debounce_time);

		let dt = Duration::from_ticks(1);
		let output = &mut Vec::new();

		matrix.update(dt, output);
		output.clear();
		matrix.update(dt, output);

		assert_eq!(
			output.len(),
			0,
			"Subsequent updates should not return any actions, but found actions: {:?}",
			output[0]
		);
	}

	#[test]
	fn subsequent_updates_dont_return_released_actions() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let row_pin = Box::new(OldMockRowPin {});
		let state = Rc::new(RefCell::new(false));
		let col_pin = Box::new(OldMockColPin {
			state: state.clone(),
		});
		let debounce_time = Duration::from_ticks(0);

		let mut matrix = KeyMatrix::new([key_id], [row_pin], [col_pin], debounce_time);

		let dt = Duration::from_ticks(1);
		let output = &mut Vec::new();

		matrix.update(dt, output);
		output.clear();
		matrix.update(dt, output);

		assert_eq!(output.len(), 0);
	}

	#[test]
	fn released_key_returns_released_action() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let row_pin = Box::new(OldMockRowPin {});
		let state = Rc::new(RefCell::new(true));
		let col_pin = Box::new(OldMockColPin {
			state: state.clone(),
		});
		let debounce_time = Duration::from_ticks(0);

		let mut matrix = KeyMatrix::new([key_id], [row_pin], [col_pin], debounce_time);

		let dt = Duration::from_ticks(1);
		let output = &mut Vec::new();

		matrix.update(dt, output);
		output.clear();
		*state.borrow_mut() = false;
		matrix.update(dt, output);

		assert_eq!(output.len(), 1);
		assert_eq!(output[0].action, KeyState::Released);
	}

	#[test]
	fn debounce_released_key() {
		let key_id = KeyId::new(Uuid::from_u128(0));
		let row_pin = Box::new(OldMockRowPin {});
		let state = Rc::new(RefCell::new(false));
		let col_pin = Box::new(OldMockColPin {
			state: state.clone(),
		});
		let debounce_time = Duration::from_ticks(5);

		let mut matrix = KeyMatrix::new([key_id], [row_pin], [col_pin], debounce_time);

		let dt = Duration::from_ticks(1);
		let output = &mut Vec::new();

		matrix.update(dt, output);
		output.clear();
		*state.borrow_mut() = false;
		matrix.update(dt, output);

		assert_eq!(output.len(), 0);
	}

	#[test]
	fn resolve_index_from_row_col_correctly_when_wrapping() {
		let index = KeyMatrix::<5, 6>::get_key_index(1, 0);
		assert_eq!(index, 6);
	}

	#[test]
	fn key_index_6_and_13_dont_register_key_index_12() {
		let button_states: [[bool; 6]; 5] = [
			[false, false, false, false, false, false],
			[true, false, false, false, false, false],
			[false, true, false, false, false, false],
			[false, false, false, false, false, false],
			[false, false, false, false, false, false],
		];

		let (state, rows, cols) = create_mock_matrix::<5, 6>();
		state.borrow_mut().set_key_states(button_states);

		let key_ids: [KeyId; 30] = [
			KeyId::new(Uuid::from_u128(0)), // 0
			KeyId::new(Uuid::from_u128(0)), // 1
			KeyId::new(Uuid::from_u128(0)), // 2
			KeyId::new(Uuid::from_u128(0)), // 3
			KeyId::new(Uuid::from_u128(0)), // 4
			KeyId::new(Uuid::from_u128(0)), // 5
			KeyId::new(Uuid::from_u128(1)), // 6
			KeyId::new(Uuid::from_u128(0)), // 7
			KeyId::new(Uuid::from_u128(0)), // 8
			KeyId::new(Uuid::from_u128(0)), // 9
			KeyId::new(Uuid::from_u128(0)), // 10
			KeyId::new(Uuid::from_u128(0)), // 11
			KeyId::new(Uuid::from_u128(2)), // 12
			KeyId::new(Uuid::from_u128(3)), // 13
			KeyId::new(Uuid::from_u128(0)), // 14
			KeyId::new(Uuid::from_u128(0)), // 15
			KeyId::new(Uuid::from_u128(0)), // 16
			KeyId::new(Uuid::from_u128(0)), // 17
			KeyId::new(Uuid::from_u128(0)), // 18
			KeyId::new(Uuid::from_u128(0)), // 19
			KeyId::new(Uuid::from_u128(0)), // 20
			KeyId::new(Uuid::from_u128(0)), // 21
			KeyId::new(Uuid::from_u128(0)), // 22
			KeyId::new(Uuid::from_u128(0)), // 23
			KeyId::new(Uuid::from_u128(0)), // 24
			KeyId::new(Uuid::from_u128(0)), // 25
			KeyId::new(Uuid::from_u128(0)), // 26
			KeyId::new(Uuid::from_u128(0)), // 27
			KeyId::new(Uuid::from_u128(0)), // 28
			KeyId::new(Uuid::from_u128(0)), // 29
		];

		let mut matrix = KeyMatrix::<5, 6>::new(key_ids, rows, cols, Duration::from_ticks(0));

		let dt = Duration::from_ticks(1);
		let output = &mut Vec::new();
		matrix.update(dt, output);

		assert_eq!(output.len(), 2);

		assert!(output.iter().any(|action| {
			action.key_id == KeyId::new(Uuid::from_u128(1)) && action.action == KeyState::Pressed
		}));
		assert!(output.iter().any(|action| {
			action.key_id == KeyId::new(Uuid::from_u128(3)) && action.action == KeyState::Pressed
		}));
	}
}
