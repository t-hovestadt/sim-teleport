use std::time::{Duration, Instant};

const PRINT_INTERVAL: Duration = Duration::from_secs(5);

pub struct RelayStats {
    window_start: Instant,
    window_packets: u64,
    window_bytes: u64,
    window_latency_us: u64,
    pub total_packets: u64,
    pub total_bytes: u64,
}

impl RelayStats {
    pub fn new() -> Self {
        Self {
            window_start: Instant::now(),
            window_packets: 0,
            window_bytes: 0,
            window_latency_us: 0,
            total_packets: 0,
            total_bytes: 0,
        }
    }
}

impl Default for RelayStats {
    fn default() -> Self {
        Self::new()
    }
}

impl RelayStats {
    pub fn record(&mut self, bytes: usize, fwd_us: u64) {
        self.window_packets += 1;
        self.window_bytes += bytes as u64;
        self.window_latency_us += fwd_us;
        self.total_packets += 1;
        self.total_bytes += bytes as u64;
    }

    /// Print a one-line stats summary if the print interval has elapsed.
    pub fn maybe_print(&mut self, name: &str, active: bool) {
        let elapsed = self.window_start.elapsed();
        if elapsed < PRINT_INTERVAL {
            return;
        }
        if !active || self.window_packets == 0 {
            println!("[{name}]  inactive");
        } else {
            let secs = elapsed.as_secs_f64();
            let pkt_s = self.window_packets as f64 / secs;
            let kb_s = self.window_bytes as f64 / 1024.0 / secs;
            let avg_b = self.window_bytes / self.window_packets;
            let avg_fwd_us = self.window_latency_us / self.window_packets;
            println!(
                "[{name}]  {pkt_s:.1} pkt/s   {kb_s:.1} KB/s   avg {avg_b} b/pkt   {avg_fwd_us} \u{b5}s fwd"
            );
        }
        self.window_packets = 0;
        self.window_bytes = 0;
        self.window_latency_us = 0;
        self.window_start = Instant::now();
    }

    pub fn print_summary(&self, name: &str, elapsed: Duration) {
        let secs = elapsed.as_secs_f64();
        if self.total_packets == 0 {
            println!("[{name}] summary: {secs:.0}s  0 packets");
            return;
        }
        let mb = self.total_bytes as f64 / 1_048_576.0;
        let avg_b = self.total_bytes / self.total_packets;
        println!(
            "[{name}] summary: {secs:.0}s  {} packets  {mb:.1} MB  avg {avg_b} b/pkt",
            self.total_packets
        );
    }
}
