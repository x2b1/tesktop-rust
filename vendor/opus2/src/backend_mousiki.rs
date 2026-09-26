//! Compatibility backend implemented on top of `mousiki::c_style_api`.

use mousiki::c_style_api::{
	opus as opus_api, opus_decoder as decoder_api, opus_encoder as encoder_api,
	opus_multistream as multistream_api, packet as packet_api, repacketizer as repacketizer_api,
};

use std::convert::TryFrom;
use std::marker::PhantomData;

const OPUS_APPLICATION_VOIP: i32 = 2048;
const OPUS_APPLICATION_AUDIO: i32 = 2049;
const OPUS_APPLICATION_RESTRICTED_LOWDELAY: i32 = 2051;

const OPUS_AUTO: i32 = -1000;
const OPUS_BITRATE_MAX: i32 = -1;

const OPUS_BANDWIDTH_NARROWBAND: i32 = 1101;
const OPUS_BANDWIDTH_MEDIUMBAND: i32 = 1102;
const OPUS_BANDWIDTH_WIDEBAND: i32 = 1103;
const OPUS_BANDWIDTH_SUPERWIDEBAND: i32 = 1104;
const OPUS_BANDWIDTH_FULLBAND: i32 = 1105;

const OPUS_SIGNAL_VOICE: i32 = 3001;
const OPUS_SIGNAL_MUSIC: i32 = 3002;

const OPUS_FRAMESIZE_ARG: i32 = 5000;
const OPUS_FRAMESIZE_2_5_MS: i32 = 5001;
const OPUS_FRAMESIZE_5_MS: i32 = 5002;
const OPUS_FRAMESIZE_10_MS: i32 = 5003;
const OPUS_FRAMESIZE_20_MS: i32 = 5004;
const OPUS_FRAMESIZE_40_MS: i32 = 5005;
const OPUS_FRAMESIZE_60_MS: i32 = 5006;
const OPUS_FRAMESIZE_80_MS: i32 = 5007;
const OPUS_FRAMESIZE_100_MS: i32 = 5008;
const OPUS_FRAMESIZE_120_MS: i32 = 5009;

const OPUS_BAD_ARG: i32 = -1;
const OPUS_BUFFER_TOO_SMALL: i32 = -2;
const OPUS_INTERNAL_ERROR: i32 = -3;
const OPUS_INVALID_PACKET: i32 = -4;
const OPUS_UNIMPLEMENTED: i32 = -5;
const OPUS_INVALID_STATE: i32 = -6;
const OPUS_ALLOC_FAIL: i32 = -7;

/// The possible applications for the codec.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
#[repr(i32)]
pub enum Application {
	/// Best for most VoIP/videoconference applications where listening quality
	/// and intelligibility matter most.
	Voip = OPUS_APPLICATION_VOIP,
	/// Best for broadcast/high-fidelity application where the decoded audio
	/// should be as close as possible to the input.
	Audio = OPUS_APPLICATION_AUDIO,
	/// Only use when lowest-achievable latency is what matters most.
	LowDelay = OPUS_APPLICATION_RESTRICTED_LOWDELAY,
}

impl Application {
	fn from_raw(raw: i32, what: &'static str) -> Result<Application> {
		match raw {
			OPUS_APPLICATION_VOIP => Ok(Application::Voip),
			OPUS_APPLICATION_AUDIO => Ok(Application::Audio),
			OPUS_APPLICATION_RESTRICTED_LOWDELAY => Ok(Application::LowDelay),
			_ => Err(Error::bad_arg(what)),
		}
	}
}

/// The available channel settings.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Channels {
	/// One channel.
	Mono = 1,
	/// Two channels, left and right.
	Stereo = 2,
}

/// The available bandwidth level settings.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
#[repr(i32)]
pub enum Bandwidth {
	/// Auto/default setting.
	#[default]
	Auto = OPUS_AUTO,
	/// 4kHz bandpass.
	Narrowband = OPUS_BANDWIDTH_NARROWBAND,
	/// 6kHz bandpass.
	Mediumband = OPUS_BANDWIDTH_MEDIUMBAND,
	/// 8kHz bandpass.
	Wideband = OPUS_BANDWIDTH_WIDEBAND,
	/// 12kHz bandpass.
	Superwideband = OPUS_BANDWIDTH_SUPERWIDEBAND,
	/// 20kHz bandpass.
	Fullband = OPUS_BANDWIDTH_FULLBAND,
}

impl Bandwidth {
	fn from_int(value: i32) -> Option<Bandwidth> {
		match value {
			OPUS_AUTO => Some(Bandwidth::Auto),
			OPUS_BANDWIDTH_NARROWBAND => Some(Bandwidth::Narrowband),
			OPUS_BANDWIDTH_MEDIUMBAND => Some(Bandwidth::Mediumband),
			OPUS_BANDWIDTH_WIDEBAND => Some(Bandwidth::Wideband),
			OPUS_BANDWIDTH_SUPERWIDEBAND => Some(Bandwidth::Superwideband),
			OPUS_BANDWIDTH_FULLBAND => Some(Bandwidth::Fullband),
			_ => None,
		}
	}

	fn from_packet(value: packet_api::Bandwidth) -> Bandwidth {
		match value {
			packet_api::Bandwidth::Narrow => Bandwidth::Narrowband,
			packet_api::Bandwidth::Medium => Bandwidth::Mediumband,
			packet_api::Bandwidth::Wide => Bandwidth::Wideband,
			packet_api::Bandwidth::SuperWide => Bandwidth::Superwideband,
			packet_api::Bandwidth::Full => Bandwidth::Fullband,
		}
	}

	fn decode(value: i32, what: &'static str) -> Result<Bandwidth> {
		Self::from_int(value).ok_or_else(|| Error::bad_arg(what))
	}

	fn raw(self) -> i32 {
		self as i32
	}
}

/// Possible error codes.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(i32)]
pub enum ErrorCode {
	/// One or more invalid/out of range arguments.
	BadArg = OPUS_BAD_ARG,
	/// Not enough bytes allocated in the buffer.
	BufferTooSmall = OPUS_BUFFER_TOO_SMALL,
	/// An internal error was detected.
	InternalError = OPUS_INTERNAL_ERROR,
	/// The compressed data passed is corrupted.
	InvalidPacket = OPUS_INVALID_PACKET,
	/// Invalid/unsupported request number.
	Unimplemented = OPUS_UNIMPLEMENTED,
	/// An encoder or decoder structure is invalid or already freed.
	InvalidState = OPUS_INVALID_STATE,
	/// Memory allocation has failed.
	AllocFail = OPUS_ALLOC_FAIL,
	/// An unknown failure.
	Unknown = -8,
}

impl ErrorCode {
	fn from_int(value: i32) -> ErrorCode {
		match value {
			OPUS_BAD_ARG => ErrorCode::BadArg,
			OPUS_BUFFER_TOO_SMALL => ErrorCode::BufferTooSmall,
			OPUS_INTERNAL_ERROR => ErrorCode::InternalError,
			OPUS_INVALID_PACKET => ErrorCode::InvalidPacket,
			OPUS_UNIMPLEMENTED => ErrorCode::Unimplemented,
			OPUS_INVALID_STATE => ErrorCode::InvalidState,
			OPUS_ALLOC_FAIL => ErrorCode::AllocFail,
			_ => ErrorCode::Unknown,
		}
	}

	/// Get a human-readable error string for this error code.
	pub fn description(self) -> &'static str {
		match self {
			ErrorCode::BadArg => "invalid argument",
			ErrorCode::BufferTooSmall => "buffer too small",
			ErrorCode::InternalError => "internal error",
			ErrorCode::InvalidPacket => "corrupted stream",
			ErrorCode::Unimplemented => "request not implemented",
			ErrorCode::InvalidState => "invalid state",
			ErrorCode::AllocFail => "memory allocation failed",
			ErrorCode::Unknown => "unknown error",
		}
	}
}

