// SPDX-License-Identifier: 0BSD

use core::arch::asm;
use core::future::Future;
use core::pin::{Pin, pin};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::task::{Context, RawWaker, RawWakerVTable, Waker};

pub const INPUT_TASK: usize = 0;
pub const TIMER_TASK: usize = 1;
const TASK_COUNT: usize = 2;
const ALL_TASKS: usize = (1 << TASK_COUNT) - 1;

static READY_TASKS: AtomicUsize = AtomicUsize::new(0);

pub fn wake_task(task_id: usize) {
    if task_id < TASK_COUNT {
        READY_TASKS.fetch_or(1 << task_id, Ordering::Release);
    }
}

pub fn run<F0, F1>(input_task: F0, timer_task: F1) -> !
where
    F0: Future,
    F1: Future,
{
    let mut input_task = pin!(input_task);
    let mut timer_task = pin!(timer_task);
    READY_TASKS.store(ALL_TASKS, Ordering::Release);
    crate::serial::serialln(format_args!(
        "SLOPOS-ASYNC: executor entered tasks={TASK_COUNT}"
    ));
    crate::interrupts::enable();

    loop {
        let ready = READY_TASKS.swap(0, Ordering::AcqRel);
        if ready & (1 << INPUT_TASK) != 0 {
            poll_task(INPUT_TASK, input_task.as_mut());
        }
        if ready & (1 << TIMER_TASK) != 0 {
            poll_task(TIMER_TASK, timer_task.as_mut());
        }
        wait_for_work();
    }
}

fn poll_task<F: Future>(task_id: usize, mut future: Pin<&mut F>) {
    let waker = task_waker(task_id);
    let mut context = Context::from_waker(&waker);
    if future.as_mut().poll(&mut context).is_ready() {
        crate::serial::serialln(format_args!(
            "SLOPOS-ASYNC: task unexpectedly completed id={task_id}"
        ));
    }
}

fn wait_for_work() {
    // Disable interrupts across the ready check and arm HLT atomically with STI.
    // An IRQ that arrives after STI wakes HLT; queued work skips HLT entirely.
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };
    if READY_TASKS.load(Ordering::Acquire) == 0 {
        // SAFETY: the IDT and unmasked IRQ handlers are installed before executor entry.
        unsafe { asm!("sti; hlt", options(nomem, nostack)) };
    } else {
        // SAFETY: restore normal interrupt delivery before polling more work.
        unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
    }
}

fn task_waker(task_id: usize) -> Waker {
    let data = (task_id + 1) as *const ();
    // SAFETY: the vtable never dereferences data; it only decodes the integer task ID.
    unsafe { Waker::from_raw(RawWaker::new(data, &WAKER_VTABLE)) }
}

unsafe fn waker_clone(data: *const ()) -> RawWaker {
    RawWaker::new(data, &WAKER_VTABLE)
}

unsafe fn waker_wake(data: *const ()) {
    wake_task(data as usize - 1);
}

unsafe fn waker_wake_by_ref(data: *const ()) {
    wake_task(data as usize - 1);
}

unsafe fn waker_drop(_data: *const ()) {}

static WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(waker_clone, waker_wake, waker_wake_by_ref, waker_drop);
