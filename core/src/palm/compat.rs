#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatLevel {
    Unsupported,
    ProbeOnly,
    Experimental,
}

#[derive(Clone, Debug)]
pub struct CompatReport {
    pub level: CompatLevel,
    pub notes: &'static str,
}
