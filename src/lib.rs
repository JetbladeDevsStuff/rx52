//! Logitech X52 driver in Rust
//!
//! A driver to control various aspects of the Logitech, (formerly Saitek)
//! [X52](https://www.logitechg.com/en-us/products/space/x52-space-flight-simulator-controller).
//! This is a library intended to replace the driver code found in
//! [gx52](https://gitlab.com/leinardi/gx52) and
//! [libx52](https://github.com/nirenjan/libx52). A simple command-line
//! application to replace the full functionality of gx52 is also included in
//! the source code of this library.

#![doc(
	html_favicon_url = "https://techtricity.net/rx52/logo.svg",
	html_logo_url = "https://techtricity.net/rx52/logo.svg"
)]
#![warn(missing_docs)]

use rusb::{
	request_type, Context, Device, DeviceDescriptor, DeviceHandle, Direction,
	Recipient, RequestType, UsbContext,
};
use std::error::Error as ErrorTrait;
use std::fmt::{Debug, Display, Formatter};
use std::time::Duration;

/// The physical type of an X52 device
#[derive(PartialEq, Eq)]
pub enum X52DeviceType {
	/// The X52 Pro, with more features
	X52Pro,
	/// The standard X52
	X52,
}

/// The color options for each LED
pub enum X52ColoredLedStatus {
	/// Turns the LED off
	Off,
	/// Sets the LED green
	Green,
	/// Sets the LED red
	Red,
	/// Sets the LED amber
	Amber,
}

/// The options for an LED which can only be on or off
pub enum X52OnOffLedStatus {
	/// Turns the LED off
	Off,
	/// Turns the LED on
	On,
}

/// The colored LEDs on the X52
pub enum X52ColoredLed {
	/// The A button on the stick
	A,
	/// The B button on the stick
	B,
	/// The D button on the throttle
	D,
	/// The E button on the throttle
	E,
	/// The LED between the T1 and T2 switches
	T1,
	/// The LED between the T3 and T4 switches
	T3,
	/// The LED between the T5 and T6 switches
	T5,
	/// The LED in the middle of the POV hat
	PovHat,
	/// The clutch button on the throttle (i button)
	Clutch,
}

/// The on/off LEDs on the X52
pub enum X52OnOffLed {
	/// The fire button on the stick
	Fire,
	/// The LED inside the throttle
	Throttle,
}

/// The line of the MFD
pub enum X52MFDLine {
	/// The first line
	Line1,
	/// The second line
	Line2,
	/// The third line
	Line3,
}

/// The date format for the MFD
pub enum X52DateFormat {
	/// Day, month, year
	DDMMYY,
	/// Month, day, year
	MMDDYY,
	/// Year, month, day
	YYMMDD,
}

/// The clock format for the MFD
pub enum X52ClockFormat {
	/// Twelve hour clock
	Hr12,
	/// 24 hour clock
	Hr24,
}

/// The clocks on the X52
pub enum X52Clocks {
	/// Clock 1 is the real clock
	Clock1,
	/// Clock 2 is an offset from clock 1
	Clock2,
	/// Clock 3 is an offset from clock 1
	Clock3,
}

impl Display for X52DeviceType {
	fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
		write!(
			fmt,
			"{}",
			if self == &Self::X52 { "X52" } else { "X52 Pro" }
		)
	}
}

/// USB descriptor for a certain model of X52
///
/// Contains information that can be used to map an X52's type to it's USB
/// vendor and descriptor IDs
///
/// # Examples
///
/// ```no_run
/// let device = rx52::get_devices()[0];
/// println!("Detected a {}", device.x52_type());
/// ```
///
/// <div class="warning">For performance, this struct has all possible
/// permutations created at compile time.</div>
pub struct X52Descriptor {
	x52_type: &'static X52DeviceType,
	vendor: &'static u16,
	product: &'static u16,
	description: &'static str,
}

