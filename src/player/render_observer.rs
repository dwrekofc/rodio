//! Read-only observation of the Player's existing periodic control application.
use crate::common::{ChannelCount, SampleRate};
use std::time::Duration;

/// Coherent values captured after one periodic control update, before its next sample.
/// Position belongs to the imported TrackPosition after speed/seek application. It is
/// not an absolute input cursor, a complete-frame guarantee, or a device/speaker clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppliedControls {
    /// Position of the wrapped TrackPosition, after any attempted seek.
    pub position: Duration,
    /// Actual Pausable state after applying the requested pause control.
    pub paused: bool,
    /// Exact factor applied to Speed in this update.
    pub speed: f32,
    /// Whether this update applied the requested stop control.
    pub stopped: bool,
    /// Whether this update consumed a queued clear/skip operation.
    pub cleared: bool,
    /// Channels reported by the pipeline at this update.
    pub channels: ChannelCount,
    /// Rate reported after speed application, before downstream conversion.
    pub sample_rate: SampleRate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{buffer::SamplesBuffer, math::nz, Player, Source};
    use std::sync::{Arc, Mutex};

    #[test]
    fn observer_keeps_samples_metadata_pause_speed_and_source_lifetime_exact() {
        for channels in [1, 2, 3] {
            for rate in [8000, 44100, 48000] {
                for speed in [0.5, 1., 1.25, 2.] {
                    let samples: Vec<_> = (0..rate / 5 * channels)
                        .map(|n| (n % 101) as crate::Sample / 101. - 0.5)
                        .collect();
                    let source = SamplesBuffer::new(
                        std::num::NonZero::new(channels as u16).unwrap(),
                        std::num::NonZero::new(rate).unwrap(),
                        samples,
                    );
                    let (plain, mut a) = Player::new();
                    let (observed, mut b) = Player::new();
                    let events = Arc::new(Mutex::new(Vec::new()));
                    let capture = events.clone();
                    plain.append(source.clone());
                    observed.append_with_render_callback(source, move |e| {
                        capture.lock().unwrap().push(e)
                    });
                    plain.set_speed(speed);
                    observed.set_speed(speed);
                    for n in 0..rate / 8 * channels {
                        if n == 100 {
                            plain.pause();
                            observed.pause();
                        }
                        if n == 300 {
                            plain.play();
                            observed.play();
                        }
                        if n == 900 {
                            plain.set_speed(0.75);
                            observed.set_speed(0.75);
                        }
                        assert_eq!(
                            (a.channels(), a.sample_rate(), a.current_span_len()),
                            (b.channels(), b.sample_rate(), b.current_span_len())
                        );
                        assert_eq!(a.next(), b.next());
                        assert_eq!(plain.get_pos(), observed.get_pos());
                    }
                    let events = events.lock().unwrap();
                    assert!(!events.is_empty());
                    assert_eq!(events[0].speed, speed);
                    assert_eq!(events[0].position, Duration::ZERO);
                    assert!(events
                        .iter()
                        .all(|e| e.channels.get() == channels as u16 && !e.stopped && !e.cleared));
                }
            }
        }
    }

    #[test]
    fn callback_reports_applied_pause_and_drops_with_the_appended_source() {
        let (player, mut output) = Player::new();
        player.pause();
        let events = Arc::new(Mutex::new(Vec::new()));
        let lifetime = Arc::new(());
        let capture = events.clone();
        let retained = lifetime.clone();
        player.append_with_render_callback(
            SamplesBuffer::new(nz!(2), nz!(48000), vec![0.25; 48000]),
            move |e| {
                let _ = &retained;
                capture.lock().unwrap().push(e);
            },
        );
        assert_eq!(Arc::strong_count(&lifetime), 2);
        assert_eq!(output.next(), Some(0.));
        assert!(events.lock().unwrap()[0].paused);
        player.play();
        // Requested controls are independent until the next existing periodic callback.
        assert!(events.lock().unwrap().last().unwrap().paused);
        for _ in 0..1000 {
            output.next();
        }
        assert!(!events.lock().unwrap().last().unwrap().paused);
        player.stop();
        for _ in 0..1000 {
            output.next();
        }
        assert!(events.lock().unwrap().last().unwrap().stopped);
        drop(output);
        assert_eq!(Arc::strong_count(&lifetime), 1);
    }
    #[test]
    fn observer_reports_seek_and_clear_after_the_same_existing_control_application() {
        let (player, mut output) = Player::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let capture = events.clone();
        player.append_with_render_callback(
            SamplesBuffer::new(nz!(2), nz!(48000), vec![0.25; 48000]),
            move |e| capture.lock().unwrap().push(e),
        );
        let requested = Duration::from_millis(100);
        let (order, feedback) = super::super::SeekOrder::new(requested);
        *player.controls.seek.lock().unwrap() = Some(order);
        output.next();
        assert!(feedback.recv().unwrap().is_ok());
        assert_eq!(events.lock().unwrap()[0].position, requested);
        *player.controls.to_clear.lock().unwrap() = 1;
        for _ in 0..1000 {
            output.next();
        }
        assert!(events.lock().unwrap().last().unwrap().cleared);
    }
}
