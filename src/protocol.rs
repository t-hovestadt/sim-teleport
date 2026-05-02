use std::io;

/// Max UDP datagram size — stays under typical jumbo-frame MTU with headroom for IP/UDP headers.
pub const MAX_DATAGRAM_SIZE: usize = 9_000;

const HEADER_SIZE: usize = std::mem::size_of::<Header>();
pub const MAX_PAYLOAD_PER_DATAGRAM: usize = MAX_DATAGRAM_SIZE - HEADER_SIZE;

/// Page identifiers carried in every datagram's `buf_offset` field.
pub const PAGE_PHYSICS: u32 = 0;
pub const PAGE_GRAPHICS: u32 = 1;
pub const PAGE_STATIC: u32 = 2;
pub const PAGE_HEARTBEAT: u32 = u32::MAX;

/// Wire header prepended to every UDP datagram — 24 bytes, no padding.
///
/// Layout (all little-endian):
///   source_us    u64   microseconds spent on the source side (for latency stats)
///   sequence     u32   monotonically increasing per full message
///   payload_size u32   total compressed payload bytes across all fragments
///   buf_offset   u32   page id: 0=physics, 1=graphics, 2=static, u32::MAX=heartbeat
///   fragment     u16   0-based index of this fragment
///   fragments    u16   total fragment count for this sequence
#[repr(C, packed)]
struct Header {
    source_us: u64,
    sequence: u32,
    payload_size: u32,
    buf_offset: u32,
    fragment: u16,
    fragments: u16,
}

const _: () = assert!(std::mem::size_of::<Header>() == 24);

// ── Sender ────────────────────────────────────────────────────────────────────

pub struct Sender {
    sequence: u32,
    buf: Vec<u8>,
}

impl Default for Sender {
    fn default() -> Self {
        Self::new()
    }
}

impl Sender {
    pub fn new() -> Self {
        Self {
            sequence: 0,
            buf: vec![0u8; MAX_DATAGRAM_SIZE],
        }
    }

    /// Fragment `data` and call `send_fn` once per datagram.
    /// Returns the number of fragments sent.
    pub fn send<F>(
        &mut self,
        data: &[u8],
        source_us: u64,
        buf_offset: u32,
        mut send_fn: F,
    ) -> io::Result<u16>
    where
        F: FnMut(&[u8]) -> io::Result<()>,
    {
        let total = data.len();
        let n_fragments = total.div_ceil(MAX_PAYLOAD_PER_DATAGRAM);
        if n_fragments > u16::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "payload too large",
            ));
        }

        for i in 0..n_fragments {
            let offset = i * MAX_PAYLOAD_PER_DATAGRAM;
            let chunk = &data[offset..(offset + MAX_PAYLOAD_PER_DATAGRAM).min(total)];

            let hdr = Header {
                sequence: self.sequence,
                fragment: i as u16,
                fragments: n_fragments as u16,
                payload_size: total as u32,
                buf_offset,
                source_us,
            };

            let hdr_bytes =
                unsafe { std::slice::from_raw_parts(&hdr as *const _ as *const u8, HEADER_SIZE) };

            self.buf[..HEADER_SIZE].copy_from_slice(hdr_bytes);
            self.buf[HEADER_SIZE..HEADER_SIZE + chunk.len()].copy_from_slice(chunk);
            send_fn(&self.buf[..HEADER_SIZE + chunk.len()])?;
        }

        self.sequence = self.sequence.wrapping_add(1);
        Ok(n_fragments as u16)
    }

    /// Send a header-only heartbeat datagram (no payload, no sequence increment).
    /// The target resets its stale timer on receipt but performs no decompression.
    pub fn send_heartbeat<F>(&self, source_us: u64, mut send_fn: F) -> io::Result<()>
    where
        F: FnMut(&[u8]) -> io::Result<()>,
    {
        let hdr = Header {
            source_us,
            sequence: self.sequence,
            payload_size: 0,
            buf_offset: PAGE_HEARTBEAT,
            fragment: 0,
            fragments: 0,
        };

        let mut buf = [0u8; HEADER_SIZE];
        let hdr_bytes =
            unsafe { std::slice::from_raw_parts(&hdr as *const _ as *const u8, HEADER_SIZE) };
        buf.copy_from_slice(hdr_bytes);
        send_fn(&buf)
    }
}

// ── Receiver ──────────────────────────────────────────────────────────────────

pub struct Receiver {
    buf: Vec<u8>,
    received: Vec<bool>,
    current_seq: Option<u32>,
    total_frags: u16,
    got_frags: u16,
    payload_size: usize,
    pub last_source_us: u64,
    pub last_fragment_count: u16,
    pub last_buf_offset: u32,
}

impl Receiver {
    pub fn new(max_payload: usize) -> Self {
        Self {
            buf: Vec::with_capacity(max_payload),
            received: Vec::new(),
            current_seq: None,
            total_frags: 0,
            got_frags: 0,
            payload_size: 0,
            last_source_us: 0,
            last_fragment_count: 0,
            last_buf_offset: 0,
        }
    }