/// Possible bitrates.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Bitrate {
	/// Explicit bitrate choice (in bits/second).
	Bits(i32),
	/// Maximum bitrate allowed (up to maximum number of bytes for the packet).
	Max,
	/// Default bitrate decided by the encoder (not recommended).
	Auto,
}

impl Bitrate {
	fn from_raw(raw: i32) -> Bitrate {
		match raw {
			OPUS_AUTO => Bitrate::Auto,
			OPUS_BITRATE_MAX => Bitrate::Max,
			_ => Bitrate::Bits(raw),
		}
	}

	fn raw(self) -> i32 {
		match self {
			Bitrate::Auto => OPUS_AUTO,
			Bitrate::Max => OPUS_BITRATE_MAX,
			Bitrate::Bits(raw) => raw,
		}
	}
}

/// Possible signal types. Hints for the encoder's mode selection.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Default)]
#[repr(i32)]
pub enum Signal {
	/// Auto/default setting.
	#[default]
	Auto = OPUS_AUTO,
	/// Bias thresholds towards choosing LPC or Hybrid modes.
	Voice = OPUS_SIGNAL_VOICE,
	/// Bias thresholds towards choosing MDCT modes.
	Music = OPUS_SIGNAL_MUSIC,
}

impl Signal {
	fn from_raw(raw: i32, what: &'static str) -> Result<Signal> {
		match raw {
			OPUS_AUTO => Ok(Signal::Auto),
			OPUS_SIGNAL_VOICE => Ok(Signal::Voice),
			OPUS_SIGNAL_MUSIC => Ok(Signal::Music),
			_ => Err(Error::bad_arg(what)),
		}
	}

	fn raw(self) -> i32 {
		self as i32
	}
}

/// Possible frame sizes. Controls encoder's use of variable duration frames.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Default)]
#[repr(i32)]
pub enum FrameSize {
	/// Select frame size from the argument (default).
	#[default]
	Arg = OPUS_FRAMESIZE_ARG,
	/// Use 2.5 ms frames.
	Ms2_5 = OPUS_FRAMESIZE_2_5_MS,
	/// Use 5 ms frames.
	Ms5 = OPUS_FRAMESIZE_5_MS,
	/// Use 10 ms frames.
	Ms10 = OPUS_FRAMESIZE_10_MS,
	/// Use 20 ms frames.
	Ms20 = OPUS_FRAMESIZE_20_MS,
	/// Use 40 ms frames.
	Ms40 = OPUS_FRAMESIZE_40_MS,
	/// Use 60 ms frames.
	Ms60 = OPUS_FRAMESIZE_60_MS,
	/// Use 80 ms frames.
	Ms80 = OPUS_FRAMESIZE_80_MS,
	/// Use 100 ms frames.
	Ms100 = OPUS_FRAMESIZE_100_MS,
	/// Use 120 ms frames.
	Ms120 = OPUS_FRAMESIZE_120_MS,
}

impl FrameSize {
	fn from_raw(raw: i32, what: &'static str) -> Result<FrameSize> {
		match raw {
			OPUS_FRAMESIZE_ARG => Ok(FrameSize::Arg),
			OPUS_FRAMESIZE_2_5_MS => Ok(FrameSize::Ms2_5),
			OPUS_FRAMESIZE_5_MS => Ok(FrameSize::Ms5),
			OPUS_FRAMESIZE_10_MS => Ok(FrameSize::Ms10),
			OPUS_FRAMESIZE_20_MS => Ok(FrameSize::Ms20),
			OPUS_FRAMESIZE_40_MS => Ok(FrameSize::Ms40),
			OPUS_FRAMESIZE_60_MS => Ok(FrameSize::Ms60),
			OPUS_FRAMESIZE_80_MS => Ok(FrameSize::Ms80),
			OPUS_FRAMESIZE_100_MS => Ok(FrameSize::Ms100),
			OPUS_FRAMESIZE_120_MS => Ok(FrameSize::Ms120),
			_ => Err(Error::bad_arg(what)),
		}
	}

	fn raw(self) -> i32 {
		self as i32
	}
}

/// Get the backend version string.
pub fn version() -> &'static str {
	mousiki::opus_get_version_string()
}

/// Opus error Result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// An error generated by the Opus backend.
#[derive(Debug)]
pub struct Error {
	function: &'static str,
	code: ErrorCode,
}

impl Error {
	fn bad_arg(what: &'static str) -> Error {
		Error { function: what, code: ErrorCode::BadArg }
	}

	fn from_code(what: &'static str, code: i32) -> Error {
		Error {
			function: what,
			code: ErrorCode::from_int(code),
		}
	}

	/// Get the name of the Opus function from which the error originated.
	pub fn function(&self) -> &'static str {
		self.function
	}

	/// Get a textual description of the error provided by Opus.
	pub fn description(&self) -> &'static str {
		self.code.description()
	}

	/// Get the Opus error code of the error.
	pub fn code(&self) -> ErrorCode {
		self.code
	}
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}: {}", self.function, self.description())
	}
}

impl std::error::Error for Error {}

fn map_encoder_init(what: &'static str, err: encoder_api::OpusEncoderInitError) -> Error {
	Error::from_code(what, err.code())
}

fn map_encoder_ctl(what: &'static str, err: encoder_api::OpusEncoderCtlError) -> Error {
	Error::from_code(what, err.code())
}

fn map_encode(what: &'static str, err: encoder_api::OpusEncodeError) -> Error {
	Error::from_code(what, err.code())
}

fn decoder_init_code(err: decoder_api::OpusDecoderInitError) -> i32 {
	match err {
		decoder_api::OpusDecoderInitError::BadArgument => OPUS_BAD_ARG,
		decoder_api::OpusDecoderInitError::CeltInit
		| decoder_api::OpusDecoderInitError::SilkInit => OPUS_INTERNAL_ERROR,
	}
}

fn decoder_ctl_code(err: decoder_api::OpusDecoderCtlError) -> i32 {
	match err {
		decoder_api::OpusDecoderCtlError::BadArgument => OPUS_BAD_ARG,
		decoder_api::OpusDecoderCtlError::Unimplemented => OPUS_UNIMPLEMENTED,
		decoder_api::OpusDecoderCtlError::Silk(_) => OPUS_INTERNAL_ERROR,
	}
}

fn map_decoder_init(what: &'static str, err: decoder_api::OpusDecoderInitError) -> Error {
	Error::from_code(what, decoder_init_code(err))
}

fn map_decoder_ctl(what: &'static str, err: decoder_api::OpusDecoderCtlError) -> Error {
	Error::from_code(what, decoder_ctl_code(err))
}

fn map_decode(what: &'static str, err: decoder_api::OpusDecodeError) -> Error {
	Error::from_code(what, err.code())
}

fn map_packet(what: &'static str, err: packet_api::PacketError) -> Error {
	Error::from_code(what, err.code())
}

fn map_repacketizer(what: &'static str, err: repacketizer_api::RepacketizerError) -> Error {
	Error::from_code(what, err.code())
}

fn map_ms_encoder(what: &'static str, err: multistream_api::OpusMultistreamEncoderError) -> Error {
	Error::from_code(what, err.code())
}

fn map_ms_decoder(what: &'static str, err: multistream_api::OpusMultistreamDecoderError) -> Error {
	Error::from_code(what, err.code())
}

fn check_len(val: usize) -> i32 {
	match i32::try_from(val) {
		Ok(value) => value,
		Err(_) => panic!("length out of range: {}", val),
	}
}

#[inline]
fn len<T>(slice: &[T]) -> i32 {
	check_len(slice.len())
}

/// An Opus encoder with associated state.
#[derive(Debug)]
pub struct Encoder {
	inner: encoder_api::OpusEncoder<'static>,
	sample_rate: u32,
	channels: Channels,
	application: Application,
	dtx_enabled: bool,
	last_in_dtx: bool,
}

unsafe impl Send for Encoder {}