impl X52Descriptor {
	/// The type of X52 this descriptor refers to
	pub fn x52_type(&self) -> &'static X52DeviceType {
		self.x52_type
	}

	/// The vendor ID for this X52, is the same for all X52s
	pub fn vendor(&self) -> &'static u16 {
		self.vendor
	}

	/// The product ID for this X52
	pub fn product(&self) -> &'static u16 {
		self.product
	}

	/// The USB description for this X52
	pub fn description(&self) -> &'static str {
		self.description
	}

	fn eq_descriptor(&self, other: &DeviceDescriptor) -> bool {
		self.vendor == &other.vendor_id() && self.product == &other.product_id()
	}

	// fn eq_descriptor_optional(&self, other: &Option<DeviceDescriptor>) -> bool {
	// 	match other {
	// 		None => false,
	// 		Some(x) => {
	// 			self.vendor == &x.vendor_id() && self.product == &x.product_id()
	// 		}
	// 	}
	// }
}

impl PartialEq for X52Descriptor {
	fn eq(&self, other: &X52Descriptor) -> bool {
		self.x52_type == other.x52_type
	}
}

impl Eq for X52Descriptor {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Machine readable error IDs for [`Error`]s generated by rx52
pub enum ErrorId {
	/// No X52s were detected
	NoX52sFound,
	/// Tried to use a Pro feature on a regular X52
	NotAPro,
	/// The device at the given bus and address is not an X52
	DeviceNotX52,
	/// The given bus and device number did not match any detected devices
	BusDeviceNotFound,
	/// Tried to write a string to the MFD that was longer than 16 characters
	MFDLineTooLong,
	/// Tried to write a string with non-ASCII characters to the MFD
	MFDNotASCII,
	/// The given offset for clocks 2 or 3 was greater than 24 hours
	ClockOffsetTooBig,
}

/// Some possible sources for ['Error']
#[derive(Debug, Clone, Copy)]
enum ErrSources {
	Rusb(rusb::Error),
}

#[derive(Debug)]
/// Error used in rx52
///
/// See [`ErrorId`] for the errors that can be generated by this module.
pub struct Error {
	maybe_id: Option<ErrorId>,
	source: Option<ErrSources>,
	msg: String,
}

impl Error {
	/// Creates a new [`Error`] from an [`ErrorId`] and a [`String`]
	fn new(id: ErrorId, string: String) -> Self {
		Self {
			maybe_id: Some(id),
			source: None,
			msg: string,
		}
	}

	// /// Creates a new [`Error`] from an [`ErrorId`] and a [`&str`]
	// fn new_str(id: ErrorId, string: &str) -> Self {
	// 	Self {
	// 		maybe_id: Some(id),
	// 		source: None,
	// 		msg: string.to_string(),
	// 	}
	// }

	/// Gives the [`ErrorId`] of the [`Error`]
	///
	/// If this type is [`None`], then the [`Error`] originated from another
	/// crate, probably `rusb`. You can try [`Error::source`] to get the
	/// underlying error.
	pub fn id(&self) -> Option<ErrorId> {
		self.maybe_id
	}

	/// Gets a [`rusb::Error`] from this [`Error`]
	///
	/// If this [`Error`] was generated by an exception in `rusb`, then this
	/// method will return [`Some`] with the [`rusb::Error`]
	pub fn rusb_error(&self) -> Option<rusb::Error> {
		self.source.map(|x| match x {
			ErrSources::Rusb(x) => x,
		})
	}
}

impl From<rusb::Error> for Error {
	fn from(err: rusb::Error) -> Self {
		Self {
			maybe_id: None,
			source: Some(ErrSources::Rusb(err)),
			msg: err.to_string(),
		}
	}
}

impl From<&str> for Error {
	fn from(string: &str) -> Self {
		Self {
			maybe_id: None,
			source: None,
			msg: string.to_string(),
		}
	}
}

impl Display for Error {
	fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
		write!(fmt, "{}", self.msg)
	}
}

impl ErrorTrait for Error {
	fn source(&self) -> Option<&(dyn ErrorTrait + 'static)> {
		// I feel like this could be prettier
		match self.source {
			Some(ref x) => match x {
				ErrSources::Rusb(ref y) => Some(y),
			},
			None => None,
		}
	}
}

const SAITEK_ID: u16 = 0x06A3;

