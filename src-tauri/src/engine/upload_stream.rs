//! 上传数据流:以固定粒度生成不可压缩随机数据,并逐块累计真实发送字节数。
//!
//! 设计要点:
//! - 数据源为随机池(避免传输层/服务器对重复数据的优化干扰测速)。
//! - 以 64KB 粒度产出子块并计数,使实时速度曲线平滑;取消时最多多计一个子块。

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::adapter::ThroughputCounters;
use tokio_util::sync::CancellationToken;

pub struct ChunkedBody {
    pool: Arc<Vec<u8>>,
    offset: usize,
    remaining: usize,
    granularity: usize,
    counters: Arc<ThroughputCounters>,
    token: CancellationToken,
    finished: bool,
}

impl ChunkedBody {
    pub fn new(
        pool: Vec<u8>,
        total_len: usize,
        counters: Arc<ThroughputCounters>,
        token: CancellationToken,
    ) -> Self {
        Self {
            pool: Arc::new(pool),
            offset: 0,
            remaining: total_len,
            granularity: 64 * 1024,
            counters,
            token,
            finished: false,
        }
    }

    fn next_slice(&mut self) -> Option<&[u8]> {
        if self.remaining == 0 || self.token.is_cancelled() {
            return None;
        }
        let take = self.granularity.min(self.remaining);
        let start = self.offset;
        let end = start + take;
        if end <= self.pool.len() {
            self.offset = end;
            self.remaining -= take;
            let slice = &self.pool[start..end];
            self.counters.add_bytes(slice.len() as u64);
            Some(slice)
        } else {
            // 跨池尾:先产出池尾剩余部分,下一拍回卷到开头
            let first = self.pool.len() - start;
            self.offset = 0;
            self.remaining -= first;
            let slice = &self.pool[start..];
            self.counters.add_bytes(slice.len() as u64);
            Some(slice)
        }
    }
}

impl futures_core::Stream for ChunkedBody {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }
        match self.next_slice() {
            Some(slice) => Poll::Ready(Some(Ok(bytes::Bytes::copy_from_slice(slice)))),
            None => {
                self.finished = true;
                Poll::Ready(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(body: &mut ChunkedBody) -> usize {
        let mut total = 0;
        while let Poll::Ready(Some(Ok(b))) =
            futures_core::Stream::poll_next(Pin::new(body), &mut Context::from_waker(
                std::task::Waker::noop(),
            ))
        {
            total += b.len();
        }
        total
    }

    #[test]
    fn test_chunked_body_counts_all_bytes() {
        let pool = vec![7u8; 1024 * 1024];
        let counters = Arc::new(ThroughputCounters::default());
        let mut body = ChunkedBody::new(pool, 3 * 1024 * 1024 + 12345, counters.clone(), CancellationToken::new());
        let total = collect(&mut body);
        assert_eq!(total, 3 * 1024 * 1024 + 12345);
        assert_eq!(counters.total(), 3 * 1024 * 1024 + 12345);
    }

    #[test]
    fn test_chunked_body_wraparound() {
        // 小池 + 大请求:验证跨池尾回卷不 panic 且计数准确
        let pool = vec![1u8; 4096];
        let counters = Arc::new(ThroughputCounters::default());
        let mut body = ChunkedBody::new(pool, 100_000, counters.clone(), CancellationToken::new());
        let total = collect(&mut body);
        assert_eq!(total, 100_000);
    }
}