impl Encoder {
	/// Create and initialize an encoder.
	pub fn new(sample_rate: u32, channels: Channels, mode: Application) -> Result<Encoder> {
		let inner =
			encoder_api::opus_encoder_create(sample_rate as i32, channels as i32, mode as i32)
				.map_err(|err| map_encoder_init("opus_encoder_create", err))?;
		Ok(Encoder {
			inner,
			sample_rate,
			channels,
			application: mode,
			dtx_enabled: false,
			last_in_dtx: false,
		})
	}

	/// Encode an Opus frame.
	pub fn encode(&mut self, input: &[i16], output: &mut [u8]) -> Result<usize> {
		if output.is_empty() {
			return Err(Error::bad_arg("opus_encode"));
		}
		let frame_size = input.len() / self.channels as usize;
		let len = encoder_api::opus_encode(&mut self.inner, input, frame_size, output)
			.map_err(|err| map_encode("opus_encode", err))?;
		self.last_in_dtx = self.dtx_enabled && len <= 2;
		Ok(len)
	}

	/// Encode an Opus frame from floating point input.
	pub fn encode_float(&mut self, input: &[f32], output: &mut [u8]) -> Result<usize> {
		if output.is_empty() {
			return Err(Error::bad_arg("opus_encode_float"));
		}
		let frame_size = input.len() / self.channels as usize;
		let len = encoder_api::opus_encode_float(&mut self.inner, input, frame_size, output)
			.map_err(|err| map_encode("opus_encode_float", err))?;
		self.last_in_dtx = self.dtx_enabled && len <= 2;
		Ok(len)
	}

	/// Encode an Opus frame to a new buffer.
	pub fn encode_vec(&mut self, input: &[i16], max_size: usize) -> Result<Vec<u8>> {
		let mut output = vec![0; max_size];
		let result = self.encode(input, &mut output)?;
		output.truncate(result);
		Ok(output)
	}

	/// Encode an Opus frame from floating point input to a new buffer.
	pub fn encode_vec_float(&mut self, input: &[f32], max_size: usize) -> Result<Vec<u8>> {
		let mut output = vec![0; max_size];
		let result = self.encode_float(input, &mut output)?;
		output.truncate(result);
		Ok(output)
	}

	/// Reset the codec state to be equivalent to a freshly initialized state.
	pub fn reset_state(&mut self) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::ResetState,
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_RESET_STATE)", err))
	}

	/// Get the final range of the codec's entropy coder.
	pub fn get_final_range(&mut self) -> Result<u32> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetFinalRange(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_FINAL_RANGE_REQUEST)", err))?;
		Ok(value)
	}

	/// Get the encoder's configured bandpass.
	pub fn get_bandwidth(&mut self) -> Result<Bandwidth> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetBandwidth(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_BANDWIDTH_REQUEST)", err))?;
		Bandwidth::decode(value, "opus_encoder_ctl(OPUS_GET_BANDWIDTH_REQUEST)")
	}

	/// Get the sample rate the encoder was initialized with.
	pub fn get_sample_rate(&mut self) -> Result<u32> {
		Ok(self.sample_rate)
	}

	/// If set to true, disables the use of phase inversion for intensity stereo.
	pub fn set_phase_inversion_disabled(&mut self, disabled: bool) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetPhaseInversionDisabled(disabled),
		)
		.map_err(|err| {
			map_encoder_ctl("opus_encoder_ctl(OPUS_SET_PHASE_INVERSION_DISABLED_REQUEST)", err)
		})
	}

	/// Get the encoder's configured phase inversion status.
	pub fn get_phase_inversion_disabled(&mut self) -> Result<bool> {
		let mut value = false;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetPhaseInversionDisabled(&mut value),
		)
		.map_err(|err| {
			map_encoder_ctl("opus_encoder_ctl(OPUS_GET_PHASE_INVERSION_DISABLED_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Get the DTX state of the encoder.
	pub fn get_in_dtx(&mut self) -> Result<bool> {
		Ok(self.last_in_dtx)
	}

	/// Configures the encoder's computational complexity.
	pub fn set_complexity(&mut self, value: i32) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetComplexity(value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_COMPLEXITY_REQUEST)", err))
	}

	/// Gets the encoder's complexity configuration.
	pub fn get_complexity(&mut self) -> Result<i32> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetComplexity(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_COMPLEXITY_REQUEST)", err))?;
		Ok(value)
	}

	/// Set the encoder's bitrate.
	pub fn set_bitrate(&mut self, value: Bitrate) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetBitrate(value.raw()),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_BITRATE_REQUEST)", err))
	}

	/// Get the encoder's bitrate.
	pub fn get_bitrate(&mut self) -> Result<Bitrate> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetBitrate(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_BITRATE_REQUEST)", err))?;
		Ok(Bitrate::from_raw(value))
	}

	/// Enable or disable variable bitrate.
	pub fn set_vbr(&mut self, vbr: bool) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetVbr(vbr),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_VBR_REQUEST)", err))
	}

	/// Determine if variable bitrate is enabled.
	pub fn get_vbr(&mut self) -> Result<bool> {
		let mut value = false;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetVbr(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_VBR_REQUEST)", err))?;
		Ok(value)
	}

	/// Enable or disable constrained VBR.
	pub fn set_vbr_constraint(&mut self, vbr: bool) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetVbrConstraint(vbr),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_VBR_CONSTRAINT_REQUEST)", err))
	}

	/// Determine if constrained VBR is enabled.
	pub fn get_vbr_constraint(&mut self) -> Result<bool> {
		let mut value = false;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetVbrConstraint(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_VBR_CONSTRAINT_REQUEST)", err))?;
		Ok(value)
	}

	/// Configures mono/stereo forcing in the encoder.
	pub fn set_force_channels(&mut self, value: Option<Channels>) -> Result<()> {
		let raw = match value {
			None => OPUS_AUTO,
			Some(Channels::Mono) => 1,
			Some(Channels::Stereo) => 2,
		};
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetForceChannels(raw),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_FORCE_CHANNELS_REQUEST)", err))
	}

	/// Gets the encoder's forced channel configuration.
	pub fn get_force_channels(&mut self) -> Result<Option<Channels>> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetForceChannels(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_FORCE_CHANNELS_REQUEST)", err))?;
		match value {
			OPUS_AUTO => Ok(None),
			1 => Ok(Some(Channels::Mono)),
			2 => Ok(Some(Channels::Stereo)),
			_ => Err(Error::bad_arg("opus_encoder_ctl(OPUS_GET_FORCE_CHANNELS_REQUEST)")),
		}
	}

	/// Configure the maximum bandpass that the encoder will select automatically.
	pub fn set_max_bandwidth(&mut self, bandwidth: Bandwidth) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetMaxBandwidth(bandwidth.raw()),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_MAX_BANDWIDTH_REQUEST)", err))
	}

	/// Get the encoder's configured maximum allowed bandpass.
	pub fn get_max_bandwidth(&mut self) -> Result<Bandwidth> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetMaxBandwidth(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_MAX_BANDWIDTH_REQUEST)", err))?;
		Bandwidth::decode(value, "opus_encoder_ctl(OPUS_GET_MAX_BANDWIDTH_REQUEST)")
	}

	/// Set the encoder's bandpass to a specific value.
	pub fn set_bandwidth(&mut self, bandwidth: Bandwidth) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetBandwidth(bandwidth.raw()),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_BANDWIDTH_REQUEST)", err))
	}

	/// Configure the type of signal being encoded.
	pub fn set_signal(&mut self, signal: Signal) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetSignal(signal.raw()),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_SIGNAL_REQUEST)", err))
	}

	/// Gets the encoder's configured signal type.
	pub fn get_signal(&mut self) -> Result<Signal> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetSignal(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_SIGNAL_REQUEST)", err))?;
		Signal::from_raw(value, "opus_encoder_ctl(OPUS_GET_SIGNAL_REQUEST)")
	}

	/// Configure the encoder's intended application.
	pub fn set_application(&mut self, application: Application) -> Result<()> {
		self.application = application;
		Ok(())
	}

	/// Get the encoder's configured application.
	pub fn get_application(&mut self) -> Result<Application> {
		Application::from_raw(
			self.application as i32,
			"opus_encoder_ctl(OPUS_GET_APPLICATION_REQUEST)",
		)
	}

	/// Gets the total samples of delay added by the entire codec.
	pub fn get_lookahead(&mut self) -> Result<i32> {
		Ok((self.sample_rate / 250) as i32)
	}

	/// Configures the encoder's use of inband forward error correction (FEC).
	pub fn set_inband_fec(&mut self, value: bool) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetInbandFec(value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_INBAND_FEC_REQUEST)", err))
	}

	/// Gets encoder's configured use of inband forward error correction.
	pub fn get_inband_fec(&mut self) -> Result<bool> {
		let mut value = false;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetInbandFec(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_INBAND_FEC_REQUEST)", err))?;
		Ok(value)
	}

	/// Sets the encoder's expected packet loss percentage.
	pub fn set_packet_loss_perc(&mut self, value: i32) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetPacketLossPerc(value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_PACKET_LOSS_PERC_REQUEST)", err))
	}

	/// Gets the encoder's expected packet loss percentage.
	pub fn get_packet_loss_perc(&mut self) -> Result<i32> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetPacketLossPerc(&mut value),
		)
		.map_err(|err| {
			map_encoder_ctl("opus_encoder_ctl(OPUS_GET_PACKET_LOSS_PERC_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Configures the encoder's use of discontinuous transmission (DTX).
	pub fn set_dtx(&mut self, value: bool) -> Result<()> {
		self.dtx_enabled = value;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetDtx(value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_DTX_REQUEST)", err))
	}

	/// Gets encoder's configured use of discontinuous transmission (DTX).
	pub fn get_dtx(&mut self) -> Result<bool> {
		let mut value = false;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetDtx(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_DTX_REQUEST)", err))?;
		Ok(value)
	}

	/// Configures the depth of signal being encoded.
	pub fn set_lsb_depth(&mut self, depth: i32) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetLsbDepth(depth),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_LSB_DEPTH_REQUEST)", err))
	}

	/// Gets the encoder's configured signal depth.
	pub fn get_lsb_depth(&mut self) -> Result<i32> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetLsbDepth(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_LSB_DEPTH_REQUEST)", err))?;
		Ok(value)
	}

	/// Configures the encoder's use of variable duration frames.
	pub fn set_expert_frame_duration(&mut self, framesize: FrameSize) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetExpertFrameDuration(framesize.raw()),
		)
		.map_err(|err| {
			map_encoder_ctl("opus_encoder_ctl(OPUS_SET_EXPERT_FRAME_DURATION_REQUEST)", err)
		})
	}

	/// Gets the encoder's configured use of variable duration frames.
	pub fn get_expert_frame_duration(&mut self) -> Result<FrameSize> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetExpertFrameDuration(&mut value),
		)
		.map_err(|err| {
			map_encoder_ctl("opus_encoder_ctl(OPUS_GET_EXPERT_FRAME_DURATION_REQUEST)", err)
		})?;
		FrameSize::from_raw(value, "opus_encoder_ctl(OPUS_GET_EXPERT_FRAME_DURATION_REQUEST)")
	}

	/// If set to true, disables almost all use of prediction.
	pub fn set_prediction_disabled(&mut self, disabled: bool) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetPredictionDisabled(disabled),
		)
		.map_err(|err| {
			map_encoder_ctl("opus_encoder_ctl(OPUS_SET_PREDICTION_DISABLED_REQUEST)", err)
		})
	}

	/// Gets the encoder's configured prediction status.
	pub fn get_prediction_disabled(&mut self) -> Result<bool> {
		let mut value = false;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetPredictionDisabled(&mut value),
		)
		.map_err(|err| {
			map_encoder_ctl("opus_encoder_ctl(OPUS_GET_PREDICTION_DISABLED_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Set the DRED duration in milliseconds.
	pub fn set_dred_duration(&mut self, duration_ms: i32) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetDredDuration(duration_ms),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_DRED_DURATION_REQUEST)", err))
	}

	/// Get the configured DRED duration in milliseconds.
	pub fn get_dred_duration(&mut self) -> Result<i32> {
		let mut value = 0;
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::GetDredDuration(&mut value),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_GET_DRED_DURATION_REQUEST)", err))?;
		Ok(value)
	}

	/// Set a DNN model blob for the encoder.
	pub fn set_dnn_blob(&mut self, blob: &[u8]) -> Result<()> {
		encoder_api::opus_encoder_ctl(
			&mut self.inner,
			encoder_api::OpusEncoderCtlRequest::SetDnnBlob(blob),
		)
		.map_err(|err| map_encoder_ctl("opus_encoder_ctl(OPUS_SET_DNN_BLOB_REQUEST)", err))
	}
}