const POSSIBLE_DESCRIPTORS: [X52Descriptor; 3] = [
	X52Descriptor {
		x52_type: &X52DeviceType::X52,
		vendor: &SAITEK_ID,
		product: &0x0225,
		description: "X52 Flight Controller",
	},
	X52Descriptor {
		x52_type: &X52DeviceType::X52,
		vendor: &SAITEK_ID,
		product: &0x075C,
		description: "X52 Flight Controller",
	},
	X52Descriptor {
		x52_type: &X52DeviceType::X52Pro,
		vendor: &SAITEK_ID,
		product: &0x0762,
		description: "Saitek X52 Pro Flight Control System",
	},
];

/// The ID used to make vendor requests
const X52_VENDOR_REQUEST: u8 = 0x91;
/// The timeout (in milliseconds) for vendor requests
const REQUEST_TIMEOUT_MILLIS: Duration = Duration::from_millis(5000);
/// The command used to set an LED
const LED_SET_COMMAND: u16 = 0xB8;
/// The command to set the brightness of the LEDs
const LED_SET_BRIGHTNESS_COMMAND: u16 = 0xB2;
/// The command to set the brightness of the MFD
const MFD_SET_BRIGHTNESS_COMMAND: u16 = 0xB1;
/// The command used to clear a line of the MFD
const MFD_CLEAR_LINE_COMMAND: u16 = 0x08;
/// The amount of text that can fit on a line
const MFD_LINE_SIZE: usize = 16;
/// The command to set the shift indicator on the MFD
const SET_SHIFT_STATUS_COMMAND: u16 = 0xFD;
/// The command to set the blinking of the throttle and POV hat LEDs
const SET_BLINK_STATUS_COMMAND: u16 = 0xB4;
/// The command to set clock 1
const CLOCK_1_SET_COMMAND: u16 = 0xC0;
/// The command to set clock 2's offset from clock 1
const CLOCK_2_OFFSET_COMMAND: u16 = 0xC1;
/// The command to set clock 3's offset from clock 1
const CLOCK_3_OFFSET_COMMAND: u16 = 0xC2;
/// Sets the day and month on the MFD
const SET_DAY_MONTH_COMMAND: u16 = 0xC4;
/// Sets the year on the MFD
const SET_YEAR_COMMAND: u16 = 0xC8;

/// Returns true if the given descriptor refers to an X52
fn is_descriptor_x52(descriptor: &DeviceDescriptor) -> bool {
	POSSIBLE_DESCRIPTORS
		.iter()
		.any(|x| x.eq_descriptor(descriptor))
}

/// Returns Ok(()) if the given descriptor is an x52, or a generic error
// fn is_descriptor_x52_or_error(
// 	descriptor: &DeviceDescriptor,
// ) -> Result<(), Error> {
// 	if is_descriptor_x52(descriptor) {
// 		Ok(())
// 	} else {
// 		Err(Error::new_str(
// 			ErrorId::DeviceNotX52,
// 			"The given device is not an X52",
// 		))
// 	}
// }

/// Finds a [rusb::Device] from a bus and device number
fn find_device_from_bus_device(
	ctx: &Context,
	bus: u8,
	device: u8,
) -> Result<Device<Context>, Error> {
	ctx.devices()?
		.iter()
		.find(|x| x.bus_number() == bus && x.address() == device)
		.ok_or(Error::new(
			ErrorId::BusDeviceNotFound,
			format!("No device found at Bus {:03} Device {:03}", bus, device),
		))
}

/// Returns an x52 type given a descriptor
fn get_x52_type_from_descriptor(
	descriptor: &DeviceDescriptor,
) -> Result<&'static X52DeviceType, Error> {
	POSSIBLE_DESCRIPTORS
		.iter()
		.find(|x| x.eq_descriptor(descriptor))
		.map(|x| x.x52_type)
		.ok_or(Error::new(
			ErrorId::DeviceNotX52,
			format!(
				"The given descriptor ID {:04x}:{:04x} is not an X52",
				descriptor.vendor_id(),
				descriptor.product_id()
			),
		))
}

// Ensure that an X52 is a pro or return with an error
fn ensure_x52_is_pro(driver: &X52Driver) -> Result<(), Error> {
	if *driver.x52_type()? == X52DeviceType::X52 {
		Err(Error::new(
			ErrorId::NotAPro,
			format!(
				"The device at Bus {:03} Device {:03} is not an X52 Pro",
				driver.get_bus_device().0,
				driver.get_bus_device().1
			),
		))
	} else {
		Ok(())
	}
}

