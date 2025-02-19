mod bpm;

use std::collections::VecDeque;
use std::error;
use std::sync::mpsc;
use std::thread;
use wasapi::*;

#[macro_use]
extern crate log;
use simplelog::*;
use crate::bpm::bpm_detect;

type Res<T> = Result<T, Box<dyn error::Error>>;

static STORE_BITS: usize = 16;
static SAMPLE_RATE: usize = 44100;

// デスクトップ音声をキャプチャするループ、tx_captにchunksize分の音声データを送信する。
fn capture_loop(tx_capt: std::sync::mpsc::SyncSender<Vec<u8>>, chunksize: usize) -> Res<()> {
    // `Direction::Capture`はマイク入力のために,
    // `Direction::Render`はスピーカー出力のループバック
    let device = get_default_device(&Direction::Render)?;

    let mut audio_client = device.get_iaudioclient()?;

    let desired_format = WaveFormat::new(
        STORE_BITS,
        STORE_BITS,
        &SampleType::Int,
        SAMPLE_RATE,
        1,
        None
    );

    let blockalign = desired_format.get_blockalign();
    debug!("Desired capture format: {:?}", desired_format);

    let (def_time, min_time) = audio_client.get_periods()?;
    debug!("default period {}, min period {}", def_time, min_time);

    audio_client.initialize_client(
        &desired_format,
        min_time,
        &Direction::Capture,
        &ShareMode::Shared,
        true,
    )?;
    debug!("initialized capture");

    let h_event = audio_client.set_get_eventhandle()?;

    let buffer_frame_count = audio_client.get_bufferframecount()?;

    let render_client = audio_client.get_audiocaptureclient()?;
    let mut sample_queue: VecDeque<u8> = VecDeque::with_capacity(
        100 * blockalign as usize * (1024 + 2 * buffer_frame_count as usize),
    );
    let session_control = audio_client.get_audiosessioncontrol()?;

    debug!("state before start: {:?}", session_control.get_state());
    audio_client.start_stream()?;
    debug!("state after start: {:?}", session_control.get_state());

    // キャプチャするためのループ
    loop {
        // デバイスからデータが読み込みされたらチャンクに分割し`tx_capt`に送信する。
        while sample_queue.len() > (blockalign as usize * chunksize) {
            let mut chunk = vec![0u8; blockalign as usize * chunksize];
            for element in chunk.iter_mut() {
                *element = sample_queue.pop_front().unwrap();
            }
            tx_capt.send(chunk)?;
        }
        trace!("capturing");

        // デバイスから音声データを読み込む
        render_client.read_from_device_to_deque(&mut sample_queue)?;

        // デバイスから音声データを読込されるまでループ。
        loop {
            // イベントが発火されるまで待つ。
            if h_event.wait_for_event(3000).is_err() {
                // 音声データが無い場合はタイムアウトエラー、もしくはNoneのエラーを返す、
                // エラー後にも音声がデバイスに入ってくる可能性があるため、エラー処理をせずに繰り返しキャプチャし続ける。
                error!(
                    "timeout error {:?} but keep capturing...",
                    h_event.wait_for_event(3000).err()
                );
            } else {
                // 音声データがデバイスから正常に読み込みされたらループを終了させ、音声を送信するフェーズに戻す。
                break;
            }
        }
    }
}

fn convert_u8_to_u16(data: Vec<u8>) -> Vec<u16> {
    data.chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]])) // Little Endianの場合
        .collect()
}

// Main loop
fn main() -> Res<()> {
    // ログングの初期化
    let _ = SimpleLogger::init(
        LevelFilter::Info,
        ConfigBuilder::new()
            .set_time_format_rfc3339()
            .set_time_offset_to_local()
            .unwrap()
            .build(),
    );

    initialize_mta().ok()?;

    let (tx_capt, rx_capt): (
        std::sync::mpsc::SyncSender<Vec<u8>>,
        std::sync::mpsc::Receiver<Vec<u8>>,
    ) = mpsc::sync_channel(2);

    // Capture
    let _handle = thread::Builder::new()
        .name("Capture".to_string())
        .spawn(move || {
            let result = capture_loop(tx_capt, SAMPLE_RATE * 5);
            if let Err(err) = result {
                error!("Capture failed with error {}", err);
            }
        });


    loop {
        match rx_capt.recv() {
            Ok(chunk) => {
                let mut bpm = bpm_detect(convert_u8_to_u16(chunk));

                while bpm < 60 {
                    bpm *= 2;
                }

                while bpm > 180 {
                    bpm /= 2;
                }


                info!("bpm: {}", bpm);
            }
            Err(err) => {
                error!("Some error {}", err);
                return Ok(());
            }
        }
    }
}
