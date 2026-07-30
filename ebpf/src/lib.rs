// SPDX-License-Identifier: 0BSD

#![no_std]

pub const MAX_INSTRUCTIONS: usize = 4096;
const STACK_SIZE: usize = 512;
const REGISTER_COUNT: usize = 11;
const UNREACHED: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Instruction {
    pub opcode: u8,
    pub registers: u8,
    pub offset: i16,
    pub immediate: i32,
}

impl Instruction {
    pub const fn new(opcode: u8, destination: u8, source: u8, offset: i16, immediate: i32) -> Self {
        Self {
            opcode,
            registers: (source << 4) | (destination & 0x0f),
            offset,
            immediate,
        }
    }

    pub const fn destination(self) -> usize {
        (self.registers & 0x0f) as usize
    }

    pub const fn source(self) -> usize {
        (self.registers >> 4) as usize
    }

    pub const fn decode(bytes: [u8; 8]) -> Self {
        Self {
            opcode: bytes[0],
            registers: bytes[1],
            offset: i16::from_le_bytes([bytes[2], bytes[3]]),
            immediate: i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifyError {
    EmptyProgram,
    ProgramTooLarge,
    InvalidOpcode { pc: usize, opcode: u8 },
    InvalidRegister { pc: usize, register: usize },
    FramePointerWrite { pc: usize },
    UninitializedRegister { pc: usize, register: usize },
    InvalidJump { pc: usize },
    BackwardJump { pc: usize },
    JumpIntoWideImmediate { pc: usize },
    TruncatedWideImmediate { pc: usize },
    InvalidWideImmediate { pc: usize },
    InvalidHelper { pc: usize, helper: i32 },
    InvalidStackBase { pc: usize },
    StackOutOfBounds { pc: usize },
    ReachableFallthrough { pc: usize },
    MissingExit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    DivisionByZero { pc: usize },
    InvalidHelper { pc: usize, helper: i32 },
    InvalidStackAccess { pc: usize },
    StepLimit,
}

pub struct VerifiedProgram<'a> {
    instructions: &'a [Instruction],
}

impl VerifiedProgram<'_> {
    pub const fn len(&self) -> usize {
        self.instructions.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }
}

pub trait HelperRegistry {
    fn call(&mut self, helper: i32, registers: &[u64; REGISTER_COUNT]) -> Option<u64>;
}

pub struct NoHelpers;

impl HelperRegistry for NoHelpers {
    fn call(&mut self, _helper: i32, _registers: &[u64; REGISTER_COUNT]) -> Option<u64> {
        None
    }
}

pub fn verify<'a>(
    instructions: &'a [Instruction],
    allowed_helpers: &[i32],
) -> Result<VerifiedProgram<'a>, VerifyError> {
    if instructions.is_empty() {
        return Err(VerifyError::EmptyProgram);
    }
    if instructions.len() > MAX_INSTRUCTIONS {
        return Err(VerifyError::ProgramTooLarge);
    }

    let mut wide_second = [false; MAX_INSTRUCTIONS];
    let mut pc = 0;
    while pc < instructions.len() {
        let instruction = instructions[pc];
        validate_register_numbers(pc, instruction)?;
        if instruction.opcode == 0x18 {
            if pc + 1 >= instructions.len() {
                return Err(VerifyError::TruncatedWideImmediate { pc });
            }
            let continuation = instructions[pc + 1];
            if instruction.source() != 0
                || continuation.opcode != 0
                || continuation.registers != 0
                || continuation.offset != 0
            {
                return Err(VerifyError::InvalidWideImmediate { pc });
            }
            wide_second[pc + 1] = true;
            pc += 2;
        } else {
            if !is_supported_opcode(instruction.opcode) {
                return Err(VerifyError::InvalidOpcode {
                    pc,
                    opcode: instruction.opcode,
                });
            }
            pc += 1;
        }
    }

    let mut incoming = [UNREACHED; MAX_INSTRUCTIONS];
    incoming[0] = (1 << 1) | (1 << 10);
    let mut saw_exit = false;

    for pc in 0..instructions.len() {
        let initialized = incoming[pc];
        if initialized == UNREACHED || wide_second[pc] {
            continue;
        }
        let instruction = instructions[pc];
        let destination = instruction.destination();
        let source = instruction.source();
        let mut outgoing = initialized;

        match instruction.opcode {
            0x18 => {
                ensure_writable_destination(pc, destination)?;
                outgoing |= 1 << destination;
                propagate(&mut incoming, instructions.len(), pc + 2, outgoing, pc)?;
            }
            0xb7 => {
                ensure_writable_destination(pc, destination)?;
                outgoing |= 1 << destination;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            0xbf => {
                ensure_writable_destination(pc, destination)?;
                require_initialized(initialized, pc, source)?;
                outgoing |= 1 << destination;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            opcode if is_alu_immediate(opcode) => {
                ensure_writable_destination(pc, destination)?;
                require_initialized(initialized, pc, destination)?;
                outgoing |= 1 << destination;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            opcode if is_alu_register(opcode) => {
                ensure_writable_destination(pc, destination)?;
                require_initialized(initialized, pc, destination)?;
                require_initialized(initialized, pc, source)?;
                outgoing |= 1 << destination;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            0x87 => {
                ensure_writable_destination(pc, destination)?;
                require_initialized(initialized, pc, destination)?;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            opcode if is_stack_load(opcode) => {
                ensure_writable_destination(pc, destination)?;
                require_stack_access(pc, source, instruction.offset, access_size(opcode))?;
                outgoing |= 1 << destination;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            opcode if is_stack_store_immediate(opcode) => {
                require_stack_access(pc, destination, instruction.offset, access_size(opcode))?;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            opcode if is_stack_store_register(opcode) => {
                require_stack_access(pc, destination, instruction.offset, access_size(opcode))?;
                require_initialized(initialized, pc, source)?;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            0x05 => {
                let target = jump_target(instructions.len(), pc, instruction.offset)?;
                if wide_second[target] {
                    return Err(VerifyError::JumpIntoWideImmediate { pc });
                }
                propagate(&mut incoming, instructions.len(), target, outgoing, pc)?;
            }
            opcode if is_conditional_jump_immediate(opcode) => {
                require_initialized(initialized, pc, destination)?;
                propagate_branch(
                    &mut incoming,
                    &wide_second,
                    instructions.len(),
                    pc,
                    instruction.offset,
                    outgoing,
                )?;
            }
            opcode if is_conditional_jump_register(opcode) => {
                require_initialized(initialized, pc, destination)?;
                require_initialized(initialized, pc, source)?;
                propagate_branch(
                    &mut incoming,
                    &wide_second,
                    instructions.len(),
                    pc,
                    instruction.offset,
                    outgoing,
                )?;
            }
            0x85 => {
                if !allowed_helpers.contains(&instruction.immediate) {
                    return Err(VerifyError::InvalidHelper {
                        pc,
                        helper: instruction.immediate,
                    });
                }
                outgoing &= !0x3e;
                outgoing |= 1;
                propagate_next(&mut incoming, instructions.len(), pc, outgoing)?;
            }
            0x95 => {
                require_initialized(initialized, pc, 0)?;
                saw_exit = true;
            }
            opcode => {
                return Err(VerifyError::InvalidOpcode { pc, opcode });
            }
        }
    }

    if !saw_exit {
        return Err(VerifyError::MissingExit);
    }
    Ok(VerifiedProgram { instructions })
}

pub fn execute(
    program: &VerifiedProgram<'_>,
    helpers: &mut impl HelperRegistry,
    context: u64,
) -> Result<u64, RuntimeError> {
    let mut registers = [0u64; REGISTER_COUNT];
    let mut stack = [0u8; STACK_SIZE];
    registers[1] = context;
    registers[10] = STACK_SIZE as u64;
    let mut pc = 0usize;
    let mut steps = 0usize;

    while pc < program.instructions.len() {
        if steps >= MAX_INSTRUCTIONS {
            return Err(RuntimeError::StepLimit);
        }
        steps += 1;
        let instruction = program.instructions[pc];
        let destination = instruction.destination();
        let source = instruction.source();
        let immediate = instruction.immediate as i64 as u64;

        match instruction.opcode {
            0x18 => {
                let high = program.instructions[pc + 1].immediate as u32 as u64;
                registers[destination] = (instruction.immediate as u32 as u64) | (high << 32);
                pc += 2;
            }
            0xb7 => {
                registers[destination] = immediate;
                pc += 1;
            }
            0xbf => {
                registers[destination] = registers[source];
                pc += 1;
            }
            opcode if is_alu_immediate(opcode) => {
                apply_alu(opcode, &mut registers[destination], immediate, pc)?;
                pc += 1;
            }
            opcode if is_alu_register(opcode) => {
                let rhs = registers[source];
                apply_alu(opcode, &mut registers[destination], rhs, pc)?;
                pc += 1;
            }
            0x87 => {
                registers[destination] = registers[destination].wrapping_neg();
                pc += 1;
            }
            opcode if is_stack_load(opcode) => {
                let size = access_size(opcode);
                let start = stack_address(instruction.offset, size, pc)?;
                registers[destination] = load_stack(&stack, start, size);
                pc += 1;
            }
            opcode if is_stack_store_immediate(opcode) => {
                let size = access_size(opcode);
                let start = stack_address(instruction.offset, size, pc)?;
                store_stack(&mut stack, start, size, immediate);
                pc += 1;
            }
            opcode if is_stack_store_register(opcode) => {
                let size = access_size(opcode);
                let start = stack_address(instruction.offset, size, pc)?;
                store_stack(&mut stack, start, size, registers[source]);
                pc += 1;
            }
            0x05 => {
                pc = branch_pc(pc, instruction.offset);
            }
            opcode if is_conditional_jump_immediate(opcode) => {
                if compare(opcode, registers[destination], immediate) {
                    pc = branch_pc(pc, instruction.offset);
                } else {
                    pc += 1;
                }
            }
            opcode if is_conditional_jump_register(opcode) => {
                if compare(opcode, registers[destination], registers[source]) {
                    pc = branch_pc(pc, instruction.offset);
                } else {
                    pc += 1;
                }
            }
            0x85 => {
                registers[0] = helpers.call(instruction.immediate, &registers).ok_or(
                    RuntimeError::InvalidHelper {
                        pc,
                        helper: instruction.immediate,
                    },
                )?;
                pc += 1;
            }
            0x95 => return Ok(registers[0]),
            _ => unreachable!("verified opcode"),
        }
    }
    Err(RuntimeError::StepLimit)
}

fn validate_register_numbers(pc: usize, instruction: Instruction) -> Result<(), VerifyError> {
    if instruction.destination() >= REGISTER_COUNT {
        return Err(VerifyError::InvalidRegister {
            pc,
            register: instruction.destination(),
        });
    }
    if instruction.source() >= REGISTER_COUNT {
        return Err(VerifyError::InvalidRegister {
            pc,
            register: instruction.source(),
        });
    }
    Ok(())
}

fn ensure_writable_destination(pc: usize, destination: usize) -> Result<(), VerifyError> {
    if destination == 10 {
        Err(VerifyError::FramePointerWrite { pc })
    } else {
        Ok(())
    }
}

fn require_initialized(initialized: u16, pc: usize, register: usize) -> Result<(), VerifyError> {
    if initialized & (1 << register) == 0 {
        Err(VerifyError::UninitializedRegister { pc, register })
    } else {
        Ok(())
    }
}

fn require_stack_access(
    pc: usize,
    base: usize,
    offset: i16,
    size: usize,
) -> Result<(), VerifyError> {
    if base != 10 {
        return Err(VerifyError::InvalidStackBase { pc });
    }
    let start = STACK_SIZE as isize + offset as isize;
    if start < 0 || start as usize + size > STACK_SIZE {
        Err(VerifyError::StackOutOfBounds { pc })
    } else {
        Ok(())
    }
}

fn propagate_next(
    incoming: &mut [u16; MAX_INSTRUCTIONS],
    length: usize,
    pc: usize,
    state: u16,
) -> Result<(), VerifyError> {
    propagate(incoming, length, pc + 1, state, pc)
}

fn propagate_branch(
    incoming: &mut [u16; MAX_INSTRUCTIONS],
    wide_second: &[bool; MAX_INSTRUCTIONS],
    length: usize,
    pc: usize,
    offset: i16,
    state: u16,
) -> Result<(), VerifyError> {
    let target = jump_target(length, pc, offset)?;
    if wide_second[target] {
        return Err(VerifyError::JumpIntoWideImmediate { pc });
    }
    propagate(incoming, length, pc + 1, state, pc)?;
    propagate(incoming, length, target, state, pc)
}

fn propagate(
    incoming: &mut [u16; MAX_INSTRUCTIONS],
    length: usize,
    target: usize,
    state: u16,
    source_pc: usize,
) -> Result<(), VerifyError> {
    if target >= length {
        return Err(VerifyError::ReachableFallthrough { pc: source_pc });
    }
    incoming[target] = if incoming[target] == UNREACHED {
        state
    } else {
        incoming[target] & state
    };
    Ok(())
}

fn jump_target(length: usize, pc: usize, offset: i16) -> Result<usize, VerifyError> {
    if offset < 0 {
        return Err(VerifyError::BackwardJump { pc });
    }
    let target = pc
        .checked_add(1)
        .and_then(|next| next.checked_add(offset as usize))
        .ok_or(VerifyError::InvalidJump { pc })?;
    if target >= length {
        Err(VerifyError::InvalidJump { pc })
    } else {
        Ok(target)
    }
}

fn branch_pc(pc: usize, offset: i16) -> usize {
    pc + 1 + offset as usize
}

fn apply_alu(opcode: u8, destination: &mut u64, rhs: u64, pc: usize) -> Result<(), RuntimeError> {
    *destination = match opcode & 0xf0 {
        0x00 => destination.wrapping_add(rhs),
        0x10 => destination.wrapping_sub(rhs),
        0x20 => destination.wrapping_mul(rhs),
        0x30 => {
            if rhs == 0 {
                return Err(RuntimeError::DivisionByZero { pc });
            }
            *destination / rhs
        }
        0x40 => *destination | rhs,
        0x50 => *destination & rhs,
        0x60 => destination.wrapping_shl((rhs & 63) as u32),
        0x70 => destination.wrapping_shr((rhs & 63) as u32),
        0x90 => {
            if rhs == 0 {
                return Err(RuntimeError::DivisionByZero { pc });
            }
            *destination % rhs
        }
        0xa0 => *destination ^ rhs,
        0xc0 => ((*destination as i64) >> (rhs & 63)) as u64,
        _ => unreachable!("verified ALU operation"),
    };
    Ok(())
}

fn compare(opcode: u8, lhs: u64, rhs: u64) -> bool {
    match opcode & 0xf0 {
        0x10 => lhs == rhs,
        0x20 => lhs > rhs,
        0x30 => lhs >= rhs,
        0x40 => lhs & rhs != 0,
        0x50 => lhs != rhs,
        0x60 => (lhs as i64) > (rhs as i64),
        0x70 => (lhs as i64) >= (rhs as i64),
        0xa0 => lhs < rhs,
        0xb0 => lhs <= rhs,
        0xc0 => (lhs as i64) < (rhs as i64),
        0xd0 => (lhs as i64) <= (rhs as i64),
        _ => false,
    }
}

fn stack_address(offset: i16, size: usize, pc: usize) -> Result<usize, RuntimeError> {
    let start = STACK_SIZE as isize + offset as isize;
    if start < 0 || start as usize + size > STACK_SIZE {
        Err(RuntimeError::InvalidStackAccess { pc })
    } else {
        Ok(start as usize)
    }
}

fn load_stack(stack: &[u8; STACK_SIZE], start: usize, size: usize) -> u64 {
    let mut value = 0u64;
    for index in 0..size {
        value |= u64::from(stack[start + index]) << (index * 8);
    }
    value
}

fn store_stack(stack: &mut [u8; STACK_SIZE], start: usize, size: usize, value: u64) {
    for index in 0..size {
        stack[start + index] = (value >> (index * 8)) as u8;
    }
}

const fn access_size(opcode: u8) -> usize {
    match opcode & 0x18 {
        0x00 => 4,
        0x08 => 2,
        0x10 => 1,
        0x18 => 8,
        _ => 0,
    }
}

const fn is_alu_immediate(opcode: u8) -> bool {
    matches!(
        opcode,
        0x07 | 0x17 | 0x27 | 0x37 | 0x47 | 0x57 | 0x67 | 0x77 | 0x97 | 0xa7 | 0xc7
    )
}

const fn is_alu_register(opcode: u8) -> bool {
    matches!(
        opcode,
        0x0f | 0x1f | 0x2f | 0x3f | 0x4f | 0x5f | 0x6f | 0x7f | 0x9f | 0xaf | 0xcf
    )
}

const fn is_conditional_jump_immediate(opcode: u8) -> bool {
    matches!(
        opcode,
        0x15 | 0x25 | 0x35 | 0x45 | 0x55 | 0x65 | 0x75 | 0xa5 | 0xb5 | 0xc5 | 0xd5
    )
}

const fn is_conditional_jump_register(opcode: u8) -> bool {
    matches!(
        opcode,
        0x1d | 0x2d | 0x3d | 0x4d | 0x5d | 0x6d | 0x7d | 0xad | 0xbd | 0xcd | 0xdd
    )
}

const fn is_stack_load(opcode: u8) -> bool {
    matches!(opcode, 0x61 | 0x69 | 0x71 | 0x79)
}

const fn is_stack_store_immediate(opcode: u8) -> bool {
    matches!(opcode, 0x62 | 0x6a | 0x72 | 0x7a)
}

const fn is_stack_store_register(opcode: u8) -> bool {
    matches!(opcode, 0x63 | 0x6b | 0x73 | 0x7b)
}

const fn is_supported_opcode(opcode: u8) -> bool {
    opcode == 0x18
        || opcode == 0xb7
        || opcode == 0xbf
        || opcode == 0x87
        || opcode == 0x05
        || opcode == 0x85
        || opcode == 0x95
        || is_alu_immediate(opcode)
        || is_alu_register(opcode)
        || is_conditional_jump_immediate(opcode)
        || is_conditional_jump_register(opcode)
        || is_stack_load(opcode)
        || is_stack_store_immediate(opcode)
        || is_stack_store_register(opcode)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn insn(
        opcode: u8,
        destination: u8,
        source: u8,
        offset: i16,
        immediate: i32,
    ) -> Instruction {
        Instruction::new(opcode, destination, source, offset, immediate)
    }

    #[test]
    fn returns_constant() {
        let program = [insn(0xb7, 0, 0, 0, 42), insn(0x95, 0, 0, 0, 0)];
        let verified = verify(&program, &[]).unwrap();
        assert_eq!(execute(&verified, &mut NoHelpers, 0), Ok(42));
    }

    #[test]
    fn decodes_little_endian_instruction_bytes() {
        assert_eq!(
            Instruction::decode([0x07, 0x12, 0xfe, 0xff, 0x78, 0x56, 0x34, 0x12]),
            insn(0x07, 2, 1, -2, 0x1234_5678)
        );
    }

    #[test]
    fn arithmetic_and_stack_round_trip() {
        let program = [
            insn(0xb7, 2, 0, 0, 20),
            insn(0x07, 2, 0, 0, 22),
            insn(0x7b, 10, 2, -8, 0),
            insn(0x79, 0, 10, -8, 0),
            insn(0x95, 0, 0, 0, 0),
        ];
        let verified = verify(&program, &[]).unwrap();
        assert_eq!(execute(&verified, &mut NoHelpers, 0), Ok(42));
    }

    #[test]
    fn rejects_uninitialized_source() {
        let program = [insn(0xbf, 0, 2, 0, 0), insn(0x95, 0, 0, 0, 0)];
        assert_eq!(
            verify(&program, &[]).err(),
            Some(VerifyError::UninitializedRegister { pc: 0, register: 2 })
        );
    }

    #[test]
    fn rejects_malformed_wide_immediate() {
        let program = [
            insn(0x18, 0, 0, 0, 42),
            insn(0xb7, 0, 0, 0, 0),
            insn(0x95, 0, 0, 0, 0),
        ];
        assert_eq!(
            verify(&program, &[]).err(),
            Some(VerifyError::InvalidWideImmediate { pc: 0 })
        );
    }

    #[test]
    fn helper_call_invalidates_argument_registers() {
        let program = [
            insn(0x85, 0, 0, 0, 7),
            insn(0xbf, 0, 1, 0, 0),
            insn(0x95, 0, 0, 0, 0),
        ];
        assert_eq!(
            verify(&program, &[7]).err(),
            Some(VerifyError::UninitializedRegister { pc: 1, register: 1 })
        );
    }

    #[test]
    fn rejects_backward_control_flow() {
        let program = [
            insn(0xb7, 0, 0, 0, 0),
            insn(0x05, 0, 0, -1, 0),
            insn(0x95, 0, 0, 0, 0),
        ];
        assert_eq!(
            verify(&program, &[]).err(),
            Some(VerifyError::BackwardJump { pc: 1 })
        );
    }

    #[test]
    fn rejects_stack_out_of_bounds() {
        let program = [
            insn(0x7a, 10, 0, -520, 1),
            insn(0xb7, 0, 0, 0, 0),
            insn(0x95, 0, 0, 0, 0),
        ];
        assert_eq!(
            verify(&program, &[]).err(),
            Some(VerifyError::StackOutOfBounds { pc: 0 })
        );
    }

    struct Helper;

    impl HelperRegistry for Helper {
        fn call(&mut self, helper: i32, _registers: &[u64; REGISTER_COUNT]) -> Option<u64> {
            (helper == 7).then_some(42)
        }
    }

    #[test]
    fn checks_and_invokes_helper_allowlist() {
        let program = [insn(0x85, 0, 0, 0, 7), insn(0x95, 0, 0, 0, 0)];
        assert!(matches!(
            verify(&program, &[]),
            Err(VerifyError::InvalidHelper { pc: 0, helper: 7 })
        ));
        let verified = verify(&program, &[7]).unwrap();
        assert_eq!(execute(&verified, &mut Helper, 0), Ok(42));
    }

    #[test]
    fn reports_runtime_division_by_zero() {
        let program = [
            insn(0xb7, 0, 0, 0, 42),
            insn(0x37, 0, 0, 0, 0),
            insn(0x95, 0, 0, 0, 0),
        ];
        let verified = verify(&program, &[]).unwrap();
        assert_eq!(
            execute(&verified, &mut NoHelpers, 0),
            Err(RuntimeError::DivisionByZero { pc: 1 })
        );
    }
}