/// Does a vendor command on the given device handle
fn do_vendor_command(
	device: &DeviceHandle<Context>,
	index: u16,
	value: u16,
) -> Result<(), Error> {
	device.write_control(
		request_type(Direction::Out, RequestType::Vendor, Recipient::Device),
		X52_VENDOR_REQUEST,
		value,
		index,
		&[0_u8; 0], // Empty data
		REQUEST_TIMEOUT_MILLIS,
	)?;
	Ok(())
}

/// Maps a given [X52OnOffLed] to a value to be sent to the X52
fn map_on_off_led_to_value(led: &X52OnOffLed) -> u8 {
	match led {
		X52OnOffLed::Fire => 1,
		X52OnOffLed::Throttle => 20,
	}
}

/// Maps a given [X52ColoredLed] to the red and green values to be sent to the X52
fn map_colored_led_to_value(led: &X52ColoredLed) -> (u8, u8) {
	match led {
		X52ColoredLed::A => (2, 3),
		X52ColoredLed::B => (4, 5),
		X52ColoredLed::D => (6, 7),
		X52ColoredLed::E => (8, 9),
		X52ColoredLed::T1 => (10, 11),
		X52ColoredLed::T3 => (12, 13),
		X52ColoredLed::T5 => (14, 15),
		X52ColoredLed::PovHat => (16, 17),
		X52ColoredLed::Clutch => (18, 19),
	}
}

/// Maps a given [X52OnOffLedStatus] to the values needed to send to the X52
fn map_on_off_led_status_to_value(status: &X52OnOffLedStatus) -> u8 {
	match status {
		X52OnOffLedStatus::Off => 0,
		X52OnOffLedStatus::On => 1,
	}
}

/// Maps a given [X52ColoredLedStatus] to the values needed for the red and green LEDs
fn map_colored_led_status_to_value(status: &X52ColoredLedStatus) -> (u8, u8) {
	match status {
		// Both off
		X52ColoredLedStatus::Off => (0, 0),
		// Red on
		X52ColoredLedStatus::Red => (1, 0),
		// Green on
		X52ColoredLedStatus::Green => (0, 1),
		// Both on
		X52ColoredLedStatus::Amber => (1, 1),
	}
}

/// Maps a given [X52MFDLine] to the values needed to send to the X52
fn map_mfd_line_to_value(line: &X52MFDLine) -> u8 {
	match line {
		X52MFDLine::Line1 => 0xD1,
		X52MFDLine::Line2 => 0xD2,
		X52MFDLine::Line3 => 0xD4,
	}
}

/// Maps booleans to values needed to send to the X52
fn map_bool_to_value(enabled: bool) -> u16 {
	match enabled {
		true => 0x51,
		false => 0x50,
	}
}

/// Writes a line to the MFD
/// This does not check if the string is ASCII!!! Text must be 16 bytes!!!
fn write_mfd_line(
	handle: &DeviceHandle<Context>,
	line: &X52MFDLine,
	text: &str,
) -> Result<(), Error> {
	if text.len() == 0 {
		Ok(())
	} else {
		do_vendor_command(
			handle,
			map_mfd_line_to_value(line) as u16,
			(text.as_bytes()[1] as u16) << 8 | text.as_bytes()[0] as u16,
		)?;
		write_mfd_line(handle, line, &text[2..])
	}
}

/// A driver used to control a X52 device
pub struct X52Driver {
	device: Device<Context>,
	// The context needs to be kept alive here
	_usb_context: Context,
}

impl X52Driver {
	/// Toggles an LED which can be either on or off on the X52
	pub fn toggle_led_on_off(
		&self,
		led: &X52OnOffLed,
		status: &X52OnOffLedStatus,
	) -> Result<(), Error> {
		ensure_x52_is_pro(self)?;
		let handle = self.device.open()?;
		do_vendor_command(
			&handle,
			LED_SET_COMMAND,
			((map_on_off_led_to_value(led) as u16) << 8)
				+ map_on_off_led_status_to_value(status) as u16,
		)
	}