/// An Opus decoder with associated state.
#[derive(Debug)]
pub struct Decoder {
	inner: decoder_api::OpusDecoder<'static>,
	sample_rate: u32,
	channels: Channels,
}

unsafe impl Send for Decoder {}

impl Decoder {
	/// Create and initialize a decoder.
	pub fn new(sample_rate: u32, channels: Channels) -> Result<Decoder> {
		let inner = decoder_api::opus_decoder_create(sample_rate as i32, channels as i32)
			.map_err(|err| map_decoder_init("opus_decoder_create", err))?;
		Ok(Decoder { inner, sample_rate, channels })
	}

	/// Decode an Opus packet.
	pub fn decode(&mut self, input: &[u8], output: &mut [i16], fec: bool) -> Result<usize> {
		let packet = if input.is_empty() { None } else { Some(input) };
		decoder_api::opus_decode(
			&mut self.inner,
			packet,
			input.len(),
			output,
			output.len() / self.channels as usize,
			fec,
		)
		.map_err(|err| map_decode("opus_decode", err))
	}

	/// Decode an Opus packet with floating point output.
	pub fn decode_float(&mut self, input: &[u8], output: &mut [f32], fec: bool) -> Result<usize> {
		let packet = if input.is_empty() { None } else { Some(input) };
		decoder_api::opus_decode_float(
			&mut self.inner,
			packet,
			input.len(),
			output,
			output.len() / self.channels as usize,
			fec,
		)
		.map_err(|err| map_decode("opus_decode_float", err))
	}

	/// Get the number of samples *per channel* of an Opus packet.
	pub fn get_nb_samples(&self, packet: &[u8]) -> Result<usize> {
		decoder_api::opus_decoder_get_nb_samples(&self.inner, packet, packet.len())
			.map_err(|err| map_packet("opus_decoder_get_nb_samples", err))
	}

