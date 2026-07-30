// SPDX-License-Identifier: 0BSD

use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll};

static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn interrupt_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    crate::executor::wake_task(crate::executor::TIMER_TASK);
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Acquire)
}

pub async fn diagnostics_task() -> ! {
    let mut deadline = ticks() + 100;
    loop {
        SleepUntil { deadline }.await;
        let (virtio_interrupts, virtio_queue_interrupts) = crate::virtio::interrupt_counts();
        crate::serial::serialln(format_args!(
            "SLOPOS-ASYNC: timer future completed tick={} input_dropped={} virtio_interrupts={}/{}",
            ticks(),
            crate::ps2::dropped_bytes(),
            virtio_interrupts,
            virtio_queue_interrupts
        ));
        deadline = deadline.saturating_add(500);
    }
}

struct SleepUntil {
    deadline: u64,
}

impl Future for SleepUntil {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if ticks() >= self.deadline {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