	/// Sets the color of a multicolored LED
	pub fn toggle_led_colored(
		&self,
		led: &X52ColoredLed,
		status: &X52ColoredLedStatus,
	) -> Result<(), Error> {
		ensure_x52_is_pro(self)?;
		let handle = self.device.open()?;
		do_vendor_command(
			&handle,
			LED_SET_COMMAND,
			((map_colored_led_to_value(led).0 as u16) << 8)
				+ map_colored_led_status_to_value(status).0 as u16,
		)?;
		do_vendor_command(
			&handle,
			LED_SET_COMMAND,
			((map_colored_led_to_value(led).1 as u16) << 8)
				+ map_colored_led_status_to_value(status).1 as u16,
		)
	}

	/// Clears a line of text on the MFD
	pub fn clear_mfd_line(&self, line: &X52MFDLine) -> Result<(), Error> {
		do_vendor_command(
			&self.device.open()?,
			MFD_CLEAR_LINE_COMMAND | map_mfd_line_to_value(line) as u16,
			0,
		)
	}

	/// Sets a line of text on the MFD
	///
	/// If there are not enough characters in `text` to fill a line (16), text
	/// is padded to be center aligned.
	pub fn set_mfd_text(
		&self,
		line: &X52MFDLine,
		text: String,
	) -> Result<(), Error> {
		if !text.is_ascii() {
			return Err(Error::new(
				ErrorId::MFDNotASCII,
				format!("The text \"{text}\" contains non-ASCII characters"),
			));
		}
		if text.len() > MFD_LINE_SIZE {
			return Err(Error::new(
				ErrorId::MFDLineTooLong,
				format!("The text \"{text}\" is too long to fit on the MFD"),
			));
		}
		self.clear_mfd_line(line)?;
		write_mfd_line(&self.device.open()?, line, &format!("{:^16}", text))
	}

	/// Sets the brightness of the LEDs on the X52
	///
	/// `brightness` should be between 0 and 128. Anything higher can cause "unintended effects", says
	/// [libx52](https://nirenjan.github.io/libx52/group__libx52mfdled.html#ga9bbf5e1ff83201f6124b2d3c75c837c6).
	pub fn set_led_brightness(&self, brightness: u8) -> Result<(), Error> {
		do_vendor_command(
			&self.device.open()?,
			LED_SET_BRIGHTNESS_COMMAND,
			brightness as u16,
		)
	}

	/// Sets the brightness of the MFD on the X52
	///
	/// `brightness` should be between 0 and 128. Anything higher can cause "unintended effects", says
	/// [libx52](https://nirenjan.github.io/libx52/group__libx52mfdled.html#ga9bbf5e1ff83201f6124b2d3c75c837c6).
	pub fn set_mfd_brightness(&self, brightness: u8) -> Result<(), Error> {
		do_vendor_command(
			&self.device.open()?,
			MFD_SET_BRIGHTNESS_COMMAND,
			brightness as u16,
		)
	}

	/// Sets the "shift" status on the X52's MFD
	pub fn set_shift_status(&self, enabled: bool) -> Result<(), Error> {
		do_vendor_command(
			&self.device.open()?,
			SET_SHIFT_STATUS_COMMAND,
			map_bool_to_value(enabled),
		)
	}

	/// Sets the blink status for the throttle and POV hat
	pub fn set_blink_status(&self, enabled: bool) -> Result<(), Error> {
		do_vendor_command(
			&self.device.open()?,
			SET_BLINK_STATUS_COMMAND,
			map_bool_to_value(enabled),
		)
	}

	/// Sets the primary clock of the X52
	pub fn set_clock_1(
		&self,
		hour: u8,
		minute: u8,
		use_24h: bool,
	) -> Result<(), Error> {
		do_vendor_command(
			&self.device.open()?,
			CLOCK_1_SET_COMMAND,
			(use_24h as u16) << 15
				| ((hour as u16) & 0x7F) << 8
				| minute as u16,
		)
	}

	/// Sets the clock 2 offset in minutes from clock 1
	pub fn set_clock_2_offset(
		&self,
		offset: i16,
		use_24h: bool,
	) -> Result<(), Error> {
		// Limit offset to 24 hours either direction
		if offset < -1440 || offset > 1440 {
			Err(Error::new(
				ErrorId::ClockOffsetTooBig,
				format!("Clock 2 offset ({offset}) too large"),
			))
		} else {
			do_vendor_command(
				&self.device.open()?,
				CLOCK_2_OFFSET_COMMAND,
				(use_24h as u16) << 15
					| if offset > 0 {
						1_u16 << 10 | -offset as u16
					} else {
						offset as u16
					},
			)
		}
	}