	/// Reset the codec state to be equivalent to a freshly initialized state.
	pub fn reset_state(&mut self) -> Result<()> {
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::ResetState,
		)
		.map_err(|err| map_decoder_ctl("opus_decoder_ctl(OPUS_RESET_STATE)", err))
	}

	/// Get the final range of the codec's entropy coder.
	pub fn get_final_range(&mut self) -> Result<u32> {
		let mut value = 0;
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::GetFinalRange(&mut value),
		)
		.map_err(|err| map_decoder_ctl("opus_decoder_ctl(OPUS_GET_FINAL_RANGE_REQUEST)", err))?;
		Ok(value)
	}

	/// Get the encoder's configured bandpass.
	pub fn get_bandwidth(&mut self) -> Result<Bandwidth> {
		let mut value = 0;
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::GetBandwidth(&mut value),
		)
		.map_err(|err| map_decoder_ctl("opus_decoder_ctl(OPUS_GET_BANDWIDTH_REQUEST)", err))?;
		Bandwidth::decode(value, "opus_decoder_ctl(OPUS_GET_BANDWIDTH_REQUEST)")
	}

	/// Get the sampling rate the decoder was initialized with.
	pub fn get_sample_rate(&mut self) -> Result<u32> {
		Ok(self.sample_rate)
	}

	/// If set to true, disables the use of phase inversion for intensity stereo.
	pub fn set_phase_inversion_disabled(&mut self, disabled: bool) -> Result<()> {
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::SetPhaseInversionDisabled(disabled),
		)
		.map_err(|err| {
			map_decoder_ctl("opus_decoder_ctl(OPUS_SET_PHASE_INVERSION_DISABLED_REQUEST)", err)
		})
	}

	/// Get the decoder's configured phase inversion status.
	pub fn get_phase_inversion_disabled(&mut self) -> Result<bool> {
		let mut value = false;
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::GetPhaseInversionDisabled(&mut value),
		)
		.map_err(|err| {
			map_decoder_ctl("opus_decoder_ctl(OPUS_GET_PHASE_INVERSION_DISABLED_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Configures decoder gain adjustment.
	pub fn set_gain(&mut self, gain: i32) -> Result<()> {
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::SetGain(gain),
		)
		.map_err(|err| map_decoder_ctl("opus_decoder_ctl(OPUS_SET_GAIN_REQUEST)", err))
	}

	/// Gets the decoder's configured gain adjustment.
	pub fn get_gain(&mut self) -> Result<i32> {
		let mut value = 0;
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::GetGain(&mut value),
		)
		.map_err(|err| map_decoder_ctl("opus_decoder_ctl(OPUS_GET_GAIN_REQUEST)", err))?;
		Ok(value)
	}

	/// Configures the decoder's complexity.
	pub fn set_complexity(&mut self, value: i32) -> Result<()> {
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::SetComplexity(value),
		)
		.map_err(|err| map_decoder_ctl("opus_decoder_ctl(OPUS_SET_COMPLEXITY_REQUEST)", err))
	}

	/// Gets the decoder's complexity configuration.
	pub fn get_complexity(&mut self) -> Result<i32> {
		let mut value = 0;
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::GetComplexity(&mut value),
		)
		.map_err(|err| map_decoder_ctl("opus_decoder_ctl(OPUS_GET_COMPLEXITY_REQUEST)", err))?;
		Ok(value)
	}

	/// Gets the duration of the last packet successfully decoded or concealed.
	pub fn get_last_packet_duration(&mut self) -> Result<u32> {
		let mut value = 0;
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::GetLastPacketDuration(&mut value),
		)
		.map_err(|err| {
			map_decoder_ctl("opus_decoder_ctl(OPUS_GET_LAST_PACKET_DURATION_REQUEST)", err)
		})?;
		Ok(value as u32)
	}

	/// Gets the pitch of the last decoded frame, if available.
	pub fn get_pitch(&mut self) -> Result<i32> {
		let mut value = 0;
		decoder_api::opus_decoder_ctl(
			&mut self.inner,
			decoder_api::OpusDecoderCtlRequest::GetPitch(&mut value),
		)
		.map_err(|err| map_decoder_ctl("opus_decoder_ctl(OPUS_GET_PITCH_REQUEST)", err))?;
		Ok(value)
	}
}

/// Analyze raw Opus packets.
pub mod packet {
	use super::*;

	/// A parsed Opus packet.
	#[derive(Debug)]
	pub struct Packet<'a> {
		/// The TOC byte of the packet.
		pub toc: u8,
		/// The frames contained in the packet.
		pub frames: Vec<&'a [u8]>,
		/// The offset into the packet at which the payload is located.
		pub payload_offset: usize,
	}

	/// Get the bandwidth of an Opus packet.
	pub fn get_bandwidth(packet: &[u8]) -> Result<Bandwidth> {
		packet_api::opus_packet_get_bandwidth(packet)
			.map(Bandwidth::from_packet)
			.map_err(|err| map_packet("opus_packet_get_bandwidth", err))
	}

	/// Get the number of channels from an Opus packet.
	pub fn get_nb_channels(packet: &[u8]) -> Result<Channels> {
		match packet_api::opus_packet_get_nb_channels(packet)
			.map_err(|err| map_packet("opus_packet_get_nb_channels", err))?
		{
			1 => Ok(Channels::Mono),
			2 => Ok(Channels::Stereo),
			_ => Err(Error::bad_arg("opus_packet_get_nb_channels")),
		}
	}

	/// Get the number of frames in an Opus packet.
	pub fn get_nb_frames(packet: &[u8]) -> Result<usize> {
		packet_api::opus_packet_get_nb_frames(packet, packet.len())
			.map_err(|err| map_packet("opus_packet_get_nb_frames", err))
	}

	/// Get the number of samples of an Opus packet.
	pub fn get_nb_samples(packet: &[u8], sample_rate: u32) -> Result<usize> {
		packet_api::opus_packet_get_nb_samples(packet, packet.len(), sample_rate)
			.map_err(|err| map_packet("opus_packet_get_nb_samples", err))
	}

	/// Get the number of samples per frame from an Opus packet.
	pub fn get_samples_per_frame(packet: &[u8], sample_rate: u32) -> Result<usize> {
		packet_api::opus_packet_get_samples_per_frame(packet, sample_rate)
			.map_err(|err| map_packet("opus_packet_get_samples_per_frame", err))
	}

	/// Parse an Opus packet into one or more frames.
	pub fn parse(packet: &[u8]) -> Result<Packet<'_>> {
		let parsed = packet_api::opus_packet_parse(packet, packet.len())
			.map_err(|err| map_packet("opus_packet_parse", err))?;
		let mut frames = Vec::with_capacity(parsed.frame_count);
		for frame in parsed.frames.iter().take(parsed.frame_count) {
			frames.push(*frame);
		}
		Ok(Packet {
			toc: parsed.toc,
			frames,
			payload_offset: parsed.payload_offset,
		})
	}

	/// Pad a given Opus packet to a larger size.
	pub fn pad(packet: &mut [u8], prev_len: usize) -> Result<usize> {
		repacketizer_api::opus_packet_pad(packet, prev_len, packet.len())
			.map_err(|err| map_repacketizer("opus_packet_pad", err))?;
		Ok(0)
	}

	/// Remove all padding from a given Opus packet and rewrite the TOC sequence.
	pub fn unpad(packet: &mut [u8]) -> Result<usize> {
		repacketizer_api::opus_packet_unpad(packet, packet.len())
			.map_err(|err| map_repacketizer("opus_packet_unpad", err))
	}

	/// Pad a given Opus multi-stream packet to a larger size.
	pub fn multistream_pad(packet: &mut [u8], prev_len: usize, nb_streams: u8) -> Result<usize> {
		repacketizer_api::opus_multistream_packet_pad(
			packet,
			prev_len,
			packet.len(),
			nb_streams as usize,
		)
		.map_err(|err| map_repacketizer("opus_multistream_packet_pad", err))?;
		Ok(0)
	}

	/// Remove all padding from a given Opus multi-stream packet.
	pub fn multistream_unpad(packet: &mut [u8], nb_streams: u8) -> Result<usize> {
		repacketizer_api::opus_multistream_packet_unpad(packet, packet.len(), nb_streams as usize)
			.map_err(|err| map_repacketizer("opus_multistream_packet_unpad", err))
	}
}

/// Soft-clipping to bring a float signal within the [-1,1] range.
#[derive(Debug)]
pub struct SoftClip {
	channels: Channels,
	memory: [f32; 2],
}

impl SoftClip {
	/// Initialize a new soft-clipping state.
	pub fn new(channels: Channels) -> SoftClip {
		SoftClip { channels, memory: [0.0; 2] }
	}

	/// Apply soft-clipping to a float signal.
	pub fn apply(&mut self, signal: &mut [f32]) {
		let channels = self.channels as usize;
		opus_api::opus_pcm_soft_clip(
			signal,
			signal.len() / channels,
			channels,
			&mut self.memory[..channels],
		);
	}
}

/// A repacketizer used to merge together or split apart multiple Opus packets.
pub struct Repacketizer {
	inner: repacketizer_api::OpusRepacketizer,
}

impl std::fmt::Debug for Repacketizer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Repacketizer").finish()
	}
}

unsafe impl Send for Repacketizer {}

impl Repacketizer {
	/// Create and initialize a repacketizer.
	pub fn new() -> Result<Repacketizer> {
		Ok(Repacketizer {
			inner: repacketizer_api::OpusRepacketizer::new(),
		})
	}