    /// Feed a raw UDP datagram.
    /// Returns `(Some(assembled_bytes), is_new_sequence)` when a full message is ready,
    /// otherwise `(None, is_new_sequence)`.
    ///
    /// Heartbeat datagrams (buf_offset == PAGE_HEARTBEAT, fragments == 0) return
    /// `(None, false)` — the caller checks `last_buf_offset` to handle them.
    pub fn ingest(&mut self, datagram: &[u8]) -> (Option<&[u8]>, bool) {
        if datagram.len() < HEADER_SIZE {
            return (None, false);
        }

        let hdr = unsafe { std::ptr::read_unaligned(datagram.as_ptr() as *const Header) };

        // Fragment 0 carries the source-side timestamp and page identity.
        if hdr.fragment == 0 {
            self.last_source_us = hdr.source_us;
            self.last_buf_offset = hdr.buf_offset;
        }

        // Heartbeat: fragments == 0, payload_size == 0 — not a real message.
        if hdr.buf_offset == PAGE_HEARTBEAT && hdr.fragments == 0 {
            self.last_buf_offset = PAGE_HEARTBEAT;
            return (None, false);
        }

        let new_seq = match self.current_seq {
            Some(s) => s != hdr.sequence,
            None => true,
        };

        if new_seq {
            self.reset(&hdr);
        }

        // Ignore out-of-range or duplicate fragments.
        let idx = hdr.fragment as usize;
        if idx >= self.total_frags as usize || self.received[idx] {
            return (None, hdr.fragment == 0);
        }

        let data = &datagram[HEADER_SIZE..];
        if data.len() > MAX_PAYLOAD_PER_DATAGRAM {
            return (None, hdr.fragment == 0);
        }
        let dest_offset = idx * MAX_PAYLOAD_PER_DATAGRAM;
        if dest_offset + data.len() > self.buf.len() {
            return (None, hdr.fragment == 0);
        }
        self.buf[dest_offset..dest_offset + data.len()].copy_from_slice(data);
        self.received[idx] = true;
        self.got_frags += 1;

        if self.got_frags == self.total_frags {
            self.last_fragment_count = self.total_frags;
            self.current_seq = None;
            (Some(&self.buf[..self.payload_size]), hdr.fragment == 0)
        } else {
            (None, hdr.fragment == 0)
        }
    }

    fn reset(&mut self, hdr: &Header) {
        self.current_seq = Some(hdr.sequence);
        self.total_frags = hdr.fragments;
        self.got_frags = 0;
        self.payload_size = hdr.payload_size as usize;

        self.received.clear();
        self.received.resize(hdr.fragments as usize, false);

        self.buf.clear();
        self.buf.resize(hdr.payload_size as usize, 0);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(payload: &[u8]) -> Vec<u8> {
        let mut sender = Sender::new();
        let mut datagrams = Vec::new();
        sender
            .send(payload, 42, PAGE_PHYSICS, |d| {
                datagrams.push(d.to_vec());
                Ok(())
            })
            .unwrap();

        let mut receiver = Receiver::new(payload.len() + MAX_PAYLOAD_PER_DATAGRAM);
        let mut result = None;
        for dg in &datagrams {
            if let (Some(out), _) = receiver.ingest(dg) {
                result = Some(out.to_vec());
            }
        }
        result.expect("never assembled")
    }

    #[test]
    fn single_fragment() {
        let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        assert_eq!(round_trip(&data), data);
    }

    #[test]
    fn multi_fragment() {
        let data: Vec<u8> = (0..MAX_PAYLOAD_PER_DATAGRAM * 3 + 500)
            .map(|i| (i % 251) as u8)
            .collect();
        assert_eq!(round_trip(&data), data);
    }

    #[test]
    fn out_of_order() {
        let payload: Vec<u8> = (0..MAX_PAYLOAD_PER_DATAGRAM * 2 + 1)
            .map(|i| (i % 199) as u8)
            .collect();

        let mut sender = Sender::new();
        let mut datagrams = Vec::new();
        sender
            .send(&payload, 0, PAGE_GRAPHICS, |d| {
                datagrams.push(d.to_vec());
                Ok(())
            })
            .unwrap();

        let mut receiver = Receiver::new(payload.len() + MAX_PAYLOAD_PER_DATAGRAM);
        let mut result = None;
        let last = datagrams.pop().unwrap();
        if let (Some(out), _) = receiver.ingest(&last) {
            result = Some(out.to_vec());
        }
        for dg in &datagrams {
            if let (Some(out), _) = receiver.ingest(dg) {
                result = Some(out.to_vec());
            }
        }
        assert_eq!(result.unwrap(), payload);
    }

    #[test]
    fn source_timestamp_preserved() {
        let data = vec![0u8; 100];
        let mut sender = Sender::new();
        let mut datagrams = Vec::new();
        sender
            .send(&data, 9999, PAGE_PHYSICS, |d| {
                datagrams.push(d.to_vec());
                Ok(())
            })
            .unwrap();

        let mut receiver = Receiver::new(1024);
        receiver.ingest(&datagrams[0]);
        assert_eq!(receiver.last_source_us, 9999);
    }

    #[test]
    fn buf_offset_preserved() {
        let data = vec![1u8; 500];
        for &page in &[PAGE_PHYSICS, PAGE_GRAPHICS, PAGE_STATIC] {
            let mut sender = Sender::new();
            let mut datagrams = Vec::new();
            sender
                .send(&data, 0, page, |d| {
                    datagrams.push(d.to_vec());
                    Ok(())
                })
                .unwrap();

            let mut receiver = Receiver::new(1024);
            let (assembled, _) = receiver.ingest(&datagrams[0]);
            assert!(assembled.is_some());
            assert_eq!(receiver.last_buf_offset, page);
        }
    }

    #[test]
    fn heartbeat_not_assembled() {
        let sender = Sender::new();
        let mut sent = Vec::new();
        sender
            .send_heartbeat(12345, |d| {
                sent.push(d.to_vec());
                Ok(())
            })
            .unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].len(), HEADER_SIZE);

        let mut receiver = Receiver::new(1024);
        let (assembled, _) = receiver.ingest(&sent[0]);
        assert!(assembled.is_none());
        assert_eq!(receiver.last_buf_offset, PAGE_HEARTBEAT);
    }
}
