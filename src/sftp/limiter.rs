use std::time::{Duration, Instant};
use tokio::sync::Mutex;

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// 令牌桶限速器：按字节匀速放行，0 表示不限速。
/// 允许 0.25 秒的突发额度，避免 TCP 往返延迟拉低实际吞吐。
pub struct RateLimiter {
    bytes_per_sec: u64,
    burst_bytes: f64,
    state: Mutex<Bucket>,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        let burst_bytes = (bytes_per_sec as f64 * 0.25).max(64.0 * 1024.0);
        Self {
            bytes_per_sec,
            burst_bytes,
            state: Mutex::new(Bucket {
                tokens: burst_bytes,
                last: Instant::now(),
            }),
        }
    }

    /// 申请传输 `bytes` 字节，必要时异步等待，不阻塞 tokio runtime
    pub async fn throttle(&self, bytes: u64) {
        if self.bytes_per_sec == 0 || bytes == 0 {
            return;
        }
        let wait = {
            let mut st = self.state.lock().await;
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(st.last).as_secs_f64();
            st.last = now;
            st.tokens =
                (st.tokens + elapsed * self.bytes_per_sec as f64).min(self.burst_bytes);
            st.tokens -= bytes as f64;
            if st.tokens >= 0.0 {
                0.0
            } else {
                -st.tokens / self.bytes_per_sec as f64
            }
        };
        if wait > 0.0 {
            tokio::time::sleep(Duration::from_secs_f64(wait)).await;
        }
    }
}
