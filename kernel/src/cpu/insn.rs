// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) 2022-2023 SUSE LLC
//
// Author: Thomas Leroy <tleroy@suse.de>

use crate::cpu::vc::VcError;
use crate::cpu::vc::VcErrorType;
use crate::error::SvsmError;
use core::ops::{Index, IndexMut};

/// An immediate value in an instruction
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Immediate {
    U8(u8),
    U16(u16),
    U32(u32),
}

/// A register in an instruction
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Register {
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsp,
    Rbp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

/// An operand in an instruction, which might be a register or an immediate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operand {
    Reg(Register),
    Imm(Immediate),
}

impl Operand {
    #[inline]
    const fn rdx() -> Self {
        Self::Reg(Register::Rdx)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodedInsn {
    Cpuid,
    Inl(Operand),
    Inb(Operand),
    Inw(Operand),
    Outl(Operand),
    Outb(Operand),
    Outw(Operand),
}

impl DecodedInsn {
    pub const fn size(&self) -> usize {
        match self {
            Self::Cpuid => 2,
            Self::Inb(..) => 1,
            Self::Inw(..) => 2,
            Self::Inl(..) => 1,
            Self::Outb(..) => 1,
            Self::Outw(..) => 2,
            Self::Outl(..) => 1,
        }
    }
}

pub const MAX_INSN_SIZE: usize = 15;
pub const MAX_INSN_FIELD_SIZE: usize = 3;

/// A common structure shared by different fields of an
/// [`Instruction`] struct.
#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct InsnBuffer<const N: usize>
where
    [u8; N]: Default,
{
    /// Internal buffer of constant size `N`.
    pub buf: [u8; N],
    /// Number of useful bytes to be taken from `buf`.
    /// if `nb_bytes = 0`, the corresponding structure has
    /// no useful information. Otherwise, only `self.buf[..self.nb_bytes]`
    /// is useful.
    pub nb_bytes: usize,
}

impl<const N: usize> InsnBuffer<N>
where
    [u8; N]: Default,
{
    fn new(buf: [u8; N], nb_bytes: usize) -> Self {
        Self { buf, nb_bytes }
    }
}

impl<const N: usize> Index<usize> for InsnBuffer<N>
where
    [u8; N]: Default,
{
    type Output = u8;
    fn index(&self, i: usize) -> &Self::Output {
        &self.buf[i]
    }
}

impl<const N: usize> IndexMut<usize> for InsnBuffer<N>
where
    [u8; N]: Default,
{
    fn index_mut(&mut self, i: usize) -> &mut Self::Output {
        &mut self.buf[i]
    }
}

/// A view of an x86 instruction.
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct Instruction {
    /// Optional x86 instruction prefixes.
    pub prefixes: Option<InsnBuffer<MAX_INSN_FIELD_SIZE>>,
    /// Raw bytes copied from rip location.
    /// After decoding, `self.insn_bytes.nb_bytes` is adjusted
    /// to the total len of the instruction, prefix included.
    pub insn_bytes: InsnBuffer<MAX_INSN_SIZE>,
    /// Mandatory opcode.
    pub opcode: InsnBuffer<MAX_INSN_FIELD_SIZE>,
    /// Operand size in bytes.
    pub opnd_bytes: usize,
}

impl Instruction {
    pub fn new(insn_bytes: [u8; MAX_INSN_SIZE]) -> Self {
        Self {
            prefixes: None,
            opcode: InsnBuffer::default(), // we'll copy content later
            insn_bytes: InsnBuffer::new(insn_bytes, 0),
            opnd_bytes: 4,
        }
    }

    /// Returns the length of the instruction.
    ///
    /// # Returns:
    ///
    /// [`usize`]: The total size of an  instruction,
    /// prefix included.
    pub fn len(&self) -> usize {
        self.insn_bytes.nb_bytes
    }

    /// Returns true if the related [`Instruction`] can be considered empty.
    pub fn is_empty(&self) -> bool {
        self.insn_bytes.nb_bytes == 0
    }

    /// Decode the instruction.
    /// At the moment, the decoding is very naive since we only need to decode CPUID,
    /// IN and OUT (without strings and immediate usage) instructions. A complete decoding
    /// of the full x86 instruction set is still TODO.
    ///
    /// # Returns
    ///
    /// A [`DecodedInsn`] if the instruction is supported, or an [`SvsmError`] otherwise.
    pub fn decode(&self) -> Result<DecodedInsn, SvsmError> {
        match self.insn_bytes[0] {
            0xEC => return Ok(DecodedInsn::Inb(Operand::rdx())),
            0xED => return Ok(DecodedInsn::Inl(Operand::rdx())),
            0xEE => return Ok(DecodedInsn::Outb(Operand::rdx())),
            0xEF => return Ok(DecodedInsn::Outl(Operand::rdx())),
            0x66 => match self.insn_bytes[1] {
                0xED => return Ok(DecodedInsn::Inw(Operand::rdx())),
                0xEF => return Ok(DecodedInsn::Outw(Operand::rdx())),
                _ => (),
            },
            0x0F => {
                if self.insn_bytes[1] == 0xA2 {
                    return Ok(DecodedInsn::Cpuid);
                }
            }
            _ => (),
        }

        Err(VcError {
            rip: 0,
            code: 0,
            error_type: VcErrorType::DecodeFailed,
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_inw() {
        let raw_insn: [u8; MAX_INSN_SIZE] = [
            0x66, 0xED, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
            0x41,
        ];

        let insn = Instruction::new(raw_insn);
        let decoded = insn.decode().unwrap();
        assert_eq!(decoded, DecodedInsn::Inw(Operand::rdx()));
        assert_eq!(decoded.size(), 2);
    }

    #[test]
    fn test_decode_outb() {
        let raw_insn: [u8; MAX_INSN_SIZE] = [
            0xEE, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
            0x41,
        ];

        let insn = Instruction::new(raw_insn);
        let decoded = insn.decode().unwrap();
        assert_eq!(decoded, DecodedInsn::Outb(Operand::rdx()));
        assert_eq!(decoded.size(), 1);
    }

    #[test]
    fn test_decode_outl() {
        let raw_insn: [u8; MAX_INSN_SIZE] = [
            0xEF, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
            0x41,
        ];

        let insn = Instruction::new(raw_insn);
        let decoded = insn.decode().unwrap();
        assert_eq!(decoded, DecodedInsn::Outl(Operand::rdx()));
        assert_eq!(decoded.size(), 1);
    }

    #[test]
    fn test_decode_cpuid() {
        let raw_insn: [u8; MAX_INSN_SIZE] = [
            0x0F, 0xA2, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
            0x41,
        ];

        let insn = Instruction::new(raw_insn);
        let decoded = insn.decode().unwrap();
        assert_eq!(decoded, DecodedInsn::Cpuid);
        assert_eq!(decoded.size(), 2);
    }

    #[test]
    fn test_decode_failed() {
        let raw_insn: [u8; MAX_INSN_SIZE] = [
            0x66, 0xEE, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
            0x41,
        ];

        let insn = Instruction::new(raw_insn);
        let err = insn.decode();

        assert!(err.is_err());
    }
}
