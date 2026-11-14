//! Progressive download buffer used by the decoder.
//!
//! A background task downloads the audio (a single progressive file or the
//! segments of an HLS playlist) into a shared, growing byte buffer. The
//! decoder reads from a [`StreamReader`], which blocks until the bytes it
//! needs have arrived — so playback starts after the first few kilobytes.

use crate::api::{net_err, StreamSource};
use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use parking_lot::{Condvar, Mutex};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::io::MediaSource;

#[derive(Default)]
struct Shared {
    data: Vec<u8>,
    done: bool,
    error: Option<String>,
    /// Total size if known up front (progressive downloads).
    total: Option<u64>,
    /// HLS only: length of the init segment (fMP4 `EXT-X-MAP`).
    init_len: usize,
    /// HLS only: start time of every segment (known once the playlist is parsed).
    seg_times: Option<Vec<f64>>,
    /// HLS only: byte offset of each segment, filled as segments arrive.
    seg_offsets: Vec<usize>,
}

#[derive(Clone)]
pub struct StreamBuffer {
    inner: Arc<(Mutex<Shared>, Condvar)>,
    cancelled: Arc<AtomicBool>,
}

impl StreamBuffer {
    pub fn new() -> Self {
        StreamBuffer { inner: Arc::new((Mutex::new(Shared::default()), Condvar::new())), cancelled: Arc::default() }
    }

    /// Seekable reader over the whole stream (MP3).
    pub fn reader(&self) -> StreamReader {
        StreamReader { buf: self.clone(), pos: 0, map: None, seekable: true }
    }

    /// Forward-only reader: lets demuxers (fMP4) start before the download ends.
    pub fn streaming_reader(&self) -> StreamReader {
        StreamReader { buf: self.clone(), pos: 0, map: None, seekable: false }
    }

    /// Forward-only reader that presents `init segment + segments[i..]`, where
    /// segment `i` contains `secs`. Returns the reader and the segment's start
    /// time. Blocks until that segment has been downloaded.
    pub fn reader_at(&self, secs: f64) -> Option<(StreamReader, f64)> {
        let (m, c) = &*self.inner;
        let mut s = m.lock();
        loop {
            if self.is_cancelled() {
                return None;
            }
            if let Some(times) = &s.seg_times {
                let i = times.iter().rposition(|&t| t <= secs).unwrap_or(0);
                if let Some(&off) = s.seg_offsets.get(i) {
                    let start = times[i];
                    let init = s.init_len;
                    return Some((
                        StreamReader { buf: self.clone(), pos: 0, map: Some((init, off)), seekable: false },
                        start,
                    ));
                }
            }
            if s.done {
                return None;
            }
            c.wait_for(&mut s, Duration::from_millis(250));
        }
    }

    fn set_segments(&self, times: Vec<f64>) {
        self.inner.0.lock().seg_times = Some(times);
    }

    fn mark_init_end(&self) {
        let mut s = self.inner.0.lock();
        s.init_len = s.data.len();
    }

    fn push_segment(&self, bytes: &[u8]) {
        let (m, c) = &*self.inner;
        let mut s = m.lock();
        let off = s.data.len();
        s.seg_offsets.push(off);
        s.data.extend_from_slice(bytes);
        c.notify_all();
    }

    /// Stop downloading and unblock readers (they will see EOF).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.inner.1.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn push(&self, bytes: &[u8]) {
        let (m, c) = &*self.inner;
        m.lock().data.extend_from_slice(bytes);
        c.notify_all();
    }

    /// Fill the buffer with a complete in-memory stream.
    #[cfg(test)]
    pub fn push_for_test(&self, bytes: &[u8]) {
        self.set_total(Some(bytes.len() as u64));
        self.push(bytes);
        self.finish(None);
    }

    fn set_total(&self, total: Option<u64>) {
        self.inner.0.lock().total = total;
    }

    fn finish(&self, error: Option<String>) {
        let (m, c) = &*self.inner;
        let mut s = m.lock();
        s.done = true;
        s.error = error;
        c.notify_all();
    }

    pub fn error(&self) -> Option<String> {
        self.inner.0.lock().error.clone()
    }
}

pub struct StreamReader {
    buf: StreamBuffer,
    /// Virtual read position.
    pos: u64,
    /// `(init_len, segment_offset)`: virtual bytes past the init segment are
    /// read from `segment_offset` onwards.
    map: Option<(usize, usize)>,
    seekable: bool,
}

impl StreamReader {
    /// Physical offset in the buffer and how many bytes may be read there
    /// before the mapping changes.
    fn physical(&self) -> (usize, usize) {
        let v = self.pos as usize;
        match self.map {
            Some((init, _)) if v < init => (v, init - v),
            Some((init, off)) => (v - init + off, usize::MAX),
            None => (v, usize::MAX),
        }
    }
}

