pub mod dm;
pub mod evt;
pub mod fnt;
pub mod frm;
pub mod mem;
pub mod sys;
pub mod table;
pub mod win;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapGroup {
    Mem,
    Dm,
    Sys,
    Evt,
    Fld,
    Frm,
    Lst,
    Win,
    Menu,
    Tim,
    Str,
    Snd,
    Fnt,
    Lib,
    Unknown,
}

impl TrapGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            TrapGroup::Mem => "mem",
            TrapGroup::Dm => "dm",
            TrapGroup::Sys => "sys",
            TrapGroup::Evt => "evt",
            TrapGroup::Fld => "fld",
            TrapGroup::Frm => "frm",
            TrapGroup::Lst => "lst",
            TrapGroup::Win => "win",
            TrapGroup::Menu => "menu",
            TrapGroup::Tim => "tim",
            TrapGroup::Str => "str",
            TrapGroup::Snd => "snd",
            TrapGroup::Fnt => "fnt",
            TrapGroup::Lib => "lib",
            TrapGroup::Unknown => "unknown",
        }
    }
}
