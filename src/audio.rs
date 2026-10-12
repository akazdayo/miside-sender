use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use tokio::sync::{mpsc, watch};

pub fn list_devices() -> Result<()> {
    for device in cpal::default_host().input_devices()? {
        println!("{}", device.name()?);
    }
    Ok(())
}

pub struct Microphone {
    stream: cpal::Stream,
    pub frames: mpsc::Receiver<Vec<f32>>,
    pub errors: watch::Receiver<Option<String>>,
    pub converter: PcmConverter,
}

impl Microphone {
    pub fn open(name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = if let Some(name) = name {
            host.input_devices()?
                .find(|device| device.name().is_ok_and(|value| value == name))
                .with_context(|| {
                    format!("入力デバイスが見つかりません: {name} (--list-devicesで確認)")
                })?
        } else {
            host.default_input_device()
                .context("デフォルト入力デバイスがありません")?
        };
        let config = device
            .default_input_config()
            .context("入力設定を取得できません")?;
        let rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let (tx, frames) = mpsc::channel(20);
        let (errors_tx, errors) = watch::channel(None);
        let converter = PcmConverter::new(rate)?;
        let stream_config = config.config();
        macro_rules! build {
            ($sample:ty) => {
                build_stream::<$sample>(
                    &device,
                    &stream_config,
                    tx,
                    errors_tx,
                    channels,
                    converter.chunk_size,
                )
            };
        }
        let stream = match config.sample_format() {
            SampleFormat::I8 => build!(i8),
            SampleFormat::I16 => build!(i16),
            SampleFormat::I32 => build!(i32),
            SampleFormat::I64 => build!(i64),
            SampleFormat::U8 => build!(u8),
            SampleFormat::U16 => build!(u16),
            SampleFormat::U32 => build!(u32),
            SampleFormat::U64 => build!(u64),
            SampleFormat::F32 => build!(f32),
            SampleFormat::F64 => build!(f64),
            format => bail!("未対応の入力音声形式: {format}"),
        }?;
        eprintln!("入力: {} ({rate} Hz, {channels} ch)", device.name()?);
        Ok(Self {
            stream,
            frames,
            errors,
            converter,
        })
    }

    pub fn start(&self) -> Result<()> {
        self.stream.play().context("マイク録音を開始できません")
    }
}

fn mono<T: Sample>(frame: &[T]) -> f32
where
    f32: FromSample<T>,
{
    frame
        .iter()
        .map(|sample| sample.to_sample::<f32>())
        .sum::<f32>()
        / frame.len() as f32
}

fn build_stream<T: SizedSample>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    tx: mpsc::Sender<Vec<f32>>,
    errors: watch::Sender<Option<String>>,
    channels: usize,
    chunk_size: usize,
) -> Result<cpal::Stream>
where
    f32: FromSample<T>,
{
    let callback_errors = errors.clone();
    let mut buffer = Vec::with_capacity(chunk_size);
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _| {
            // Never block the audio callback on network or OSC throughput.
            for frame in data.chunks_exact(channels) {
                buffer.push(mono(frame));
                if buffer.len() == chunk_size {
                    let samples = std::mem::replace(&mut buffer, Vec::with_capacity(chunk_size));
                    if tx.try_send(samples).is_err() {
                        callback_errors
                            .send_replace(Some("音声の送信が追いつかないため停止しました".into()));
                        return;
                    }
                }
            }
        },
        move |error| {
            errors.send_replace(Some(format!("録音エラー: {error}")));
        },
        None,
    )?)
}

pub struct PcmConverter {
    chunk_size: usize,
    resampler: Option<SincFixedIn<f32>>,
}

impl PcmConverter {
    pub fn new(rate: u32) -> Result<Self> {
        anyhow::ensure!(rate >= 10, "不正な入力サンプルレート");
        let chunk_size = rate as usize / 10;
        let resampler = if rate == 16000 {
            None
        } else {
            Some(SincFixedIn::new(
                16000.0 / rate as f64,
                1.0,
                SincInterpolationParameters {
                    sinc_len: 256,
                    f_cutoff: 0.95,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 128,
                    window: WindowFunction::BlackmanHarris2,
                },
                chunk_size,
                1,
            )?)
        };
        Ok(Self {
            chunk_size,
            resampler,
        })
    }

    pub fn convert(&mut self, samples: Vec<f32>) -> Result<Vec<u8>> {
        anyhow::ensure!(samples.len() == self.chunk_size, "不正な音声チャンク長");
        let samples = if let Some(resampler) = &mut self.resampler {
            resampler.process(&[samples], None)?.remove(0)
        } else {
            samples
        };
        Ok(samples
            .into_iter()
            .flat_map(|value| {
                let sample = (value.clamp(-1.0, 1.0) * 32768.0)
                    .round()
                    .clamp(-32768.0, 32767.0) as i16;
                sample.to_le_bytes()
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_stereo_and_encodes_signed_little_endian() {
        assert_eq!(mono(&[i16::MIN, i16::MIN]), -1.0);
        assert_eq!(mono(&[-0.5_f32, 0.5]), 0.0);
        let mut converter = PcmConverter::new(16000).unwrap();
        let mut samples = vec![0.0; 1600];
        samples[..5].copy_from_slice(&[-1.0, -0.5, 0.0, 0.5, 1.0]);
        let pcm = converter.convert(samples).unwrap();
        assert_eq!(&pcm[..10], &[0, 128, 0, 192, 0, 0, 0, 64, 255, 127]);
        assert_eq!(pcm.len(), 3200);
    }

    #[test]
    fn resamples_continuously_at_common_device_rates() {
        for rate in [44100, 48000, 96000] {
            let mut converter = PcmConverter::new(rate).unwrap();
            let mut output = Vec::new();
            for _ in 0..10 {
                output.extend(converter.convert(vec![0.5; rate as usize / 10]).unwrap());
            }
            // Sinc resampling introduces a short initial filter delay.
            assert!(
                (31000..=32000).contains(&output.len()),
                "{rate}: {}",
                output.len()
            );
            let tail = &output[output.len() - 200..];
            assert!(tail.as_chunks::<2>().0.iter().all(|sample| {
                (i16::from_le_bytes([sample[0], sample[1]]) as i32 - 16384).abs() < 20
            }));
        }
    }

    #[tokio::test]
    #[ignore = "既定マイクを約100ms録音するローカル検証。外部送信なし"]
    async fn local_microphone_capture() -> Result<()> {
        let mut microphone = Microphone::open(None)?;
        microphone.start()?;
        let samples =
            tokio::time::timeout(std::time::Duration::from_secs(5), microphone.frames.recv())
                .await?
                .context("マイク入力が終了しました")?;
        let pcm = microphone.converter.convert(samples)?;
        anyhow::ensure!(!pcm.is_empty() && pcm.len() % 2 == 0, "PCMが不正です");
        Ok(())
    }
}
