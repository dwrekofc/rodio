use crate::{
    common::{ChannelCount, SampleRate},
    Sample,
};
use cpal::{FromSample, OutputStreamTimestamp};

/// Notifications from the actual output callback, before conversion into the device buffer.
/// Timestamps are the host's callback and predicted playback instants, not acoustic capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputBufferEvent {
    /// Before any source sample is pulled for this callback buffer.
    Begin {
        /// Number of output sample slots, including any partial final frame.
        samples: usize,
        /// Configured interleaved channel count.
        channels: ChannelCount,
        /// Configured device sample rate.
        sample_rate: SampleRate,
        /// Host callback time and predicted playback time, forwarded without conversion.
        timestamp: OutputStreamTimestamp,
    },
    /// Immediately before pulling the first sample of this interleaved output frame.
    Frame {
        /// Zero-based frame offset in this callback buffer.
        offset: usize,
    },
    /// After every output slot has been filled, including an empty buffer.
    End,
}

/// Runs on the output thread. Implementations must not block, panic or render recursively.
pub type OutputBufferCallback = dyn Fn(OutputBufferEvent) + Send + Sync + 'static;

pub(super) fn fill_buffer<S, T>(
    data: &mut [T],
    samples: &mut S,
    channels: ChannelCount,
    sample_rate: SampleRate,
    timestamp: OutputStreamTimestamp,
    callback: Option<&OutputBufferCallback>,
) where
    S: Iterator<Item = Sample>,
    T: cpal::Sample + FromSample<Sample>,
{
    let Some(callback) = callback else {
        data.iter_mut().for_each(|d| {
            *d = samples
                .next()
                .map(cpal::Sample::from_sample)
                .unwrap_or(T::EQUILIBRIUM)
        });
        return;
    };
    callback(OutputBufferEvent::Begin {
        samples: data.len(),
        channels,
        sample_rate,
        timestamp,
    });
    for (offset, frame) in data.chunks_mut(usize::from(channels.get())).enumerate() {
        callback(OutputBufferEvent::Frame { offset });
        frame.iter_mut().for_each(|d| {
            *d = samples
                .next()
                .map(cpal::Sample::from_sample)
                .unwrap_or(T::EQUILIBRIUM)
        });
    }
    callback(OutputBufferEvent::End);
}

#[cfg(test)]
mod tests;
