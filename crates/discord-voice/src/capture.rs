//! Pace callback batches into 20 ms packets without discarding normal speech.
use crate::CaptureFrame;
use std::sync::mpsc::Receiver;

#[derive(Default)]
pub(crate) struct CapturePacer {
	// One frame of lookahead absorbs callback/worker jitter. The input channel
	// remains capped at eight frames: at most nine 96 kHz frames / 138,240 PCM bytes total.
	pending: Option<CaptureFrame>,
}

impl CapturePacer {
	/// Local detection while alone: consume audio without retaining it for transmission.
	pub fn preview(&mut self, input: &Receiver<CaptureFrame>) -> Option<CaptureFrame> {
		self.pending = None;
		let mut latest = None;
		for _ in 0..8 {
			let Ok(frame) = input.try_recv() else {
				break;
			};
			latest = Some(frame);
		}
		latest
	}

	pub fn next(
		&mut self,
		input: &Receiver<CaptureFrame>,
		enabled: bool,
		stalled: bool,
	) -> Option<CaptureFrame> {
		if !enabled || stalled {
			self.pending = None;
			for _ in 0..8 {
				if input.try_recv().is_err() {
					break;
				}
			}
			return None;
		}
		let frame = self.pending.take();
		self.pending = input.try_recv().ok();
		frame
	}
}