	/// Shortcut to combine several smaller packets into one larger one.
	pub fn combine(&mut self, input: &[&[u8]], output: &mut [u8]) -> Result<usize> {
		let mut state = self.begin();
		for &packet in input {
			state.cat(packet)?;
		}
		state.out(output)
	}

	/// Begin using the repacketizer.
	#[allow(clippy::needless_lifetimes)]
	pub fn begin<'rp, 'buf>(&'rp mut self) -> RepacketizerState<'rp, 'buf> {
		self.inner.opus_repacketizer_init();
		RepacketizerState { inner: &mut self.inner, phantom: PhantomData }
	}
}

/// An in-progress repacketization.
pub struct RepacketizerState<'rp, 'buf> {
	inner: &'rp mut repacketizer_api::OpusRepacketizer,
	phantom: PhantomData<(&'rp mut Repacketizer, &'buf [u8])>,
}

impl<'rp, 'buf> std::fmt::Debug for RepacketizerState<'rp, 'buf> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("RepacketizerState").finish()
	}
}

unsafe impl<'rp, 'buf> Send for RepacketizerState<'rp, 'buf> {}

impl<'rp, 'buf> RepacketizerState<'rp, 'buf> {
	/// Add a packet to the current repacketizer state.
	pub fn cat(&mut self, packet: &'buf [u8]) -> Result<()> {
		self.inner
			.opus_repacketizer_cat(packet, packet.len())
			.map_err(|err| map_repacketizer("opus_repacketizer_cat", err))
	}

	/// Add a packet to the current repacketizer state, moving it.
	pub fn cat_move<'b2>(self, packet: &'b2 [u8]) -> Result<RepacketizerState<'rp, 'b2>>
	where
		'buf: 'b2,
	{
		let mut shorter = self;
		shorter.cat(packet)?;
		Ok(shorter)
	}

	/// Get the total number of frames contained in packet data submitted so far.
	pub fn get_nb_frames(&mut self) -> usize {
		self.inner.opus_repacketizer_get_nb_frames()
	}

	/// Construct a new packet from data previously submitted via `cat`.
	pub fn out(&mut self, buffer: &mut [u8]) -> Result<usize> {
		self.inner
			.opus_repacketizer_out(buffer, buffer.len())
			.map_err(|err| map_repacketizer("opus_repacketizer_out", err))
	}

	/// Construct a new packet from data previously submitted via `cat`.
	pub fn out_range(&mut self, begin: usize, end: usize, buffer: &mut [u8]) -> Result<usize> {
		self.inner
			.opus_repacketizer_out_range(begin, end, buffer, buffer.len())
			.map_err(|err| map_repacketizer("opus_repacketizer_out_range", err))
	}
}

/// Combine individual Opus streams in a single packet, up to 255 channels.
#[derive(Debug)]
pub struct MSEncoder {
	inner: multistream_api::OpusMultistreamEncoder<'static>,
	channels: i32,
	sample_rate: u32,
	application: Application,
	dtx_enabled: bool,
	last_in_dtx: bool,
}

/// A surround sound multistream encoder with its configuration.
#[derive(Debug)]
pub struct SurroundEncoder {
	/// The multistream encoder instance.
	pub encoder: MSEncoder,
	/// The number of streams that will be used.
	pub streams: u8,
	/// The number of coupled (stereo) streams.
	pub coupled_streams: u8,
	/// Channel mapping array showing how input channels map to streams.
	pub mapping: Vec<u8>,
}

unsafe impl Send for MSEncoder {}

impl MSEncoder {
	/// Create and initialize a multistream encoder.
	pub fn new(
		sample_rate: u32,
		streams: u8,
		coupled_streams: u8,
		mapping: &[u8],
		application: Application,
	) -> Result<MSEncoder> {
		let inner = multistream_api::opus_multistream_encoder_create(
			sample_rate as i32,
			mapping.len(),
			streams as usize,
			coupled_streams as usize,
			mapping,
			application as i32,
		)
		.map_err(|err| map_ms_encoder("opus_multistream_encoder_create", err))?;
		Ok(MSEncoder {
			inner,
			channels: len(mapping),
			sample_rate,
			application,
			dtx_enabled: false,
			last_in_dtx: false,
		})
	}

	/// Create and initialize a multistream encoder for surround sound.
	pub fn new_surround(
		sample_rate: u32,
		channels: u8,
		mapping_family: i32,
		application: Application,
	) -> Result<SurroundEncoder> {
		let mapping_family = u8::try_from(mapping_family)
			.map_err(|_| Error::bad_arg("opus_multistream_surround_encoder_create"))?;
		let (inner, layout) = multistream_api::opus_multistream_surround_encoder_create(
			sample_rate as i32,
			channels as usize,
			mapping_family,
			application as i32,
		)
		.map_err(|err| map_ms_encoder("opus_multistream_surround_encoder_create", err))?;
		Ok(SurroundEncoder {
			encoder: MSEncoder {
				inner,
				channels: channels as i32,
				sample_rate,
				application,
				dtx_enabled: false,
				last_in_dtx: false,
			},
			streams: layout.streams as u8,
			coupled_streams: layout.coupled_streams as u8,
			mapping: layout.mapping,
		})
	}

	/// Encode an Opus frame.
	pub fn encode(&mut self, input: &[i16], output: &mut [u8]) -> Result<usize> {
		if output.is_empty() {
			return Err(Error::bad_arg("opus_multistream_encode"));
		}
		let frame_size = input.len() / self.channels as usize;
		let len =
			multistream_api::opus_multistream_encode(&mut self.inner, input, frame_size, output)
				.map_err(|err| map_ms_encoder("opus_multistream_encode", err))?;
		self.last_in_dtx = self.dtx_enabled && len <= 2;
		Ok(len)
	}

	/// Encode an Opus frame from floating point input.
	pub fn encode_float(&mut self, input: &[f32], output: &mut [u8]) -> Result<usize> {
		if output.is_empty() {
			return Err(Error::bad_arg("opus_multistream_encode_float"));
		}
		let frame_size = input.len() / self.channels as usize;
		let len = multistream_api::opus_multistream_encode_float(
			&mut self.inner,
			input,
			frame_size,
			output,
		)
		.map_err(|err| map_ms_encoder("opus_multistream_encode_float", err))?;
		self.last_in_dtx = self.dtx_enabled && len <= 2;
		Ok(len)
	}

	/// Encode an Opus frame to a new buffer.
	pub fn encode_vec(&mut self, input: &[i16], max_size: usize) -> Result<Vec<u8>> {
		let mut output = vec![0; max_size];
		let result = self.encode(input, &mut output)?;
		output.truncate(result);
		Ok(output)
	}

	/// Encode an Opus frame from floating point input to a new buffer.
	pub fn encode_vec_float(&mut self, input: &[f32], max_size: usize) -> Result<Vec<u8>> {
		let mut output = vec![0; max_size];
		let result = self.encode_float(input, &mut output)?;
		output.truncate(result);
		Ok(output)
	}

