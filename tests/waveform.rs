use fish_s2pro_tts::timeline::TimelineClip;

#[test]
fn waveform_handles_full_negative_amplitude() {
    assert_eq!(TimelineClip::calculate_waveform_peaks(&[i16::MIN], 1, 1), vec![1.0]);
}

#[test]
fn waveform_includes_right_channel_and_tail() {
    let stereo = [0, 0, 0, 0, 0, 16384];
    assert_eq!(TimelineClip::calculate_waveform_peaks(&stereo, 2, 2), vec![0.0, 0.5]);
}

#[test]
fn waveform_spans_short_clips_without_inventing_silence() {
    assert_eq!(TimelineClip::calculate_waveform_peaks(&[16384], 1, 4), vec![0.5; 4]);
    assert_eq!(TimelineClip::calculate_waveform_peaks(&[0; 8], 1, 4), vec![0.0; 4]);
    assert_eq!(TimelineClip::calculate_waveform_peaks(&[], 1, 4), vec![0.0; 4]);
    assert!(TimelineClip::calculate_waveform_peaks(&[1], 1, 0).is_empty());
}

#[test]
fn waveform_respects_trim_and_actual_sample_rate() {
    let pcm = [i16::MAX, 16384, 8192, i16::MIN];
    let peaks = TimelineClip::calculate_waveform_peaks_sliced(&pcm, 1, 0.001, 0.001, 1000, 2);
    assert_eq!(peaks, vec![0.5, 0.25]);
}
