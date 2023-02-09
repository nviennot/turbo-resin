// SPDX-License-Identifier: GPL-3.0-or-later


use futures::Future;
use futures::FutureExt;

use embassy_util::channel::signal::Signal;
use core::cell::Cell;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;

/// TaskRunner runs in a different embassy task a given async task `T`
pub struct TaskRunner<T: CancellableTask + Copy + Send> {
    task_signal: Signal<()>,
    cancel_signal: Signal<()>,
    task: Cell<Option<T>>,
    cancelled: Cell<bool>,
}


impl<T: CancellableTask + Copy + Send> Default for TaskRunner<T> {
    fn default() -> Self {
        Self {
            task_signal: Signal::new(),
            cancel_signal: Signal::new(),
            task: Default::default(),
            cancelled: Default::default(),
        }
    }
}

impl<T: CancellableTask + Copy + Send> TaskRunner<T> {
    #[inline]
    pub fn is_busy(&self) -> bool {
        self.get_current_task().is_some()
    }

    #[inline]
    pub fn is_task_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    #[inline]
    pub fn get_current_task(&self) -> Option<T> {
        self.task.get()
    }

    // This function must be called within in a lower interrupt context than the main_loop()
    // function. This way we don't need locks (is_busy() might not atomic otherwise).
    // Returns an error if we are already working on something.
    #[inline]
    pub fn enqueue_task(&self, task: T) -> Result<(),()> {
        if self.is_busy() {
            Err(())
        } else {
            self.cancelled.set(false);
            self.cancel_signal.reset();
            self.task.replace(Some(task));
            self.task_signal.signal(());
            Ok(())
        }
    }

    #[inline]
    pub fn cancel_task(&self) {
        self.cancelled.set(true);
        self.cancel_signal.signal(());
    }

    /// Must be called from a higher interrupt context than the calls going to enqueue_task().
    pub async fn main_loop_task(&self, ctx: &mut T::Context) {
        loop {
            self.task_signal.wait().await;
            let task = self.get_current_task().unwrap();

            debug!("Executing task: {:?}", task);

            let was_cancelled = futures::select_biased! {
                _ = task.run(ctx).fuse() => false,
                _ = self.cancel_signal.wait().fuse() => true,
            };

            if was_cancelled {
                task.cancel(ctx).await;
                debug!("Task cancelled");
            } else {
                debug!("Task complete");
            }

            // Clears up the task so that is_busy() returns false
            self.task.take();
            self.cancelled.set(false);
        }
    }
}

pub trait CancellableTask: Send + core::fmt::Debug {
    type Context;

    type RunFuture<'a>: Future<Output = ()> + 'a where Self: 'a;
    type CancelFuture<'a>: Future<Output = ()> + 'a where Self: 'a;

    /// The task to run
    // &mut self is not an option as we are sharing references in get_current_task()
    fn run<'a>(&'a self, ctx: &'a mut Self::Context) -> Self::RunFuture<'a>;
    /// What to do when cancelled.
    fn cancel<'a>(&'a self, ctx: &'a mut Self::Context) -> Self::CancelFuture<'a>;
}
