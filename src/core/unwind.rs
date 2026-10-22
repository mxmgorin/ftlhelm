//! The unwinder entry points std links against, defined here so the library needs no
//! libgcc_s: box64 has no x86_64 copy of it to load. With panic = abort nothing unwinds,
//! and a backtrace walk reports an empty stack. The symbols are hidden, so the game's own
//! unwinder never binds to them.

std::arch::global_asm!(
    ".macro stub name",
    ".globl \\name",
    ".hidden \\name",
    ".type \\name, @function",
    "\\name:",
    ".endm",
    // _URC_END_OF_STACK: the walk found no frames.
    "stub _Unwind_Backtrace",
    "mov eax, 5",
    "ret",
    // Nothing raises, so nothing resumes.
    "stub _Unwind_Resume",
    "ud2",
    "stub _Unwind_GetIPInfo",
    "test rsi, rsi",
    "jz 2f",
    "mov dword ptr [rsi], 0",
    "2:",
    "xor eax, eax",
    "ret",
    "stub _Unwind_GetIP",
    "stub _Unwind_GetDataRelBase",
    "stub _Unwind_GetTextRelBase",
    "stub _Unwind_GetLanguageSpecificData",
    "stub _Unwind_GetRegionStart",
    "xor eax, eax",
    "ret",
    "stub _Unwind_SetGR",
    "stub _Unwind_SetIP",
    "ret",
);
