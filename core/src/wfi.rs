// WFI wakeup primitive: Condvar-based notification for
// halted CPU wakeup by device IRQ or manager stop.
//
// All state is protected by a single mutex to prevent
// lost-wakeup races between wake/stop and wait.

use std::sync::{Condvar, Mutex};

/// Internal state guarded by the mutex.
struct WfiState {
    /// Set by wake() when device IRQ arrives.
    irq_pending: bool,
    /// Set by stop() for manager shutdown.
    stopped: bool,
}

/// Wakeup signal for WFI (Wait For Interrupt).
///
/// - Device IRQ sinks call `wake()` to unblock WFI.
/// - CpuManager calls `stop()` to force-unblock WFI
///   for safe shutdown.
/// - `wait()` blocks until either condition is met.
///
/// All three methods acquire the same mutex, so there
/// is no window for lost wakeups.
pub struct WfiWaker {
    state: Mutex<WfiState>,
    cv: Condvar,
}

impl WfiWaker {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(WfiState {
                irq_pending: false,
                stopped: false,
            }),
            cv: Condvar::new(),
        }
    }

    /// Wake halted CPU (device IRQ arrived).
    pub fn wake(&self) {
        let mut s = self.state.lock().unwrap();
        s.irq_pending = true;
        self.cv.notify_all();
    }

    /// Force-unblock any waiting CPU (manager stop).
    pub fn stop(&self) {
        let mut s = self.state.lock().unwrap();
        s.stopped = true;
        self.cv.notify_all();
    }

    /// Block until woken by `wake()` or `stop()`.
    /// Returns true if woken by IRQ, false if stopped.
    pub fn wait(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        loop {
            if s.irq_pending {
                s.irq_pending = false;
                return true;
            }
            if s.stopped {
                return false;
            }
            s = self.cv.wait(s).unwrap();
        }
    }
}

impl Default for WfiWaker {
    fn default() -> Self {
        Self::new()
    }
}
