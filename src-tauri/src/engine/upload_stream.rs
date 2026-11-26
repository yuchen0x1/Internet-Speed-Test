//! 上传数据流:以固定粒度生成不可压缩随机数据,累计交给 HTTP 客户端的字节数。
//!
//! 设计要点:
//! - 数据源为随机池(避免传输层/服务器对重复数据的优化干扰测速)。
//! - 以 64KB 粒度产出子块;产出量不等于套接字写入量或远端接收量。

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
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
    count_immediately: bool,
    finished: bool,
    produced: Arc<AtomicU64>,
}

impl ChunkedBody {
    pub fn new(
        pool: Vec<u8>,
        total_len: usize,
        counters: Arc<ThroughputCounters>,
        token: CancellationToken,
    ) -> Self {
        Self::build(pool, total_len, counters, token, true)
    }

    pub fn new_untracked(
        pool: Vec<u8>,
        total_len: usize,
        counters: Arc<ThroughputCounters>,
        token: CancellationToken,
    ) -> Self {
        Self::build(pool, total_len, counters, token, false)
    }

    fn build(
        pool: Vec<u8>,
        total_len: usize,
        counters: Arc<ThroughputCounters>,
        token: CancellationToken,
        count_immediately: bool,
    ) -> Self {
        Self {
            pool: Arc::new(pool),
            offset: 0,
            remaining: total_len,
            granularity: 64 * 1024,
            counters,
            token,
            count_immediately,
            finished: false,
            produced: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn produced_bytes(&self) -> Arc<AtomicU64> {
        self.produced.clone()
    }

    fn next_slice(&mut self) -> Option<&[u8]> {
        if self.remaining == 0 || self.pool.is_empty() || self.token.is_cancelled() {
            return None;
        }
        let take = self.granularity.min(self.remaining);
        let start = self.offset;
        let end = start + take;
        if end <= self.pool.len() {
            self.offset = end;
            self.remaining -= take;
            let slice = &self.pool[start..end];
            self.produced
                .fetch_add(slice.len() as u64, Ordering::Relaxed);
            self.counters.add_sent(slice.len() as u64);
            if self.count_immediately {
                self.counters.add_bytes(slice.len() as u64);
            }
            Some(slice)
        } else {
            // 跨池尾:先产出池尾剩余部分,下一拍回卷到开头
            let first = self.pool.len() - start;
            self.offset = 0;
            self.remaining -= first;
            let slice = &self.pool[start..];
            self.produced
                .fetch_add(slice.len() as u64, Ordering::Relaxed);
            self.counters.add_sent(slice.len() as u64);
            if self.count_immediately {
                self.counters.add_bytes(slice.len() as u64);
            }
            Some(slice)
        }
    }
}

/// Require the entire body to have been produced before crediting a successful
/// HTTP response. This is an HTTP-level estimate, not remote byte telemetry;
/// producing the body alone does not prove that the peer received it in full.
pub fn commit_completed_upload(
    counters: &ThroughputCounters,
    produced: &AtomicU64,
    expected: usize,
) -> bool {
    if produced.load(Ordering::Relaxed) != expected as u64 {
        counters.add_failure();
        counters.set_phase_error("上传服务在请求体产出完成前返回成功，未计入该请求");
        return false;
    }
    counters.commit_accepted(expected as u64);
    true
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
        while let Poll::Ready(Some(Ok(b))) = futures_core::Stream::poll_next(
            Pin::new(body),
            &mut Context::from_waker(std::task::Waker::noop()),
        ) {
            total += b.len();
        }
        total
    }

    #[test]
    fn test_chunked_body_counts_all_bytes() {
        let pool = vec![7u8; 1024 * 1024];
        let counters = Arc::new(ThroughputCounters::default());
        let mut body = ChunkedBody::new(
            pool,
            3 * 1024 * 1024 + 12345,
            counters.clone(),
            CancellationToken::new(),
        );
        let total = collect(&mut body);
        assert_eq!(total, 3 * 1024 * 1024 + 12345);
        assert_eq!(counters.total(), 3 * 1024 * 1024 + 12345);
        assert_eq!(counters.sent(), 3 * 1024 * 1024 + 12345);
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

    #[test]
    fn test_untracked_body_counts_sent_without_claiming_acceptance() {
        let counters = Arc::new(ThroughputCounters::default());
        let mut body = ChunkedBody::new_untracked(
            vec![5u8; 1024],
            4096,
            counters.clone(),
            CancellationToken::new(),
        );

        assert_eq!(collect(&mut body), 4096);
        assert_eq!(counters.sent(), 4096);
        assert_eq!(counters.total(), 0);
        assert_eq!(counters.accepted(), 0);
    }

    #[test]
    fn early_success_does_not_credit_unproduced_upload_bytes() {
        let counters = Arc::new(ThroughputCounters::default());
        let mut body = ChunkedBody::new_untracked(
            vec![3u8; 1024],
            4096,
            counters.clone(),
            CancellationToken::new(),
        );
        let produced = body.produced_bytes();
        let _ = futures_core::Stream::poll_next(
            Pin::new(&mut body),
            &mut Context::from_waker(std::task::Waker::noop()),
        );
        assert_eq!(produced.load(Ordering::Relaxed), 1024);
        assert!(!commit_completed_upload(&counters, &produced, 4096));
        assert_eq!(counters.total(), 0);
        assert_eq!(counters.accepted(), 0);
        assert_eq!(counters.failure_count(), 1);
    }

    #[test]
    fn complete_success_credits_only_its_own_request() {
        let counters = Arc::new(ThroughputCounters::default());
        counters.add_sent(1_000_000);
        let mut body = ChunkedBody::new_untracked(
            vec![3u8; 1024],
            4096,
            counters.clone(),
            CancellationToken::new(),
        );
        let produced = body.produced_bytes();
        assert_eq!(collect(&mut body), 4096);
        assert!(commit_completed_upload(&counters, &produced, 4096));
        assert_eq!(counters.total(), 4096);
        assert_eq!(counters.accepted(), 4096);
    }

    #[test]
    fn empty_pool_finishes_instead_of_producing_empty_chunks_forever() {
        let counters = Arc::new(ThroughputCounters::default());
        let mut body =
            ChunkedBody::new(Vec::new(), 4096, counters.clone(), CancellationToken::new());
        assert_eq!(collect(&mut body), 0);
        assert_eq!(counters.sent(), 0);
    }
}
