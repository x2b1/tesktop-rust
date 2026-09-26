//! Display-only activity; never gates or changes transmitted audio.
// ponytail: a -45 dBFS level threshold detects sound, not speech; use VAD if noise lights it up.
pub(crate) fn hold(energy: f32, previous: u8) -> u8 {
	if energy.is_finite() && energy > 960.0 * 0.000_031_623 {
		10 // 200 ms at the transport's 20 ms cadence.
	} else {
		previous.saturating_sub(1)
	}
}

/// Finite RMS level for the local preview meter, clamped to its display range.
pub(crate) fn level_db(frame: &[f32; 960]) -> f32 {
	let energy: f32 = frame
		.iter()
		.filter(|s| s.is_finite())
		.map(|s| s.clamp(-1.0, 1.0).powi(2))
		.sum();
	(10.0 * (energy / 960.0).max(1e-10).log10()).clamp(-100.0, 0.0)
}

pub(crate) fn hold_at(energy: f32, previous: u8, threshold: i16) -> u8 {
	if energy.is_finite()
		&& energy > 960.0 * 10.0_f32.powf(f32::from(threshold.clamp(-80, 0)) / 10.0)
	{
		10
	} else {
		previous.saturating_sub(1)
	}
}
