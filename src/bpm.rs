pub fn bpm_detect(audio_data: Vec<u16>) -> usize {
    let sample_rate = 44100;
    let window_size = sample_rate / 10; // 100msごとに解析
    let mut energy_values = Vec::new();

    // 1. 短時間エネルギー計算
    for chunk in audio_data.chunks(window_size) {
        let energy: f64 = chunk.iter().map(|&s| {
            let sample = s as i32 - 32768; // 16bit の符号なしを符号付きに変換
            (sample * sample) as f64
        }).sum();
        energy_values.push(energy);
    }

    // 2. エネルギー変化の微分
    let mut diffs = Vec::new();
    for i in 1..energy_values.len() {
        diffs.push(energy_values[i] - energy_values[i - 1]);
    }

    // 3. ピーク検出（単純な閾値処理）
    let threshold = diffs.iter().cloned().fold(f64::NAN, f64::max) * 0.6;
    let mut peak_times = Vec::new();
    for (i, &diff) in diffs.iter().enumerate() {
        if diff > threshold {
            peak_times.push(i as f64 * (window_size as f64 / sample_rate as f64));
        }
    }

    // 4. BPM 推定
    if peak_times.len() < 2 {
        return 0; // ビートが検出できなかった場合
    }

    let mut intervals = Vec::new();
    for w in peak_times.windows(2) {
        intervals.push(w[1] - w[0]);
    }

    let avg_interval = intervals.iter().sum::<f64>() / intervals.len() as f64;
    let bpm = (60.0 / avg_interval) as usize;

    bpm
}
