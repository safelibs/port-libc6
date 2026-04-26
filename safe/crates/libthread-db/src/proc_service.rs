pub const PROC_SERVICE_HEADER_PATH: &str = "/usr/include/proc_service.h";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcServiceOperation {
    ReadMemory,
    WriteMemory,
    LookupSymbol,
}

pub fn supported_operations() -> &'static [ProcServiceOperation] {
    &[
        ProcServiceOperation::ReadMemory,
        ProcServiceOperation::WriteMemory,
        ProcServiceOperation::LookupSymbol,
    ]
}
