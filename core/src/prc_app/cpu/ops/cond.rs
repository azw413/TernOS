pub fn sr_cond_true(sr: u16, cond: u8) -> bool {
    let c = (sr & 0x0001) != 0;
    let v = (sr & 0x0002) != 0;
    let z = (sr & 0x0004) != 0;
    let n = (sr & 0x0008) != 0;
    match cond {
        0x0 => true,               // T
        0x1 => false,              // F
        0x2 => !c && !z,           // HI
        0x3 => c || z,             // LS
        0x4 => !c,                 // CC
        0x5 => c,                  // CS
        0x6 => !z,                 // NE
        0x7 => z,                  // EQ
        0x8 => !v,                 // VC
        0x9 => v,                  // VS
        0xA => !n,                 // PL
        0xB => n,                  // MI
        0xC => n == v,             // GE
        0xD => n != v,             // LT
        0xE => !z && (n == v),     // GT
        0xF => z || (n != v),      // LE
        _ => false,
    }
}

pub fn target_pc(base_pc: u32, disp: i32) -> Option<u32> {
    let target = (base_pc as i64) + (disp as i64);
    if target < 0 {
        None
    } else {
        Some(target as u32)
    }
}