	/// Reset the codec state to be equivalent to a freshly initialized state.
	pub fn reset_state(&mut self) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::ResetState,
		)
		.map_err(|err| map_ms_encoder("opus_multistream_encoder_ctl(OPUS_RESET_STATE)", err))
	}

	/// Get the final range of the codec's entropy coder.
	pub fn get_final_range(&mut self) -> Result<u32> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetFinalRange(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_FINAL_RANGE_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Get the encoder's configured bandpass.
	pub fn get_bandwidth(&mut self) -> Result<Bandwidth> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetBandwidth(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_BANDWIDTH_REQUEST)", err)
		})?;
		Bandwidth::decode(value, "opus_multistream_encoder_ctl(OPUS_GET_BANDWIDTH_REQUEST)")
	}

	/// Get the sample rate the encoder was initialized with.
	pub fn get_sample_rate(&mut self) -> Result<u32> {
		Ok(self.sample_rate)
	}

	/// If set to true, disables the use of phase inversion for intensity stereo.
	pub fn set_phase_inversion_disabled(&mut self, disabled: bool) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetPhaseInversionDisabled(disabled),
		)
		.map_err(|err| {
			map_ms_encoder(
				"opus_multistream_encoder_ctl(OPUS_SET_PHASE_INVERSION_DISABLED_REQUEST)",
				err,
			)
		})
	}

	/// Get the encoder's configured phase inversion status.
	pub fn get_phase_inversion_disabled(&mut self) -> Result<bool> {
		let mut value = false;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetPhaseInversionDisabled(
				&mut value,
			),
		)
		.map_err(|err| {
			map_ms_encoder(
				"opus_multistream_encoder_ctl(OPUS_GET_PHASE_INVERSION_DISABLED_REQUEST)",
				err,
			)
		})?;
		Ok(value)
	}

	/// Get the DTX state of the encoder.
	pub fn get_in_dtx(&mut self) -> Result<bool> {
		Ok(self.last_in_dtx)
	}

	/// Configures the encoder's computational complexity.
	pub fn set_complexity(&mut self, value: i32) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetComplexity(value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_COMPLEXITY_REQUEST)", err)
		})
	}

	/// Gets the encoder's complexity configuration.
	pub fn get_complexity(&mut self) -> Result<i32> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetComplexity(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_COMPLEXITY_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Set the encoder's bitrate.
	pub fn set_bitrate(&mut self, value: Bitrate) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetBitrate(value.raw()),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_BITRATE_REQUEST)", err)
		})
	}

	/// Get the encoder's bitrate.
	pub fn get_bitrate(&mut self) -> Result<Bitrate> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetBitrate(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_BITRATE_REQUEST)", err)
		})?;
		Ok(Bitrate::from_raw(value))
	}

	/// Enable or disable variable bitrate.
	pub fn set_vbr(&mut self, vbr: bool) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetVbr(vbr),
		)
		.map_err(|err| map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_VBR_REQUEST)", err))
	}

	/// Determine if variable bitrate is enabled.
	pub fn get_vbr(&mut self) -> Result<bool> {
		let mut value = false;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetVbr(&mut value),
		)
		.map_err(|err| map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_VBR_REQUEST)", err))?;
		Ok(value)
	}

	/// Enable or disable constrained VBR.
	pub fn set_vbr_constraint(&mut self, vbr: bool) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetVbrConstraint(vbr),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_VBR_CONSTRAINT_REQUEST)", err)
		})
	}

	/// Determine if constrained VBR is enabled.
	pub fn get_vbr_constraint(&mut self) -> Result<bool> {
		let mut value = false;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetVbrConstraint(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_VBR_CONSTRAINT_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Configures mono/stereo forcing in the encoder.
	pub fn set_force_channels(&mut self, value: Option<Channels>) -> Result<()> {
		let raw = match value {
			None => OPUS_AUTO,
			Some(Channels::Mono) => 1,
			Some(Channels::Stereo) => 2,
		};
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetForceChannels(raw),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_FORCE_CHANNELS_REQUEST)", err)
		})
	}

	/// Gets the encoder's forced channel configuration.
	pub fn get_force_channels(&mut self) -> Result<Option<Channels>> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetForceChannels(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_FORCE_CHANNELS_REQUEST)", err)
		})?;
		match value {
			OPUS_AUTO => Ok(None),
			1 => Ok(Some(Channels::Mono)),
			2 => Ok(Some(Channels::Stereo)),
			_ => {
				Err(Error::bad_arg("opus_multistream_encoder_ctl(OPUS_GET_FORCE_CHANNELS_REQUEST)"))
			}
		}
	}

	/// Configure the maximum bandpass that the encoder will select automatically.
	pub fn set_max_bandwidth(&mut self, bandwidth: Bandwidth) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetMaxBandwidth(bandwidth.raw()),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_MAX_BANDWIDTH_REQUEST)", err)
		})
	}

	/// Get the encoder's configured maximum allowed bandpass.
	pub fn get_max_bandwidth(&mut self) -> Result<Bandwidth> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetMaxBandwidth(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_MAX_BANDWIDTH_REQUEST)", err)
		})?;
		Bandwidth::decode(value, "opus_multistream_encoder_ctl(OPUS_GET_MAX_BANDWIDTH_REQUEST)")
	}

	/// Set the encoder's bandpass to a specific value.
	pub fn set_bandwidth(&mut self, bandwidth: Bandwidth) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetBandwidth(bandwidth.raw()),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_BANDWIDTH_REQUEST)", err)
		})
	}

	/// Configure the type of signal being encoded.
	pub fn set_signal(&mut self, signal: Signal) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetSignal(signal.raw()),
		)
		.map_err(|err| map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_SIGNAL_REQUEST)", err))
	}

	/// Gets the encoder's configured signal type.
	pub fn get_signal(&mut self) -> Result<Signal> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetSignal(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_SIGNAL_REQUEST)", err)
		})?;
		Signal::from_raw(value, "opus_multistream_encoder_ctl(OPUS_GET_SIGNAL_REQUEST)")
	}

	/// Configure the encoder's intended application.
	pub fn set_application(&mut self, application: Application) -> Result<()> {
		self.application = application;
		Ok(())
	}

	/// Get the encoder's configured application.
	pub fn get_application(&mut self) -> Result<Application> {
		Application::from_raw(
			self.application as i32,
			"opus_multistream_encoder_ctl(OPUS_GET_APPLICATION_REQUEST)",
		)
	}

	/// Gets the total samples of delay added by the entire codec.
	pub fn get_lookahead(&mut self) -> Result<i32> {
		Ok((self.sample_rate / 250) as i32)
	}

	/// Configures the encoder's use of inband forward error correction (FEC).
	pub fn set_inband_fec(&mut self, value: bool) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetInbandFec(value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_INBAND_FEC_REQUEST)", err)
		})
	}

	/// Gets encoder's configured use of inband forward error correction.
	pub fn get_inband_fec(&mut self) -> Result<bool> {
		let mut value = false;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetInbandFec(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_INBAND_FEC_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Sets the encoder's expected packet loss percentage.
	pub fn set_packet_loss_perc(&mut self, value: i32) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetPacketLossPerc(value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_PACKET_LOSS_PERC_REQUEST)", err)
		})
	}

	/// Gets the encoder's expected packet loss percentage.
	pub fn get_packet_loss_perc(&mut self) -> Result<i32> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetPacketLossPerc(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_PACKET_LOSS_PERC_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Configures the encoder's use of discontinuous transmission (DTX).
	pub fn set_dtx(&mut self, value: bool) -> Result<()> {
		self.dtx_enabled = value;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetDtx(value),
		)
		.map_err(|err| map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_DTX_REQUEST)", err))
	}

	/// Gets encoder's configured use of discontinuous transmission (DTX).
	pub fn get_dtx(&mut self) -> Result<bool> {
		let mut value = false;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetDtx(&mut value),
		)
		.map_err(|err| map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_DTX_REQUEST)", err))?;
		Ok(value)
	}

	/// Configures the depth of signal being encoded.
	pub fn set_lsb_depth(&mut self, depth: i32) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetLsbDepth(depth),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_LSB_DEPTH_REQUEST)", err)
		})
	}

	/// Gets the encoder's configured signal depth.
	pub fn get_lsb_depth(&mut self) -> Result<i32> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetLsbDepth(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_LSB_DEPTH_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Configures the encoder's use of variable duration frames.
	pub fn set_expert_frame_duration(&mut self, framesize: FrameSize) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetExpertFrameDuration(
				framesize.raw(),
			),
		)
		.map_err(|err| {
			map_ms_encoder(
				"opus_multistream_encoder_ctl(OPUS_SET_EXPERT_FRAME_DURATION_REQUEST)",
				err,
			)
		})
	}

	/// Gets the encoder's configured use of variable duration frames.
	pub fn get_expert_frame_duration(&mut self) -> Result<FrameSize> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetExpertFrameDuration(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder(
				"opus_multistream_encoder_ctl(OPUS_GET_EXPERT_FRAME_DURATION_REQUEST)",
				err,
			)
		})?;
		FrameSize::from_raw(
			value,
			"opus_multistream_encoder_ctl(OPUS_GET_EXPERT_FRAME_DURATION_REQUEST)",
		)
	}

	/// If set to true, disables almost all use of prediction.
	pub fn set_prediction_disabled(&mut self, disabled: bool) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetPredictionDisabled(disabled),
		)
		.map_err(|err| {
			map_ms_encoder(
				"opus_multistream_encoder_ctl(OPUS_SET_PREDICTION_DISABLED_REQUEST)",
				err,
			)
		})
	}

	/// Gets the encoder's configured prediction status.
	pub fn get_prediction_disabled(&mut self) -> Result<bool> {
		let mut value = false;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetPredictionDisabled(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder(
				"opus_multistream_encoder_ctl(OPUS_GET_PREDICTION_DISABLED_REQUEST)",
				err,
			)
		})?;
		Ok(value)
	}

	/// Set the DRED duration in milliseconds.
	pub fn set_dred_duration(&mut self, duration_ms: i32) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetDredDuration(duration_ms),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_DRED_DURATION_REQUEST)", err)
		})
	}

	/// Get the configured DRED duration in milliseconds.
	pub fn get_dred_duration(&mut self) -> Result<i32> {
		let mut value = 0;
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::GetDredDuration(&mut value),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_GET_DRED_DURATION_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Set a DNN model blob for the encoder.
	pub fn set_dnn_blob(&mut self, blob: &[u8]) -> Result<()> {
		multistream_api::opus_multistream_encoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamEncoderCtlRequest::SetDnnBlob(blob),
		)
		.map_err(|err| {
			map_ms_encoder("opus_multistream_encoder_ctl(OPUS_SET_DNN_BLOB_REQUEST)", err)
		})
	}
}

