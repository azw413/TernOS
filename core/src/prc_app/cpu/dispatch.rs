#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpcodeFamily {
    Branch,
    Quick,
    MulDiv,
    ATrap,
    Trap,
    Other,
}

#[derive(Clone, Copy)]
struct DispatchRule {
    mask: u16,
    value: u16,
    family: OpcodeFamily,
}

const RULES: &[DispatchRule] = &[
    // High-priority exact families first.
    DispatchRule {
        mask: 0xF000,
        value: 0xA000,
        family: OpcodeFamily::ATrap,
    },
    DispatchRule {
        mask: 0xFFF0,
        value: 0x4E40,
        family: OpcodeFamily::Trap,
    },
    // 0x6xxx: BRA/BSR/Bcc.
    DispatchRule {
        mask: 0xF000,
        value: 0x6000,
        family: OpcodeFamily::Branch,
    },
    // 0x5xxx: ADDQ/SUBQ/Scc/DBcc/TRAPcc.
    DispatchRule {
        mask: 0xF000,
        value: 0x5000,
        family: OpcodeFamily::Quick,
    },
    // DIVU/DIVS/MULU/MULS word forms.
    DispatchRule {
        mask: 0xF1C0,
        value: 0x80C0,
        family: OpcodeFamily::MulDiv,
    },
    DispatchRule {
        mask: 0xF1C0,
        value: 0x81C0,
        family: OpcodeFamily::MulDiv,
    },
    DispatchRule {
        mask: 0xF1C0,
        value: 0xC0C0,
        family: OpcodeFamily::MulDiv,
    },
    DispatchRule {
        mask: 0xF1C0,
        value: 0xC1C0,
        family: OpcodeFamily::MulDiv,
    },
];

pub fn classify(word: u16) -> OpcodeFamily {
    for rule in RULES {
        if (word & rule.mask) == rule.value {
            return rule.family;
        }
    }
    OpcodeFamily::Other
}
