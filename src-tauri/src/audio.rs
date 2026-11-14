//! Audio engine: streaming decode (symphonia), output (rodio/cpal) and the
//! player controller with its play queue.
//!
//! * [`ScSource`] decodes a [`StreamReader`] on the fly and implements
//!   `rodio::Source`. Seeks are requested through [`SourceCtl`] and executed
//!   on the audio thread.
//! * The output thread owns the (non-`Send`) `OutputStream` and receives
//!   [`AudioCmd`]s over a channel.
//! * [`Player`] is the async-facing controller used by Tauri commands, the
//!   tray and MPRIS. It owns the queue and emits `player://state` events.

use crate::api::ApiClient;
use crate::config::{self, RepeatMode, Settings};
use crate::models::Track;
use crate::mpris::{MprisHandle, MprisUpdate};
use crate::stream::{self, StreamBuffer, StreamReader};
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use rodio::cpal::traits::HostTrait;
use rodio::DeviceTrait;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase};

// ----------------------------------------------------------------------
// Decoder source
// ----------------------------------------------------------------------

/// Shared control block between the controller and a playing source.
#[derive(Default)]
pub struct SourceCtl {
    pending_seek: Mutex<Option<f64>>,
    position_us: AtomicU64,
    finished: AtomicBool,
    stop: AtomicBool,
}

impl SourceCtl {
    pub fn position(&self) -> f64 {
        self.position_us.load(Ordering::Relaxed) as f64 / 1e6
    }
    fn set_position(&self, secs: f64) {
        self.position_us.store((secs.max(0.0) * 1e6) as u64, Ordering::Relaxed);
    }
    pub fn request_seek(&self, secs: f64) {
        *self.pending_seek.lock() = Some(secs.max(0.0));
        self.position_us.store((secs.max(0.0) * 1e6) as u64, Ordering::Relaxed);
    }
}

pub struct ScSource {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    sample_buf: Option<SampleBuffer<f32>>,
    buf: Vec<f32>,
    idx: usize,
    channels: u16,
    rate: u32,
    buf_start: f64,
    /// Drop decoded audio before this time (precise start after a reopen).
    skip_until: Option<f64>,
    /// Added to packet timestamps (fMP4 fragments may restart at zero).
    ts_offset: f64,
    /// Expected timestamp of the first decoded buffer.
    ts_floor: Option<f64>,
    ctl: Arc<SourceCtl>,
}

/// Open a decoder for `buffer`, starting at `start` seconds.
///
/// MP3 streams are opened seekable and seek in place. fMP4 (HLS AAC) is
/// demuxed forward-only so playback can begin before the download ends; to
/// start mid-track we open a reader positioned at the right HLS segment.
pub fn open_source(buffer: &StreamBuffer, hint: &str, ctl: Arc<SourceCtl>, start: f64) -> Result<ScSource> {
    if hint == "mp4" {
        if start > 0.5 {
            let (reader, seg_start) = buffer.reader_at(start).ok_or_else(|| anyhow!("загрузка прервана"))?;
            ScSource::new(reader, hint, ctl, Some(start), Some(seg_start))
        } else {
            ScSource::new(buffer.streaming_reader(), hint, ctl, None, None)
        }
    } else {
        let src = ScSource::new(buffer.reader(), hint, ctl.clone(), None, None)?;
        if start > 0.5 {
            ctl.request_seek(start);
        }
        Ok(src)
    }
}

impl ScSource {
    /// Probe the stream and decode the first packet. Blocks until enough
    /// data has been downloaded — call from a blocking thread.
    pub fn new(
        reader: StreamReader,
        hint_ext: &str,
        ctl: Arc<SourceCtl>,
        skip_until: Option<f64>,
        ts_floor: Option<f64>,
    ) -> Result<Self> {
        let mss = MediaSourceStream::new(Box::new(reader), MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        hint.with_extension(hint_ext);
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
            .map_err(|e| anyhow!("не удалось распознать аудиопоток: {e}"))?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow!("в потоке нет аудиодорожки"))?;
        let track_id = track.id;
        let time_base = track.codec_params.time_base;
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| anyhow!("неподдерживаемый кодек: {e}"))?;
        let channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);
        let rate = track.codec_params.sample_rate.unwrap_or(44_100);
        let mut src = ScSource {
            format,
            decoder,
            track_id,
            time_base,
            sample_buf: None,
            buf: Vec::new(),
            idx: 0,
            channels,
            rate,
            buf_start: ts_floor.unwrap_or(0.0),
            skip_until,
            ts_offset: 0.0,
            ts_floor,
            ctl,
        };
        if !src.refill() {
            return Err(anyhow!("не удалось декодировать начало трека"));
        }
        Ok(src)
    }

    fn refill(&mut self) -> bool {
        loop {
            if self.ctl.stop.load(Ordering::Relaxed) {
                return false;
            }
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(_) => return false, // EOF or fatal I/O error
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let frames = decoded.frames();
                    if frames == 0 {
                        continue;
                    }
                    let need = decoded.capacity() as u64;
                    let sb = match &mut self.sample_buf {
                        Some(sb) if sb.capacity() as u64 >= need * spec.channels.count() as u64 => sb,
                        _ => self.sample_buf.insert(SampleBuffer::<f32>::new(need, spec)),
                    };
                    sb.copy_interleaved_ref(decoded);
                    self.buf.clear();
                    self.buf.extend_from_slice(sb.samples());
                    self.idx = 0;
                    self.channels = spec.channels.count() as u16;
                    self.rate = spec.rate;
                    let raw = match self.time_base {
                        Some(tb) => {
                            let t = tb.calc_time(packet.ts());
                            t.seconds as f64 + t.frac
                        }
                        None => self.buf_start + frames as f64 / self.rate as f64,
                    };
                    if let Some(floor) = self.ts_floor.take() {
                        // Fragment timestamps restarted from zero: re-base them.
                        if raw + 1.0 < floor {
                            self.ts_offset = floor - raw;
                        }
                    }
                    let start = raw + self.ts_offset;
                    self.buf_start = start;
                    if let Some(target) = self.skip_until {
                        let end = start + frames as f64 / self.rate.max(1) as f64;
                        if end <= target {
                            continue;
                        }
                        let drop_frames = ((target - start).max(0.0) * self.rate as f64) as usize;
                        let n = (drop_frames * self.channels as usize).min(self.buf.len());
                        self.buf.drain(..n);
                        self.buf_start = start.max(target);
                        self.skip_until = None;
                    }
                    self.publish_position();
                    if !self.buf.is_empty() {
                        return true;
                    }
                }
                Err(SymError::DecodeError(e)) => {
                    log::debug!("decode error (skipping packet): {e}");
                    continue;
                }
                Err(SymError::IoError(_)) => return false,
                Err(e) => {
                    log::warn!("fatal decode error: {e}");
                    return false;
                }
            }
        }
    }

    fn publish_position(&self) {
        let frames_in = (self.idx / self.channels.max(1) as usize) as f64;
        let pos = self.buf_start + frames_in / self.rate.max(1) as f64;
        self.ctl.position_us.store((pos * 1e6) as u64, Ordering::Relaxed);
    }

    fn do_seek(&mut self, secs: f64) {
        let to = SeekTo::Time { time: Time::from(secs), track_id: Some(self.track_id) };
        match self.format.seek(SeekMode::Accurate, to) {
            Ok(_) => {}
            Err(e) => log::warn!("seek failed: {e}"),
        }
        self.decoder.reset();
        self.buf.clear();
        self.idx = 0;
        self.buf_start = secs;
        // The demuxer lands on the packet containing `secs`; trim the rest.
        self.skip_until = Some(secs);
        self.ctl.position_us.store((secs * 1e6) as u64, Ordering::Relaxed);
    }
}