/// Decode packets into many Opus streams, up to 255.
#[derive(Debug)]
pub struct MSDecoder {
	inner: multistream_api::OpusMultistreamDecoder<'static>,
	channels: i32,
	sample_rate: u32,
}

unsafe impl Send for MSDecoder {}

impl MSDecoder {
	/// Create and initialize a multistream decoder.
	pub fn new(
		sample_rate: u32,
		streams: u8,
		coupled_streams: u8,
		mapping: &[u8],
	) -> Result<MSDecoder> {
		let inner = multistream_api::opus_multistream_decoder_create(
			sample_rate as i32,
			mapping.len(),
			streams as usize,
			coupled_streams as usize,
			mapping,
		)
		.map_err(|err| map_ms_decoder("opus_multistream_decoder_create", err))?;
		Ok(MSDecoder { inner, channels: len(mapping), sample_rate })
	}

	/// Decode a multistream Opus packet.
	pub fn decode(&mut self, input: &[u8], output: &mut [i16], fec: bool) -> Result<usize> {
		multistream_api::opus_multistream_decode(
			&mut self.inner,
			input,
			input.len(),
			output,
			output.len() / self.channels as usize,
			fec,
		)
		.map_err(|err| map_ms_decoder("opus_multistream_decode", err))
	}

	/// Decode a multistream Opus packet with floating point output.
	pub fn decode_float(&mut self, input: &[u8], output: &mut [f32], fec: bool) -> Result<usize> {
		multistream_api::opus_multistream_decode_float(
			&mut self.inner,
			input,
			input.len(),
			output,
			output.len() / self.channels as usize,
			fec,
		)
		.map_err(|err| map_ms_decoder("opus_multistream_decode_float", err))
	}

	/// Reset the codec state to be equivalent to a freshly initialized state.
	pub fn reset_state(&mut self) -> Result<()> {
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::ResetState,
		)
		.map_err(|err| map_ms_decoder("opus_multistream_decoder_ctl(OPUS_RESET_STATE)", err))
	}

	/// Get the final range of the codec's entropy coder.
	pub fn get_final_range(&mut self) -> Result<u32> {
		let mut value = 0;
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::GetFinalRange(&mut value),
		)
		.map_err(|err| {
			map_ms_decoder("opus_multistream_decoder_ctl(OPUS_GET_FINAL_RANGE_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Get the decoder's configured bandpass.
	pub fn get_bandwidth(&mut self) -> Result<Bandwidth> {
		let mut value = 0;
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::GetBandwidth(&mut value),
		)
		.map_err(|err| {
			map_ms_decoder("opus_multistream_decoder_ctl(OPUS_GET_BANDWIDTH_REQUEST)", err)
		})?;
		Bandwidth::decode(value, "opus_multistream_decoder_ctl(OPUS_GET_BANDWIDTH_REQUEST)")
	}

	/// Get the sample rate the decoder was initialized with.
	pub fn get_sample_rate(&mut self) -> Result<u32> {
		Ok(self.sample_rate)
	}

	/// If set to true, disables the use of phase inversion for intensity stereo.
	pub fn set_phase_inversion_disabled(&mut self, disabled: bool) -> Result<()> {
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::SetPhaseInversionDisabled(disabled),
		)
		.map_err(|err| {
			map_ms_decoder(
				"opus_multistream_decoder_ctl(OPUS_SET_PHASE_INVERSION_DISABLED_REQUEST)",
				err,
			)
		})
	}

	/// Get the decoder's configured phase inversion status.
	pub fn get_phase_inversion_disabled(&mut self) -> Result<bool> {
		let mut value = false;
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::GetPhaseInversionDisabled(
				&mut value,
			),
		)
		.map_err(|err| {
			map_ms_decoder(
				"opus_multistream_decoder_ctl(OPUS_GET_PHASE_INVERSION_DISABLED_REQUEST)",
				err,
			)
		})?;
		Ok(value)
	}

	/// Configures decoder gain adjustment.
	pub fn set_gain(&mut self, gain: i32) -> Result<()> {
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::SetGain(gain),
		)
		.map_err(|err| map_ms_decoder("opus_multistream_decoder_ctl(OPUS_SET_GAIN_REQUEST)", err))
	}

	/// Gets the decoder's configured gain adjustment.
	pub fn get_gain(&mut self) -> Result<i32> {
		let mut value = 0;
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::GetGain(&mut value),
		)
		.map_err(|err| {
			map_ms_decoder("opus_multistream_decoder_ctl(OPUS_GET_GAIN_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Configures the decoder's complexity.
	pub fn set_complexity(&mut self, value: i32) -> Result<()> {
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::SetComplexity(value),
		)
		.map_err(|err| {
			map_ms_decoder("opus_multistream_decoder_ctl(OPUS_SET_COMPLEXITY_REQUEST)", err)
		})
	}

	/// Gets the decoder's complexity configuration.
	pub fn get_complexity(&mut self) -> Result<i32> {
		let mut value = 0;
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::GetComplexity(&mut value),
		)
		.map_err(|err| {
			map_ms_decoder("opus_multistream_decoder_ctl(OPUS_GET_COMPLEXITY_REQUEST)", err)
		})?;
		Ok(value)
	}

	/// Gets the duration of the last packet successfully decoded or concealed.
	pub fn get_last_packet_duration(&mut self) -> Result<u32> {
		let mut value = 0;
		multistream_api::opus_multistream_decoder_ctl(
			&mut self.inner,
			multistream_api::OpusMultistreamDecoderCtlRequest::GetLastPacketDuration(&mut value),
		)
		.map_err(|err| {
			map_ms_decoder(
				"opus_multistream_decoder_ctl(OPUS_GET_LAST_PACKET_DURATION_REQUEST)",
				err,
			)
		})?;
		Ok(value as u32)
	}

	/// Gets the pitch of the last decoded frame, if available.
	pub fn get_pitch(&mut self) -> Result<i32> {
		Err(Error::from_code(
			"opus_multistream_decoder_ctl(OPUS_GET_PITCH_REQUEST)",
			OPUS_UNIMPLEMENTED,
		))
	}
}
