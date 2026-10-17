use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

fn conversion<T>()
where
    T: cpal::Sample + FromSample<Sample> + PartialEq + std::fmt::Debug,
{
    let timestamp = OutputStreamTimestamp {
        callback: cpal::StreamInstant::new(12, 345),
        playback: cpal::StreamInstant::new(12, 1345),
    };
    for channels in [1u16, 2, 3] {
        for length in [0, 1, 2, 3, 5, 6, 44, 128] {
            for available in [0, 1, 3, 7] {
                let values: Vec<Sample> =
                    [-1., -0.5, -0., 0., 0.25, 0.75, 1.][..available].to_vec();
                let mut reference = values.clone().into_iter();
                let expected = (0..length)
                    .map(|_| {
                        reference
                            .next()
                            .map(cpal::Sample::from_sample)
                            .unwrap_or(T::EQUILIBRIUM)
                    })
                    .collect::<Vec<T>>();
                for enabled in [false, true] {
                    let calls = Arc::new(AtomicUsize::new(0));
                    let pulls = calls.clone();
                    let mut values = values.clone().into_iter();
                    let mut source = std::iter::from_fn(move || {
                        pulls.fetch_add(1, Ordering::Relaxed);
                        values.next()
                    });
                    let rows = Arc::new(Mutex::new(Vec::new()));
                    let captured = rows.clone();
                    let read = calls.clone();
                    let callback = move |event| {
                        captured
                            .lock()
                            .unwrap()
                            .push((event, read.load(Ordering::Relaxed)))
                    };
                    let mut actual = vec![T::EQUILIBRIUM; length];
                    fill_buffer(
                        &mut actual,
                        &mut source,
                        channels.try_into().unwrap(),
                        48000u32.try_into().unwrap(),
                        timestamp,
                        if enabled { Some(&callback) } else { None },
                    );
                    assert_eq!(actual, expected);
                    assert_eq!(calls.load(Ordering::Relaxed), length);
                    let rows = rows.lock().unwrap();
                    if enabled {
                        let mut expected = vec![(
                            OutputBufferEvent::Begin {
                                samples: length,
                                channels: channels.try_into().unwrap(),
                                sample_rate: 48000u32.try_into().unwrap(),
                                timestamp,
                            },
                            0,
                        )];
                        for offset in 0..length.div_ceil(channels as usize) {
                            expected.push((
                                OutputBufferEvent::Frame { offset },
                                offset * channels as usize,
                            ));
                        }
                        expected.push((OutputBufferEvent::End, length));
                        assert_eq!(*rows, expected);
                    } else {
                        assert!(rows.is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn observers_preserve_all_native_format_conversions_pulls_and_event_order() {
    conversion::<f32>();
    conversion::<f64>();
    conversion::<i8>();
    conversion::<i16>();
    conversion::<cpal::I24>();
    conversion::<i32>();
    conversion::<i64>();
    conversion::<u8>();
    conversion::<u16>();
    conversion::<cpal::U24>();
    conversion::<u32>();
    conversion::<u64>();
}

#[test]
fn identity_float_output_preserves_bits_and_builder_changes_retain_the_callback() {
    let timestamp = OutputStreamTimestamp {
        callback: cpal::StreamInstant::new(1, 0),
        playback: cpal::StreamInstant::new(1, 1),
    };
    let input = [-0., Sample::NAN, Sample::INFINITY, Sample::NEG_INFINITY];
    let mut actual = [0.; 4];
    fill_buffer(
        &mut actual,
        &mut input.into_iter(),
        2.try_into().unwrap(),
        48000.try_into().unwrap(),
        timestamp,
        Some(&|_| {}),
    );
    assert_eq!(actual.map(Sample::to_bits), input.map(Sample::to_bits));
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let builder = super::super::DeviceSinkBuilder::default()
        .with_output_callback(move |_| {
            seen.fetch_add(1, Ordering::Relaxed);
        })
        .with_error_callback(|_| {})
        .with_channels(2.try_into().unwrap())
        .with_sample_rate(48000.try_into().unwrap());
    builder.output_callback.unwrap()(OutputBufferEvent::End);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}