impl Read for StreamReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let (start, limit) = self.physical();
        let (m, c) = &*self.buf.inner;
        let mut s = m.lock();
        loop {
            if self.buf.is_cancelled() {
                return Ok(0);
            }
            if start < s.data.len() {
                break;
            }
            if s.done {
                return match &s.error {
                    Some(e) => Err(io::Error::other(e.clone())),
                    None => Ok(0),
                };
            }
            c.wait_for(&mut s, Duration::from_millis(250));
        }
        let n = out.len().min(s.data.len() - start).min(limit);
        out[..n].copy_from_slice(&s.data[start..start + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for StreamReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::Current(d) => self.pos as i64 + d,
            SeekFrom::End(d) => {
                let (m, c) = &*self.buf.inner;
                let mut s = m.lock();
                // Without a known size we must wait for the download to end.
                while s.total.is_none() && !s.done && !self.buf.is_cancelled() {
                    c.wait_for(&mut s, Duration::from_millis(250));
                }
                let end = s.total.unwrap_or(s.data.len() as u64);
                end as i64 + d
            }
        };
        if target < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = target as u64;
        Ok(self.pos)
    }
}

impl MediaSource for StreamReader {
    fn is_seekable(&self) -> bool {
        self.seekable
    }

    fn byte_len(&self) -> Option<u64> {
        if !self.seekable {
            return None;
        }
        let s = self.buf.inner.0.lock();
        s.total.or(if s.done { Some(s.data.len() as u64) } else { None })
    }
}

// ----------------------------------------------------------------------
// Downloader
// ----------------------------------------------------------------------

/// Download `src` into `buf`. Never panics; errors are recorded in the buffer.
pub async fn download(http: reqwest::Client, src: StreamSource, buf: StreamBuffer) {
    let res = download_inner(&http, &src, &buf).await;
    if let Err(e) = &res {
        if !buf.is_cancelled() {
            log::warn!("stream download failed: {e:#}");
        }
    }
    buf.finish(res.err().map(|e| format!("{e:#}")));
}

/// Deadline for small HLS requests (playlists, ~10 s segments).
const SEGMENT_TIMEOUT: Duration = Duration::from_secs(60);

fn get(http: &reqwest::Client, url: &str, auth: Option<&str>) -> reqwest::RequestBuilder {
    let mut req = http.get(url);
    if let Some(a) = auth {
        req = req.header("Authorization", a);
    }
    req
}