	/// Sets the clock 3 offset in minutes from clock 1
	pub fn set_clock_3_offset(
		&self,
		offset: i16,
		use_24h: bool,
	) -> Result<(), Error> {
		// Limit offset to 24 hours either direction
		if offset < -1440 || offset > 1440 {
			Err(Error::new(
				ErrorId::ClockOffsetTooBig,
				format!("Clock 3 offset ({offset}) too large"),
			))
		} else {
			do_vendor_command(
				&self.device.open()?,
				CLOCK_3_OFFSET_COMMAND,
				(use_24h as u16) << 15
					| if offset > 0 {
						1_u16 << 10 | -offset as u16
					} else {
						offset as u16
					},
			)
		}
	}

    /// Sets the given day, month, and year as they day on the X52
    /// Year must only be two digits
    pub fn set_date(&self, day: u8, month: u8, year: u8, format: X52DateFormat) -> Result<(), Error> {
        do_vendor_command(&self.device.open()?, SET_DAY_MONTH_COMMAND, match format {
            X52DateFormat::DDMMYY => (month as u16) << 8 | day as u16,
            X52DateFormat::MMDDYY => (day as u16) << 8 | month as u16,
            X52DateFormat::YYMMDD => (month as u16) << 8 | year as u16,
        })?;
        do_vendor_command(&self.device.open()?, SET_YEAR_COMMAND, match format {
            X52DateFormat::DDMMYY => year as u16,
            X52DateFormat::MMDDYY => year as u16,
            X52DateFormat::YYMMDD => day as u16,
        })
    }

	/// Gets the type of X52 this device refers to
	pub fn x52_type(&self) -> Result<&'static X52DeviceType, Error> {
		get_x52_type_from_descriptor(&self.device.device_descriptor()?)
	}

	/// Returns the (bus, device) of the device
	pub fn get_bus_device(&self) -> (u8, u8) {
		(self.device.bus_number(), self.device.address())
	}

	/// Creates an X52Driver given a known bus and device number
	pub fn new_from_bus_device(
		bus: u8,
		device: u8,
	) -> Result<X52Driver, Error> {
		let context = Context::new()?;
		let usb_device = find_device_from_bus_device(&context, bus, device)?;

		if is_descriptor_x52(&usb_device.device_descriptor()?) {
			Ok(Self {
				device: usb_device,
				_usb_context: context,
			})
		} else {
			Err(Error::new(
				ErrorId::DeviceNotX52,
				format!(
					"The device at Bus {:03} Device {:03} is not an X52",
					bus, device
				),
			))
		}
	}
}

/// Gets a vector of available X52 devices without creating a driver for any
pub fn get_possible_device_types() -> Result<Vec<&'static X52Descriptor>, Error> {
	Ok(Context::new()?
		.devices()?
		.iter()
		.filter_map(|x| x.device_descriptor().ok())
		.filter_map(|x| {
			POSSIBLE_DESCRIPTORS.iter().find(|y| y.eq_descriptor(&x))
		})
		.collect::<Vec<&'static X52Descriptor>>())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn possible_descriptors_ok() {
		for i in POSSIBLE_DESCRIPTORS {
			assert_eq!(i.vendor(), &SAITEK_ID);
		}
	}

	// #[test]
	// fn error_str() {
	// 	let id = ErrorId::DeviceNotFound;
	// 	let string = "Cool test";
	// 	let err = Error::new_str(id, string);
	// 	assert_eq!(err.id().unwrap(), id);
	// 	assert_eq!(err.to_string(), string)
	// }

	#[test]
	fn error_string() {
		let id = ErrorId::NoX52sFound;
		let string: String = "Cool test String".to_string();
		let err = Error::new(id, string.clone());
		assert_eq!(err.id().unwrap(), id);
		assert_eq!(err.to_string(), string)
	}

	#[test]
	fn error_rusb_test() {
		let err_rusb = rusb::Error::Busy;
		let err = Error::from(err_rusb);
		assert_eq!(err.rusb_error().unwrap(), err_rusb);
		assert_eq!(err.to_string(), err_rusb.to_string())
	}
}
