// SPDX-License-Identifier: GPL-3.0-or-later


use futures::Future;
use futures::FutureExt;

use embassy::channel::signal::Signal;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;

use crate::{ui::Task, drivers::zaxis};

pub struct TaskRunner<T: Send> {
    task_signal: Signal<T>,
    cancel_signal: Signal<()>,
    current_task: Option<T>,
    cancelled: AtomicBool,
}

impl<T: Send> TaskRunner<T> {
    pub fn new() -> Self {
        Self {
            task_signal: Signal::new(),
            cancel_signal: Signal::new(),
            current_task: None,
            cancelled: AtomicBool::new(false),
        }
    }

    // These functions are used in a lower interrupt context than the
    // run_tasks() function.

    pub fn is_busy(&self) -> bool {
        self.current_task.is_some()
    }

    pub fn is_task_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn get_current_task(&self) -> Option<&T> {
        self.current_task.as_ref()
    }

    // Returns an error if we are already working on something.
    pub fn enqueue_task(&self, task: T) -> Result<(),()> {
        if self.is_busy() {
            Err(())
        } else {
            self.cancel_signal.reset();
            self.task_signal.signal(task);
            Ok(())
        }
    }

    pub fn cancel_task(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.cancel_signal.signal(());
    }

    pub async fn cancellable<R>(&self, fut: impl Future<Output=R>) -> Result<R, ()> {
        futures::select_biased! {
            _ = self.cancel_signal.wait().fuse() => Err(()),
            r = fut.fuse() => Ok(r),
        }
    }
}

impl TaskRunner<Task> {
    pub async fn run_tasks(&mut self, zaxis: &mut zaxis::MotionControlAsync) {
        loop {
            self.current_task = None;
            self.cancelled.store(false, Ordering::Release);

            let task = self.task_signal.wait().await;
            crate::debug!("Executing task: {:?}", task);
            self.current_task = Some(task);
            self.current_task.as_ref().unwrap().run(&self, zaxis).await;
            crate::debug!("Done executing task");
        }
    }
}
