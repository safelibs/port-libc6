use std::cell::RefCell;
use std::collections::BTreeSet;

thread_local! {
    static SIGNAL_MASK: RefCell<BTreeSet<i32>> = RefCell::new(BTreeSet::new());
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SignalMaskSnapshot {
    blocked: BTreeSet<i32>,
}

impl SignalMaskSnapshot {
    pub fn contains(&self, signal: i32) -> bool {
        self.blocked.contains(&signal)
    }

    pub fn blocked_signals(&self) -> impl Iterator<Item = i32> + '_ {
        self.blocked.iter().copied()
    }
}

pub fn current_mask() -> SignalMaskSnapshot {
    SIGNAL_MASK.with(|mask| SignalMaskSnapshot {
        blocked: mask.borrow().clone(),
    })
}

pub fn replace_mask(signals: impl IntoIterator<Item = i32>) -> SignalMaskSnapshot {
    let next = signals.into_iter().collect::<BTreeSet<_>>();
    SIGNAL_MASK.with(|mask| {
        *mask.borrow_mut() = next.clone();
    });
    SignalMaskSnapshot { blocked: next }
}

pub fn block_signal(signal: i32) -> SignalMaskSnapshot {
    SIGNAL_MASK.with(|mask| {
        let mut blocked = mask.borrow_mut();
        blocked.insert(signal);
        SignalMaskSnapshot {
            blocked: blocked.clone(),
        }
    })
}

pub fn unblock_signal(signal: i32) -> SignalMaskSnapshot {
    SIGNAL_MASK.with(|mask| {
        let mut blocked = mask.borrow_mut();
        blocked.remove(&signal);
        SignalMaskSnapshot {
            blocked: blocked.clone(),
        }
    })
}