impl Iterator for ScSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.ctl.stop.load(Ordering::Relaxed) {
            return None;
        }
        let seek = self.ctl.pending_seek.lock().take();
        if let Some(secs) = seek {
            self.do_seek(secs);
        }
        if self.idx >= self.buf.len() && !self.refill() {
            self.ctl.finished.store(true, Ordering::Relaxed);
            return None;
        }
        let s = self.buf[self.idx];
        self.idx += 1;
        if self.idx.is_multiple_of(4096) {
            self.publish_position();
        }
        Some(s)
    }
}

impl rodio::Source for ScSource {
    fn current_frame_len(&self) -> Option<usize> {
        Some((self.buf.len() - self.idx.min(self.buf.len())).max(1))
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

// ----------------------------------------------------------------------
// Output thread
// ----------------------------------------------------------------------

enum AudioCmd {
    Load { source: ScSource, gen: u64, paused: bool },
    Play,
    Pause,
    Stop,
    SetVolume(f32),
    SetDevice(Option<String>),
    /// Tests: the (null) device stops pulling samples, and the next
    /// `fail_reopens` attempts to open an output fail.
    #[cfg(test)]
    SimulateDeviceLoss { fail_reopens: u32 },
}

enum AudioEvent {
    Ended(u64),
    /// The output was re-created: re-attach the current track to it.
    DeviceReset(u64),
    /// Device vanished; playback is suspended while we reconnect.
    DeviceLost(String),
    /// Output is back (possibly on another device).
    DeviceRestored(String),
    Error(String),
}

/// A device that hasn't pulled samples for this long is considered gone.
const STALL_TIMEOUT: Duration = if cfg!(test) { Duration::from_millis(500) } else { Duration::from_secs(2) };
/// Delay between attempts to reopen an output.
const REOPEN_INTERVAL: Duration = if cfg!(test) { Duration::from_millis(300) } else { Duration::from_secs(2) };
/// How often to look for the preferred device while running on a fallback.
const PREFERRED_PROBE_INTERVAL: Duration = Duration::from_secs(5);

fn find_exact_device(name: &str) -> Option<rodio::Device> {
    rodio::cpal::default_host()
        .output_devices()
        .ok()?
        .find(|d| d.name().map(|dn| dn == name).unwrap_or(false))
}

pub fn list_output_devices() -> Vec<String> {
    let host = rodio::cpal::default_host();
    host.output_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Endless silence mixed into the output. The mixer only pulls it while the
/// device is alive, so a stale timestamp means the device is gone — rodio
/// does not expose cpal's stream-error callback.
struct Heartbeat {
    beat: Arc<AtomicU64>,
    epoch: std::time::Instant,
    n: u32,
}

impl Iterator for Heartbeat {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.n = self.n.wrapping_add(1);
        if self.n.is_multiple_of(512) {
            self.beat.store(self.epoch.elapsed().as_millis() as u64, Ordering::Relaxed);
        }
        Some(0.0)
    }
}

impl rodio::Source for Heartbeat {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        44_100
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Where decoded audio goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBackend {
    /// The system sound device (cpal → PipeWire/PulseAudio/ALSA).
    Device,
    /// No device: a mixer drained in real time and discarded (tests,
    /// headless machines).
    Null,
}

enum OutputKind {
    Device { _stream: rodio::OutputStream, handle: rodio::OutputStreamHandle },
    Null { mixer: Arc<rodio::dynamic_mixer::DynamicMixerController<f32>>, alive: Arc<AtomicBool> },
}

/// An open output plus its liveness heartbeat.
struct Output {
    kind: OutputKind,
    /// Human-readable device name (for notices).
    label: String,
    /// Opened on the user's chosen device (or no preference was set).
    on_preferred: bool,
    beat: Arc<AtomicU64>,
    epoch: std::time::Instant,
}

impl Output {
    fn open(
        backend: AudioBackend,
        preferred: Option<&str>,
        epoch: std::time::Instant,
        fail_budget: &mut u32,
    ) -> Result<Output> {
        if *fail_budget > 0 {
            *fail_budget -= 1;
            anyhow::bail!("аудиоустройство недоступно");
        }
        let (kind, label, on_preferred) = match backend {
            AudioBackend::Null => {
                let (mixer, mut out) = rodio::dynamic_mixer::mixer::<f32>(2, 44_100);
                let alive = Arc::new(AtomicBool::new(true));
                let alive2 = alive.clone();
                std::thread::Builder::new()
                    .name("null-audio".into())
                    .spawn(move || {
                        while alive2.load(Ordering::Relaxed) {
                            for _ in 0..882 {
                                let _ = out.next();
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    })
                    .map_err(|e| anyhow!("не удалось запустить поток вывода: {e}"))?;
                (OutputKind::Null { mixer, alive }, "null".to_string(), true)
            }
            AudioBackend::Device => {
                let exact = preferred.and_then(find_exact_device).and_then(|d| {
                    let name = d.name().unwrap_or_default();
                    rodio::OutputStream::try_from_device(&d).ok().map(|o| (o, name))
                });
                match exact {
                    Some(((stream, handle), name)) => (OutputKind::Device { _stream: stream, handle }, name, true),
                    None => {
                        let (stream, handle) = rodio::OutputStream::try_default()
                            .map_err(|e| anyhow!("не удалось открыть аудиоустройство: {e}"))?;
                        let name = rodio::cpal::default_host()
                            .default_output_device()
                            .and_then(|d| d.name().ok())
                            .unwrap_or_else(|| "по умолчанию".into());
                        (OutputKind::Device { _stream: stream, handle }, name, preferred.is_none())
                    }
                }
            }
        };
        let beat = Arc::new(AtomicU64::new(epoch.elapsed().as_millis() as u64));
        let out = Output { kind, label, on_preferred, beat: beat.clone(), epoch };
        out.add(Heartbeat { beat, epoch, n: 0 })?;
        Ok(out)
    }

    fn add<S: rodio::Source<Item = f32> + Send + 'static>(&self, source: S) -> Result<()> {
        match &self.kind {
            OutputKind::Device { handle, .. } => {
                handle.play_raw(source).map_err(|e| anyhow!("ошибка аудиовыхода: {e}"))
            }
            OutputKind::Null { mixer, .. } => {
                mixer.add(source);
                Ok(())
            }
        }
    }

    /// New sink feeding this output.
    fn sink(&self) -> Result<rodio::Sink> {
        let (sink, queue) = rodio::Sink::new_idle();
        self.add(queue)?;
        Ok(sink)
    }

    fn stalled(&self) -> bool {
        let now = self.epoch.elapsed().as_millis() as u64;
        now.saturating_sub(self.beat.load(Ordering::Relaxed)) > STALL_TIMEOUT.as_millis() as u64
    }

    #[cfg(test)]
    fn freeze(&self) {
        if let OutputKind::Null { alive, .. } = &self.kind {
            alive.store(false, Ordering::Relaxed);
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        if let OutputKind::Null { alive, .. } = &self.kind {
            alive.store(false, Ordering::Relaxed);
        }
    }
}

/// Linear slider value → perceptual gain.
fn gain(volume: f32) -> f32 {
    let v = volume.clamp(0.0, 1.0);
    v * v
}

/// State of the output thread.
struct Engine {
    backend: AudioBackend,
    events: tokio::sync::mpsc::UnboundedSender<AudioEvent>,
    epoch: std::time::Instant,
    preferred: Option<String>,
    volume: f32,
    output: Option<Output>,
    sink: Option<rodio::Sink>,
    /// Generation + control block of the track attached to the output.
    current: Option<(u64, Arc<SourceCtl>)>,
    /// A track loaded while no output was available; attached on recovery.
    parked: Option<(ScSource, bool)>,
    ended_sent: bool,
    /// The device disappeared and we are trying to get it back.
    lost: bool,
    next_reopen: std::time::Instant,
    next_probe: std::time::Instant,
    fail_budget: u32,
}

impl Engine {
    fn notify(&self, ev: AudioEvent) {
        let _ = self.events.send(ev);
    }

    fn try_open(&mut self) -> bool {
        match Output::open(self.backend, self.preferred.as_deref(), self.epoch, &mut self.fail_budget) {
            Ok(o) => {
                log::info!("audio output opened: {}", o.label);
                self.output = Some(o);
                true
            }
            Err(e) => {
                log::debug!("cannot open audio output: {e:#}");
                self.next_reopen = std::time::Instant::now() + REOPEN_INTERVAL;
                false
            }
        }
    }

    /// Attach `source` to the current output (or park it until there is one).
    fn attach(&mut self, source: ScSource, gen: u64, paused: bool) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        self.current = Some((gen, source.ctl.clone()));
        self.ended_sent = true;
        if self.output.is_none() && !self.lost {
            self.try_open();
        }
        let Some(out) = &self.output else {
            // No device right now: keep the decoder, resume when one appears.
            self.parked = Some((source, paused));
            if !self.lost {
                self.lost = true;
                self.notify(AudioEvent::DeviceLost("Нет доступного аудиоустройства — ожидаю подключения…".into()));
            }
            return;
        };
        match out.sink() {
            Ok(s) => {
                s.set_volume(gain(self.volume));
                s.append(source);
                if paused {
                    s.pause();
                }
                self.sink = Some(s);
                self.ended_sent = false;
            }
            Err(e) => {
                self.current = None;
                self.notify(AudioEvent::Error(format!("{e:#}")));
            }
        }
    }

    /// Drop the output and everything attached to it.
    fn drop_output(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        self.output = None;
    }

    /// The output came (back): resume what was playing.
    fn on_output_ready(&mut self, restored: bool) {
        let label = self.output.as_ref().map(|o| o.label.clone()).unwrap_or_default();
        if restored {
            self.notify(AudioEvent::DeviceRestored(format!("Звук переключён на «{label}»")));
        }
        if let Some((source, paused)) = self.parked.take() {
            let gen = self.current.as_ref().map(|(g, _)| *g).unwrap_or(0);
            self.attach(source, gen, paused);
        } else if let Some((gen, _)) = self.current.take() {
            self.ended_sent = true;
            self.notify(AudioEvent::DeviceReset(gen));
        }
    }

    fn handle(&mut self, cmd: AudioCmd) {
        match cmd {
            AudioCmd::Load { source, gen, paused } => {
                self.parked = None;
                self.attach(source, gen, paused);
            }
            AudioCmd::Play | AudioCmd::Pause => {
                let pause = matches!(cmd, AudioCmd::Pause);
                if let Some(s) = &self.sink {
                    if pause {
                        s.pause()
                    } else {
                        s.play()
                    }
                }
                if let Some((_, p)) = &mut self.parked {
                    *p = pause;
                }
            }
            AudioCmd::Stop => {
                if let Some(s) = self.sink.take() {
                    s.stop();
                }
                self.parked = None;
                self.current = None;
                self.ended_sent = true;
            }
            AudioCmd::SetVolume(v) => {
                self.volume = v;
                if let Some(s) = &self.sink {
                    s.set_volume(gain(v));
                }
            }
            AudioCmd::SetDevice(name) => {
                self.preferred = name;
                self.drop_output();
                self.lost = false;
                if self.try_open() {
                    self.on_output_ready(false);
                } else {
                    self.lost = true;
                    self.notify(AudioEvent::DeviceLost("Устройство недоступно — ожидаю подключения…".into()));
                }
            }
            #[cfg(test)]
            AudioCmd::SimulateDeviceLoss { fail_reopens } => {
                if let Some(o) = &self.output {
                    o.freeze();
                }
                self.fail_budget = fail_reopens;
            }
        }
    }

    /// Periodic checks: dead device, reconnection, preferred device return,
    /// end of track.
    fn tick(&mut self) {
        let now = std::time::Instant::now();

        if self.output.as_ref().map(Output::stalled).unwrap_or(false) {
            let label = self.output.as_ref().map(|o| o.label.clone()).unwrap_or_default();
            log::warn!("audio device '{label}' stopped responding, reconnecting");
            self.drop_output();
            self.lost = true;
            self.next_reopen = now;
            self.notify(AudioEvent::DeviceLost(format!("Аудиоустройство «{label}» отключено — переподключаюсь…")));
        }

        if self.output.is_none() && now >= self.next_reopen && self.try_open() {
            let restored = std::mem::take(&mut self.lost);
            self.on_output_ready(restored);
        }

        // Running on a fallback: switch back once the chosen device returns.
        let on_fallback = self.output.as_ref().map(|o| !o.on_preferred).unwrap_or(false);
        if on_fallback && self.backend == AudioBackend::Device && now >= self.next_probe {
            self.next_probe = now + PREFERRED_PROBE_INTERVAL;
            if self.preferred.as_deref().and_then(find_exact_device).is_some() {
                log::info!("preferred audio device is back, switching");
                self.drop_output();
                if self.try_open() {
                    self.on_output_ready(true);
                } else {
                    self.lost = true;
                }
            }
        }

        if !self.ended_sent {
            if let (Some((gen, _)), Some(s)) = (&self.current, &self.sink) {
                if s.empty() {
                    self.ended_sent = true;
                    self.notify(AudioEvent::Ended(*gen));
                }
            }
        }
    }
}

fn audio_thread(
    rx: mpsc::Receiver<AudioCmd>,
    events: tokio::sync::mpsc::UnboundedSender<AudioEvent>,
    settings: Settings,
    backend: AudioBackend,
) {
    let now = std::time::Instant::now();
    let mut engine = Engine {
        backend,
        events,
        epoch: now,
        preferred: settings.output_device.clone(),
        volume: settings.volume,
        output: None,
        sink: None,
        current: None,
        parked: None,
        ended_sent: true,
        lost: false,
        next_reopen: now,
        next_probe: now + PREFERRED_PROBE_INTERVAL,
        fail_budget: 0,
    };
    if !engine.try_open() {
        engine.notify(AudioEvent::Error("Не удалось открыть аудиоустройство — повторю попытку автоматически".into()));
    }
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(cmd) => engine.handle(cmd),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        engine.tick();
    }
}

// ----------------------------------------------------------------------
// Player controller
// ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    Loading,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub track: Option<Track>,
    pub index: Option<usize>,
    pub queue_len: usize,
    pub status: Status,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub repeat: RepeatMode,
    pub liked: bool,
    pub preview: bool,
    pub error: Option<String>,
    pub output_device: Option<String>,
}

struct Inner {
    queue: Vec<Track>,
    index: Option<usize>,
    status: Status,
    ctl: Option<Arc<SourceCtl>>,
    buffer: Option<StreamBuffer>,
    preview: bool,
    hint: &'static str,
    error: Option<String>,
    settings: Settings,
}

/// Receives every player state change (the app forwards it to the webview).
pub type StateSink = Arc<dyn Fn(&PlayerState) + Send + Sync>;

pub struct Player {
    on_state: StateSink,
    api: Arc<ApiClient>,
    tx: Mutex<mpsc::Sender<AudioCmd>>,
    inner: Mutex<Inner>,
    gen: AtomicU64,
    seek_seq: AtomicU64,
    mpris: OnceLock<MprisHandle>,
}

impl Player {
    pub fn new(api: Arc<ApiClient>, on_state: StateSink, backend: AudioBackend) -> Arc<Player> {
        let settings: Settings = config::load_from(api.config_dir(), "settings.json");
        let (tx, rx) = mpsc::channel();
        let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel();
        let s2 = settings.clone();
        let spawned = std::thread::Builder::new()
            .name("audio-output".into())
            .spawn(move || audio_thread(rx, ev_tx.clone(), s2, backend));
        let startup_error = spawned.err().map(|e| {
            log::error!("cannot start audio thread: {e}");
            format!("не удалось запустить аудиопоток: {e}")
        });

        let player = Arc::new(Player {
            on_state,
            api,
            tx: Mutex::new(tx),
            inner: Mutex::new(Inner {
                queue: Vec::new(),
                index: None,
                status: Status::Idle,
                ctl: None,
                buffer: None,
                preview: false,
                hint: "mp3",
                error: startup_error,
                settings,
            }),
            gen: AtomicU64::new(0),
            seek_seq: AtomicU64::new(0),
            mpris: OnceLock::new(),
        });

        // Events coming back from the audio thread.
        let p = player.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(ev) = ev_rx.recv().await {
                match ev {
                    AudioEvent::Ended(gen) if gen == p.gen.load(Ordering::SeqCst) => p.advance(false).await,
                    AudioEvent::Ended(_) => {}
                    AudioEvent::DeviceReset(gen) => {
                        let snapshot = {
                            let i = p.inner.lock();
                            if p.gen.load(Ordering::SeqCst) != gen
                                || !matches!(i.status, Status::Playing | Status::Paused) {
                                None
                            } else {
                                i.buffer.clone().zip(i.ctl.clone()).map(|(b, c)| {
                                    let pos = c.position();
                                    (b, c, i.hint, pos, p.seek_seq.load(Ordering::SeqCst))
                                })
                            }
                        };
                        if let Some((buffer, ctl, hint, pos, seq)) = snapshot {
                            let result = tokio::task::spawn_blocking(move || open_source(&buffer, hint, ctl, pos)).await;
                            let mut i = p.inner.lock();
                            if p.gen.load(Ordering::SeqCst) == gen
                                && p.seek_seq.load(Ordering::SeqCst) == seq
                                && matches!(i.status, Status::Playing | Status::Paused) {
                                match result {
                                    Ok(Ok(source)) => p.send(AudioCmd::Load { source, gen, paused: i.status == Status::Paused }),
                                    other => i.error = Some(format!("Не удалось восстановить звук: {}", match other {
                                        Ok(Err(e)) => e.to_string(),
                                        Err(e) => e.to_string(),
                                        _ => unreachable!(),
                                    })),
                                }
                            }
                            drop(i);
                            p.emit();
                        }
                    }
                    AudioEvent::DeviceLost(message) => {
                        p.inner.lock().error = Some(message);
                        p.emit();
                    }
                    AudioEvent::DeviceRestored(message) => {
                        log::info!("{message}");
                        p.inner.lock().error = None;
                        p.emit();
                    }
                    AudioEvent::Error(e) => {
                        p.inner.lock().error = Some(e);
                        p.emit();
                    }
                }
            }
        });

        // Position ticker for the UI while playing.
        let p = player.clone();
        tauri::async_runtime::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_millis(250));
            loop {
                iv.tick().await;
                let st = p.inner.lock().status;
                if st == Status::Playing || st == Status::Loading {
                    p.emit();
                }
            }
        });
        player
    }

    pub fn set_mpris(&self, handle: MprisHandle) {
        let _ = self.mpris.set(handle);
    }

    fn send(&self, cmd: AudioCmd) {
        let _ = self.tx.lock().send(cmd);
    }

    fn position(&self) -> f64 {
        self.inner.lock().ctl.as_ref().map(|c| c.position()).unwrap_or(0.0)
    }

    pub fn state(&self) -> PlayerState {
        let i = self.inner.lock();
        let track = i.index.and_then(|ix| i.queue.get(ix)).cloned();
        let position = i.ctl.as_ref().map(|c| c.position()).unwrap_or(0.0);
        let duration = track.as_ref().map(|t| t.duration_ms as f64 / 1000.0).unwrap_or(0.0);
        PlayerState {
            liked: track.as_ref().map(|t| self.api.is_liked(t.id)).unwrap_or(false),
            track,
            index: i.index,
            queue_len: i.queue.len(),
            status: i.status,
            position,
            duration,
            volume: i.settings.volume,
            repeat: i.settings.repeat,
            preview: i.preview,
            error: i.error.clone(),
            output_device: i.settings.output_device.clone(),
        }
    }

    pub fn emit(&self) {
        let st = self.state();
        (self.on_state)(&st);
    }

    fn notify_mpris_playback(&self) {
        if let Some(m) = self.mpris.get() {
            let st = self.state();
            m.send(MprisUpdate::Playback { status: st.status, position: st.position });
        }
    }

    fn notify_mpris_track(&self) {
        if let Some(m) = self.mpris.get() {
            let st = self.state();
            if let Some(t) = st.track {
                m.send(MprisUpdate::Metadata {
                    title: t.title,
                    artist: t.artist,
                    cover: t.artwork_url,
                    duration: Duration::from_millis(t.duration_ms),
                });
            }
        }
    }

    fn save_settings(&self) {
        let s = self.inner.lock().settings.clone();
        if let Err(e) = config::save_to(self.api.config_dir(), "settings.json", &s) {
            log::warn!("cannot save settings: {e}");
        }
    }

    pub fn settings(&self) -> Settings {
        self.inner.lock().settings.clone()
    }

    pub fn set_close_to_tray(&self, v: bool) {
        self.inner.lock().settings.close_to_tray = v;
        self.save_settings();
    }

    /// Replace the queue and start playing `index`.
    pub async fn play_queue(&self, tracks: Vec<Track>, index: usize, start_fraction: Option<f64>) {
        let start = {
            let mut i = self.inner.lock();
            let index = index.min(tracks.len().saturating_sub(1));
            let start = start_fraction
                .zip(tracks.get(index))
                .filter(|(f, _)| f.is_finite())
                .map(|(f, t)| f.clamp(0.0, 1.0) * t.duration_ms as f64 / 1000.0)
                .unwrap_or(0.0);
            i.queue = tracks;
            i.index = if i.queue.is_empty() { None } else { Some(index) };
            start
        };
        self.load_current(start, true).await;
    }

    /// Resolve, download and start decoding the current queue entry.
    ///
    /// Concurrency: every load takes a new generation number *under the
    /// `inner` lock*, and all later state changes / audio commands for that
    /// load are only applied under the same lock if the generation is still
    /// current. A slow, superseded load can therefore never touch the state or
    /// the output of the newer one.
    async fn load_current(&self, start_secs: f64, autoplay: bool) {
        let start_secs = if start_secs.is_finite() { start_secs.max(0.0) } else { 0.0 };
        let (gen, track) = {
            let mut i = self.inner.lock();
            let gen = self.gen.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(b) = i.buffer.take() {
                b.cancel();
            }
            if let Some(c) = i.ctl.take() {
                c.stop.store(true, Ordering::Relaxed);
            }
            i.error = None;
            i.preview = false;
            match i.index.and_then(|ix| i.queue.get(ix)).cloned() {
                Some(t) => {
                    i.status = Status::Loading;
                    self.send(AudioCmd::Stop);
                    (gen, t)
                }
                None => {
                    i.status = Status::Idle;
                    self.send(AudioCmd::Stop);
                    drop(i);
                    self.emit();
                    return;
                }
            }
        };
        self.emit();
        self.notify_mpris_track();
        self.notify_mpris_playback();

        let result: Result<()> = async {
            let src = self.api.resolve_stream(&track).await?;
            let preview = src.preview;
            let hint = src.hint;
            let buffer = StreamBuffer::new();
            let ctl = Arc::new(SourceCtl::default());
            {
                let mut i = self.inner.lock();
                if self.gen.load(Ordering::SeqCst) != gen {
                    buffer.cancel();
                    return Ok(());
                }
                i.buffer = Some(buffer.clone());
                i.ctl = Some(ctl.clone());
                i.preview = preview;
                i.hint = hint;
            }
            tauri::async_runtime::spawn(stream::download(self.api.http.clone(), src, buffer.clone()));

            let ctl2 = ctl.clone();
            let b2 = buffer.clone();
            let source = tokio::task::spawn_blocking(move || open_source(&b2, hint, ctl2, start_secs))
                .await
                .map_err(|e| anyhow!("decoder thread: {e}"))?;
            let source = match source {
                Ok(s) => s,
                Err(e) => {
                    if buffer.is_cancelled() {
                        return Ok(());
                    }
                    return Err(buffer.error().map(|d| anyhow!("{e:#}: {d}")).unwrap_or(e));
                }
            };
            let mut i = self.inner.lock();
            if self.gen.load(Ordering::SeqCst) == gen {
                self.send(AudioCmd::Load { source, gen, paused: !autoplay });
                i.status = if autoplay { Status::Playing } else { Status::Paused };
            }
            Ok(())
        }
        .await;

        if let Err(e) = result {
            let mut i = self.inner.lock();
            if self.gen.load(Ordering::SeqCst) == gen {
                log::warn!("playback failed for track {}: {e:#}", track.id);
                i.status = Status::Idle;
                i.error = Some(format!("{e:#}"));
            }
        }
        self.emit();
        self.notify_mpris_playback();
    }

    pub async fn toggle(&self) {
        let st = self.inner.lock().status;
        match st {
            Status::Playing => self.pause(),
            Status::Paused => self.resume(),
            Status::Idle => {
                let has = self.inner.lock().index.is_some();
                if has {
                    self.load_current(0.0, true).await;
                }
            }
            Status::Loading => {}
        }
    }

    pub fn pause(&self) {
        let mut i = self.inner.lock();
        if i.status == Status::Playing {
            i.status = Status::Paused;
            drop(i);
            self.send(AudioCmd::Pause);
            self.emit();
            self.notify_mpris_playback();
        }
    }

    pub fn resume(&self) {
        let mut i = self.inner.lock();
        if i.status == Status::Paused {
            i.status = Status::Playing;
            drop(i);
            self.send(AudioCmd::Play);
            self.emit();
            self.notify_mpris_playback();
        }
    }

    pub async fn play(&self) {
        let st = self.inner.lock().status;
        match st {
            Status::Paused => self.resume(),
            Status::Idle => self.toggle().await,
            _ => {}
        }
    }

    pub fn stop(&self) {
        {
            let mut i = self.inner.lock();
            self.gen.fetch_add(1, Ordering::SeqCst);
            if let Some(b) = i.buffer.take() {
                b.cancel();
            }
            if let Some(c) = i.ctl.take() {
                c.stop.store(true, Ordering::Relaxed);
            }
            i.status = Status::Idle;
            self.send(AudioCmd::Stop);
        }
        self.emit();
        self.notify_mpris_playback();
    }

    /// Move to the next track. `manual` = user pressed "next".
    pub async fn advance(&self, manual: bool) {
        let next = {
            let mut i = self.inner.lock();
            let len = i.queue.len();
            let Some(cur) = i.index else { return };
            let repeat = i.settings.repeat;
            let next = if !manual && repeat == RepeatMode::One {
                Some(cur)
            } else if cur + 1 < len {
                Some(cur + 1)
            } else if repeat == RepeatMode::All || (manual && repeat == RepeatMode::One) {
                Some(0)
            } else {
                None
            };
            if let Some(n) = next {
                i.index = Some(n);
            }
            next
        };
        match next {
            Some(_) => self.load_current(0.0, true).await,
            None => {
                // End of queue: stop and rewind.
                self.stop();
            }
        }
    }

    pub async fn previous(self: &Arc<Self>) {
        if self.position() > 3.0 {
            self.seek(0.0);
            return;
        }
        {
            let mut i = self.inner.lock();
            let Some(cur) = i.index else { return };
            let len = i.queue.len();
            i.index = Some(if cur > 0 {
                cur - 1
            } else if i.settings.repeat == RepeatMode::All {
                len.saturating_sub(1)
            } else {
                0
            });
        }
        self.load_current(0.0, true).await;
    }

    pub async fn jump_to(&self, index: usize) {
        {
            let mut i = self.inner.lock();
            if index >= i.queue.len() {
                return;
            }
            i.index = Some(index);
        }
        self.load_current(0.0, true).await;
    }

    pub fn seek(self: &Arc<Self>, secs: f64) {
        if !secs.is_finite() {
            return;
        }
        let (ctl, buffer, hint, duration) = {
            let i = self.inner.lock();
            if i.status == Status::Loading || i.status == Status::Idle {
                return; // nothing to seek in (yet)
            }
            let dur = i.index.and_then(|ix| i.queue.get(ix)).map(|t| t.duration_ms as f64 / 1000.0);
            (i.ctl.clone(), i.buffer.clone(), i.hint, dur.unwrap_or(f64::MAX))
        };
        let (Some(ctl), Some(buffer)) = (ctl, buffer) else { return };
        let secs = secs.clamp(0.0, (duration - 0.5).max(0.0));
        if hint == "mp4" {
            // Re-open the decoder at the HLS segment holding `secs`, reusing
            // the already-downloaded bytes.
            ctl.set_position(secs);
            let seq = self.seek_seq.fetch_add(1, Ordering::SeqCst) + 1;
            let gen = self.gen.load(Ordering::SeqCst);
            let this = self.clone();
            tauri::async_runtime::spawn(async move {
                let ctl2 = ctl.clone();
                let res = tokio::task::spawn_blocking(move || open_source(&buffer, "mp4", ctl2, secs)).await;
                match res {
                    Ok(Ok(source)) => {
                        // Check-and-send under the lock (see load_current).
                        let i = this.inner.lock();
                        let current = this.gen.load(Ordering::SeqCst) == gen
                            && this.seek_seq.load(Ordering::SeqCst) == seq
                            && matches!(i.status, Status::Playing | Status::Paused);
                        if current {
                            this.send(AudioCmd::Load { source, gen, paused: i.status == Status::Paused });
                        }
                    }
                    Ok(Err(e)) => log::warn!("seek failed: {e:#}"),
                    Err(e) => log::warn!("seek task failed: {e}"),
                }
                this.notify_mpris_playback();
            });
        } else {
            ctl.request_seek(secs);
        }
        self.emit();
        self.notify_mpris_playback();
    }

    pub fn seek_relative(self: &Arc<Self>, delta: f64) {
        if !delta.is_finite() {
            return;
        }
        let pos = self.position();
        self.seek((pos + delta).max(0.0));
    }

    pub fn set_volume(&self, v: f32) {
        if !v.is_finite() {
            return;
        }
        let v = v.clamp(0.0, 1.0);
        self.inner.lock().settings.volume = v;
        self.send(AudioCmd::SetVolume(v));
        self.save_settings();
        if let Some(m) = self.mpris.get() {
            m.send(MprisUpdate::Volume(v as f64));
        }
        self.emit();
    }

    pub fn set_repeat(&self, mode: RepeatMode) {
        self.inner.lock().settings.repeat = mode;
        self.save_settings();
        self.emit();
    }

    pub fn set_device(&self, name: Option<String>) {
        self.inner.lock().settings.output_device = name.clone();
        self.save_settings();
        self.send(AudioCmd::SetDevice(name));
        self.emit();
    }

    pub fn queue(&self) -> Vec<Track> {
        self.inner.lock().queue.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiClient;

    use crate::api::tests::client;
    use crate::config::AuthStore;
    use serde_json::json;
    use wiremock::matchers::path_regex;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// `frames` MPEG-1 Layer III frames of digital silence (mono, 44.1 kHz,
    /// 128 kbit/s, 1152 samples each): zeroed side info = no Huffman data.
    pub fn silent_mp3(frames: usize) -> Vec<u8> {
        const FRAME_LEN: usize = 417; // 144 * 128000 / 44100, no padding
        let mut out = Vec::with_capacity(frames * FRAME_LEN);
        for _ in 0..frames {
            out.extend_from_slice(&[0xFF, 0xFB, 0x90, 0xC0]);
            out.resize(out.len() + FRAME_LEN - 4, 0);
        }
        out
    }

    #[test]
    fn output_recovers_without_network() {
        let (tx, rx) = mpsc::channel();
        let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
        let worker = std::thread::spawn(move || audio_thread(rx, events, Settings::default(), AudioBackend::Null));
        let buffer = StreamBuffer::new();
        buffer.push_for_test(&silent_mp3(400));
        let ctl = Arc::new(SourceCtl::default());
        let source = open_source(&buffer, "mp3", ctl.clone(), 0.0).unwrap();
        tx.send(AudioCmd::Load { source, gen: 42, paused: false }).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(ctl.position() > 0.0);
        tx.send(AudioCmd::SimulateDeviceLoss { fail_reopens: 2 }).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let (mut lost, mut restored, mut reset) = (false, false, false);
        while std::time::Instant::now() < deadline && !reset {
            while let Ok(event) = received.try_recv() {
                match event {
                    AudioEvent::DeviceLost(_) => lost = true,
                    AudioEvent::DeviceRestored(_) => restored = true,
                    AudioEvent::DeviceReset(gen) => { assert_eq!(gen, 42); reset = true; }
                    AudioEvent::Ended(_) => panic!("device loss must not end the track"),
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        drop(tx);
        worker.join().unwrap();
        assert!(lost && restored && reset);
    }

    #[test]
    fn decodes_and_seeks_in_memory_mp3() {
        let buf = StreamBuffer::new();
        buf.push_for_test(&silent_mp3(200)); // ~5.2 s
        let ctl = Arc::new(SourceCtl::default());
        let mut src = open_source(&buf, "mp3", ctl.clone(), 0.0).expect("probe mp3");
        assert_eq!(rodio::Source::sample_rate(&src), 44_100);
        assert_eq!(src.by_ref().take(44_100).count(), 44_100);
        ctl.request_seek(3.0);
        assert!(src.by_ref().take(10).count() == 10);
        assert!((3.0..3.2).contains(&ctl.position()), "pos {}", ctl.position());
        // Seeking past the end just ends the stream, no panic.
        ctl.request_seek(1e6);
        assert_eq!(src.by_ref().take(10).count(), 0);
    }

    #[test]
    fn corrupt_stream_is_an_error_not_a_panic() {
        for data in [vec![], vec![0u8; 10_000], b"<html>not audio</html>".to_vec()] {
            let buf = StreamBuffer::new();
            buf.push_for_test(&data);
            let ctl = Arc::new(SourceCtl::default());
            assert!(open_source(&buf, "mp3", ctl.clone(), 0.0).is_err());
            assert!(open_source(&buf, "mp4", ctl, 0.0).is_err());
        }
    }

    fn mock_track(base: &str, id: u64) -> Track {
        Track::from_value(&json!({
            "kind":"track","id":id,"title":format!("track {id}"),"duration":5000,
            "user":{"id":1,"username":"artist"},
            "media":{"transcodings":[{
                "url": format!("{base}/v2/media/{id}/stream/progressive"),
                "format":{"protocol":"progressive","mime_type":"audio/mpeg"}
            }]}
        }))
        .unwrap()
    }

    async fn wait_for(p: &Arc<Player>, what: &str, f: impl Fn(&PlayerState) -> bool) -> PlayerState {
        for _ in 0..200 {
            let st = p.state();
            if f(&st) {
                return st;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("timed out waiting for {what}: {:?}", p.state());
    }

    /// Hammer the controller from many tasks at once (Play/Pause/Seek/Next/
    /// Prev/Stop/volume) on the null backend, then check it still works.
    /// A deadlock shows up as the outer timeout, a panic as a JoinError.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn controller_survives_rapid_state_changes() {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(path_regex(r"^/v2/media/\d+/stream/progressive$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"url": format!("{base}/audio.mp3")})))
            .mount(&server)
            .await;
        Mock::given(path_regex(r"^/audio\.mp3$"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "audio/mpeg")
                    .set_body_bytes(silent_mp3(200)),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let api = Arc::new(client(&server, dir.path(), AuthStore { v2_client_id: Some("CID".into()), ..Default::default() }));
        let emitted = Arc::new(AtomicU64::new(0));
        let counter = emitted.clone();
        let sink: StateSink = Arc::new(move |_st: &PlayerState| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        let player = Player::new(api, sink, AudioBackend::Null);
        let tracks: Vec<Track> = (1..=3).map(|id| mock_track(&base, id)).collect();

        tokio::time::timeout(Duration::from_secs(60), async {
            player.play_queue(tracks.clone(), 0, None).await;
            wait_for(&player, "first track playing", |s| s.status == Status::Playing).await;

            let mut tasks = Vec::new();
            for worker in 0..8u64 {
                let p = player.clone();
                let t = tracks.clone();
                tasks.push(tokio::spawn(async move {
                    for step in 0..30u64 {
                        match (worker + step) % 9 {
                            0 => p.play_queue(t.clone(), (step % 3) as usize, Some(0.4)).await,
                            1 => p.toggle().await,
                            2 => p.pause(),
                            3 => p.resume(),
                            4 => p.seek(step as f64 * 0.1),
                            5 => p.advance(true).await,
                            6 => p.previous().await,
                            7 => p.stop(),
                            _ => {
                                p.set_volume((step % 10) as f32 / 10.0);
                                p.set_repeat(if step % 2 == 0 { RepeatMode::All } else { RepeatMode::Off });
                                p.seek(f64::NAN);
                                p.set_volume(f32::NAN);
                                let _ = p.state();
                            }
                        }
                        if step % 5 == 0 {
                            tokio::task::yield_now().await;
                        }
                    }
                }));
            }
            for t in tasks {
                t.await.expect("controller task panicked");
            }

            // After the storm the player must still behave deterministically.
            player.play_queue(tracks.clone(), 1, None).await;
            let st = wait_for(&player, "track 2 playing", |s| s.status == Status::Playing).await;
            assert_eq!(st.track.as_ref().map(|t| t.id), Some(2));
            assert!(st.error.is_none(), "{:?}", st.error);
            let p0 = player.state().position;
            tokio::time::sleep(Duration::from_millis(600)).await;
            let p1 = player.state().position;
            assert!(p1 > p0, "position must advance on the null sink ({p0} -> {p1})");

            player.send(AudioCmd::SimulateDeviceLoss { fail_reopens: 2 });
            wait_for(&player, "device loss", |s| s.error.is_some()).await;
            player.pause();
            wait_for(&player, "device recovery", |s| s.error.is_none()).await;
            assert_eq!(player.state().status, Status::Paused);
            let recovered = player.state().position;
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!((player.state().position - recovered).abs() < 0.1);
            player.resume();
            wait_for(&player, "resumed position", |s| s.position > recovered + 0.1).await;

            player.pause();
            assert_eq!(player.state().status, Status::Paused);
            player.seek(2.0);
            assert!((player.state().position - 2.0).abs() < 0.5);
            player.resume();
            assert_eq!(player.state().status, Status::Playing);

            // Track end advances the queue automatically (repeat off).
            player.set_repeat(RepeatMode::Off);
            player.seek(4.8);
            wait_for(&player, "auto-advance to track 3", |s| s.track.as_ref().map(|t| t.id) == Some(3)).await;

            player.stop();
            assert_eq!(player.state().status, Status::Idle);
            let v = player.state().volume;
            assert!(v.is_finite() && (0.0..=1.0).contains(&v));
        })
        .await
        .expect("player deadlocked or stalled");
        assert!(emitted.load(Ordering::Relaxed) > 0);
    }

    /// End-to-end: scrape client_id, search, resolve, download and decode.
    /// Run with `cargo test -- --ignored --nocapture` (needs network).
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn stream_and_decode_real_track() {
        let dir = tempfile::tempdir().unwrap();
        let api = ApiClient::with_endpoints(crate::api::Endpoints::default(), dir.path().to_path_buf()).unwrap();
        let id = api.scrape_client_id(true).await.expect("scrape client_id");
        println!("client_id: {}", crate::config::mask(&id));
        let page = api.search("lofi hip hop").await.expect("search");
        println!("search returned {} tracks", page.tracks.len());
        assert!(!page.tracks.is_empty());
        let mut ok = 0;
        for track in page.tracks.iter().take(6) {
            let src = match api.resolve_stream(track).await {
                Ok(s) => s,
                Err(e) => {
                    println!("  {} — resolve failed: {e:#}", track.title);
                    continue;
                }
            };
            let buffer = StreamBuffer::new();
            tokio::spawn(stream::download(api.http.clone(), src.clone(), buffer.clone()));
            let hint = src.hint;
            let ctl = Arc::new(SourceCtl::default());
            let ctl2 = ctl.clone();
            let b2 = buffer.clone();
            let res = tokio::task::spawn_blocking(move || -> anyhow::Result<(u16, u32, usize, f64)> {
                let t0 = std::time::Instant::now();
                let mut s = open_source(&b2, hint, ctl2.clone(), 0.0)?;
                println!("    first audio after {:?}", t0.elapsed());
                let (ch, rate) = (rodio::Source::channels(&s), rodio::Source::sample_rate(&s));
                let n = (rate as usize) * ch as usize * 5; // 5 seconds
                let got = s.by_ref().take(n).count();
                // Seek to 30 s the same way the player does.
                let mut s = if hint == "mp4" {
                    open_source(&b2, hint, ctl2.clone(), 30.0)?
                } else {
                    ctl2.request_seek(30.0);
                    s
                };
                let after = s.by_ref().take(rate as usize * ch as usize).count();
                assert!(after > 0, "no samples after seek");
                Ok((ch, rate, got, ctl2.position()))
            })
            .await
            .unwrap();
            buffer.cancel();
            match res {
                Ok((ch, rate, got, pos)) => {
                    println!("  {} — {} {}ch {}Hz, decoded {} samples, pos after seek {:.1}s, preview={}",
                        track.title, hint, ch, rate, got, pos, src.preview);
                    assert!(got > 0);
                    assert!((30.0..32.0).contains(&pos), "unexpected position after seek: {pos}");
                    ok += 1;
                }
                Err(e) => println!("  {} — decode failed: {e:#}", track.title),
            }
        }
        assert!(ok > 0, "no track could be decoded");
        let wf = page.tracks.iter().find_map(|t| t.waveform_url.clone()).unwrap();
        let bars = api.waveform(&wf, 100).await.expect("waveform");
        assert_eq!(bars.len(), 100);
    }
}
