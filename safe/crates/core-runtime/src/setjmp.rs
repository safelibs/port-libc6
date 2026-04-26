use crate::tls;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JumpContext {
    pub saves_signal_mask: bool,
    pub pointer_guard: u64,
}

pub fn capture(saves_signal_mask: bool) -> JumpContext {
    JumpContext {
        saves_signal_mask,
        pointer_guard: tls::pointer_guard(),
    }
}

pub fn validate(context: &JumpContext) -> bool {
    context.pointer_guard == tls::pointer_guard()
}