async fn download_inner(http: &reqwest::Client, src: &StreamSource, buf: &StreamBuffer) -> Result<()> {
    // No overall timeout here (long progressive downloads); the client's
    // read timeout aborts stalled connections.
    let resp = get(http, &src.url, src.auth.as_deref())
        .send()
        .await
        .map_err(net_err)?
        .error_for_status()
        .map_err(net_err)?;
    let final_url = resp.url().clone();
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let len = resp.content_length();
    let mut body = resp.bytes_stream();

    let first = match body.next().await {
        Some(chunk) => chunk.map_err(net_err)?,
        None => bail!("пустой ответ сервера"),
    };
    let is_playlist = ctype.contains("mpegurl") || first.starts_with(b"#EXTM3U");

    if !is_playlist {
        buf.set_total(len);
        buf.push(&first);
        while let Some(chunk) = body.next().await {
            if buf.is_cancelled() {
                return Ok(());
            }
            buf.push(&chunk.map_err(net_err)?);
        }
        return Ok(());
    }

    // HLS: read the whole (small) playlist, then fetch segments in order.
    let mut text = String::from_utf8_lossy(&first).into_owned();
    while let Some(chunk) = body.next().await {
        text.push_str(&String::from_utf8_lossy(&chunk.map_err(net_err)?));
        if text.len() > 4 * 1024 * 1024 {
            bail!("HLS-плейлист слишком большой");
        }
    }
    let mut base = final_url;
    let mut playlist = parse_m3u8(&text, &base)?;
    if let Some(variant) = playlist.variant.take() {
        // Master playlist: follow the first variant.
        let resp = get(http, variant.as_str(), None)
            .timeout(SEGMENT_TIMEOUT)
            .send()
            .await
            .map_err(net_err)?
            .error_for_status()
            .map_err(net_err)?;
        base = resp.url().clone();
        let text = resp.text().await.map_err(net_err)?;
        playlist = parse_m3u8(&text, &base)?;
    }
    if playlist.segments.is_empty() {
        bail!("HLS-плейлист пуст");
    }
    let mut t = 0.0;
    let times = playlist
        .segments
        .iter()
        .map(|(_, d)| {
            let start = t;
            t += d;
            start
        })
        .collect();
    buf.set_segments(times);
    let mut parts: Vec<(url::Url, bool)> = Vec::with_capacity(playlist.segments.len() + 1);
    parts.extend(playlist.init.map(|u| (u, true)));
    parts.extend(playlist.segments.into_iter().map(|(u, _)| (u, false)));
    for (seg, is_init) in parts {
        if buf.is_cancelled() {
            return Ok(());
        }
        let mut attempt = 0;
        loop {
            attempt += 1;
            let res = async {
                let resp = get(http, seg.as_str(), None)
                    .timeout(SEGMENT_TIMEOUT)
                    .send()
                    .await
                    .map_err(net_err)?
                    .error_for_status()
                    .map_err(net_err)?;
                let mut s = resp.bytes_stream();
                let mut local = Vec::new();
                while let Some(chunk) = s.next().await {
                    local.extend_from_slice(&chunk.map_err(net_err)?);
                }
                anyhow::Ok(local)
            }
            .await;
            match res {
                Ok(bytes) => {
                    if is_init {
                        buf.push(&bytes);
                        buf.mark_init_end();
                    } else {
                        buf.push_segment(&bytes);
                    }
                    break;
                }
                Err(e) if attempt < 3 => {
                    log::warn!("segment retry {attempt}: {e:#}");
                    tokio::time::sleep(Duration::from_millis(500 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct M3u8 {
    pub init: Option<url::Url>,
    /// Segment URL and duration in seconds.
    pub segments: Vec<(url::Url, f64)>,
    pub variant: Option<url::Url>,
}

pub fn parse_m3u8(text: &str, base: &url::Url) -> Result<M3u8> {
    if !text.trim_start().starts_with("#EXTM3U") {
        bail!("ответ не является HLS-плейлистом");
    }
    let mut out = M3u8::default();
    // (bandwidth, url) of the variant announced by the previous STREAM-INF line.
    let mut pending_bw: Option<u64> = None;
    let mut best_bw = 0u64;
    let mut next_dur = 0.0f64;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            if let Some(uri) = rest.split("URI=\"").nth(1).and_then(|r| r.split('"').next()) {
                out.init = Some(base.join(uri)?);
            }
        } else if line.starts_with("#EXT-X-KEY") && !line.contains("METHOD=NONE") {
            bail!("зашифрованный HLS-поток не поддерживается");
        } else if let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let bw = attrs
                .split(',')
                .find_map(|a| a.strip_prefix("BANDWIDTH="))
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            pending_bw = Some(bw);
        } else if let Some(d) = line.strip_prefix("#EXTINF:") {
            next_dur = d.split(',').next().and_then(|x| x.trim().parse().ok()).unwrap_or(0.0);
        } else if line.starts_with('#') {
            continue;
        } else if let Some(bw) = pending_bw.take() {
            // Master playlist: keep the highest-bandwidth variant.
            if out.variant.is_none() || bw > best_bw {
                best_bw = bw;
                out.variant = Some(base.join(line)?);
            }
        } else {
            let url = base.join(line).map_err(|e| anyhow!("bad segment url: {e}"))?;
            out.segments.push((url, std::mem::take(&mut next_dur)));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_media_playlist() {
        let base = url::Url::parse("https://cf-hls-media.sndcdn.com/playlist/abc/x.m3u8?q=1").unwrap();
        let text = "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-MAP:URI=\"https://cdn/init.mp4\"\n#EXTINF:1.9,\nhttps://cdn/0.m4s\n#EXTINF:10,\nseg1.mp3\n#EXT-X-ENDLIST\n";
        let p = parse_m3u8(text, &base).unwrap();
        assert_eq!(p.init.unwrap().as_str(), "https://cdn/init.mp4");
        assert_eq!(p.segments.len(), 2);
        assert_eq!(p.segments[1].0.as_str(), "https://cf-hls-media.sndcdn.com/playlist/abc/seg1.mp3");
        assert_eq!(p.segments[0].1, 1.9);
    }

    #[test]
    fn master_playlist_picks_highest_bandwidth() {
        let base = url::Url::parse("https://x/m.m3u8").unwrap();
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=96000,CODECS=\"mp4a.40.2\"\nlow.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=160000\nhigh.m3u8\n";
        let p = parse_m3u8(text, &base).unwrap();
        assert_eq!(p.variant.unwrap().as_str(), "https://x/high.m3u8");
        assert!(p.segments.is_empty());
    }

    #[test]
    fn rejects_encrypted() {
        let base = url::Url::parse("https://x/").unwrap();
        let text = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\"\nseg.mp4\n";
        assert!(parse_m3u8(text, &base).is_err());
    }

    #[test]
    fn mapped_reader_skips_to_segment() {
        let buf = StreamBuffer::new();
        buf.set_segments(vec![0.0, 10.0, 20.0]);
        buf.push(b"INIT");
        buf.mark_init_end();
        buf.push_segment(b"aaaa");
        buf.push_segment(b"bbbb");
        buf.push_segment(b"cccc");
        buf.finish(None);
        let (mut r, start) = buf.reader_at(15.0).unwrap();
        assert_eq!(start, 10.0);
        let mut s = String::new();
        r.read_to_string(&mut s).unwrap();
        assert_eq!(s, "INITbbbbcccc");
    }

    #[test]
    fn reader_blocks_until_data() {
        let buf = StreamBuffer::new();
        let mut r = buf.reader();
        let b2 = buf.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            b2.push(b"hello");
            b2.finish(None);
        });
        let mut s = String::new();
        r.read_to_string(&mut s).unwrap();
        assert_eq!(s, "hello");
        t.join().unwrap();
    }
}
