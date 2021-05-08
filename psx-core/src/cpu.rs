mod instruction;
mod instructions_table;
mod register;

use crate::coprocessor::SystemControlCoprocessor;
use crate::memory::BusLine;

use instruction::{Instruction, Opcode};
use register::Registers;

pub struct Cpu {
    regs: Registers,
    cop0: SystemControlCoprocessor,

    jump_dest_next: Option<u32>,
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            // reset value
            regs: Registers::new(),
            cop0: SystemControlCoprocessor::default(),
            jump_dest_next: None,
        }
    }

    pub fn execute_next<P: BusLine>(&mut self, bus: &mut P) {
        let pc = self.regs.pc;
        let instruction = self.bus_read_u32(bus, self.regs.pc);
        let instruction = Instruction::from_u32(instruction);

        self.regs.pc += 4;

        if let Some(jump_dest) = self.jump_dest_next.take() {
            log::trace!("pc jump {:08X}", jump_dest);
            self.regs.pc = jump_dest;
        }

        log::trace!("{:08X}: {:02X?}", pc, instruction);
        self.execute_instruction(instruction, bus);
    }
}

impl Cpu {
    fn sign_extend_16(data: u16) -> u32 {
        data as i16 as i32 as u32
    }

    fn sign_extend_8(data: u8) -> u32 {
        data as i8 as i32 as u32
    }

    fn execute_alu_reg<F>(&mut self, instruction: Instruction, handler: F)
    where
        F: FnOnce(u32, u32) -> u32,
    {
        let rs = self.regs.read_register(instruction.rs);
        let rt = self.regs.read_register(instruction.rt);

        let result = handler(rs, rt);
        self.regs.write_register(instruction.rd, result);
    }

    fn execute_alu_imm<F>(&mut self, instruction: Instruction, handler: F)
    where
        F: FnOnce(u32, &Instruction) -> u32,
    {
        let rs = self.regs.read_register(instruction.rs);
        let result = handler(rs, &instruction);
        self.regs.write_register(instruction.rt, result);
    }

    fn execute_branch<F>(&mut self, instruction: Instruction, have_rt: bool, handler: F) -> bool
    where
        F: FnOnce(i32, i32) -> bool,
    {
        let rs = self.regs.read_register(instruction.rs) as i32;
        let rt = if have_rt {
            self.regs.read_register(instruction.rt) as i32
        } else {
            0
        };
        let signed_imm16 = Self::sign_extend_16(instruction.imm16).wrapping_mul(4);

        let should_jump = handler(rs, rt);
        if should_jump {
            self.jump_dest_next = Some(self.regs.pc.wrapping_add(signed_imm16));
        }

        should_jump
    }

    fn execute_load<F>(&mut self, instruction: Instruction, mut handler: F)
    where
        F: FnMut(&Self, u32) -> u32,
    {
        let rs = self.regs.read_register(instruction.rs);
        let computed_addr = rs.wrapping_add(Self::sign_extend_16(instruction.imm16));

        let data = handler(self, computed_addr);

        self.regs.write_register(instruction.rt, data);
    }

    fn execute_store<F>(&mut self, instruction: Instruction, mut handler: F)
    where
        F: FnMut(&Self, u32, u32),
    {
        let rs = self.regs.read_register(instruction.rs);
        let rt = self.regs.read_register(instruction.rt);
        let computed_addr = rs.wrapping_add(Self::sign_extend_16(instruction.imm16));

        handler(self, computed_addr, rt);
    }

    fn execute_instruction<P: BusLine>(&mut self, instruction: Instruction, bus: &mut P) {
        match instruction.opcode {
            Opcode::Lb => {
                self.execute_load(instruction, |s, computed_addr| {
                    s.bus_read_u8(bus, computed_addr) as u32
                });
            }
            Opcode::Lbu => {
                self.execute_load(instruction, |s, computed_addr| {
                    Self::sign_extend_8(s.bus_read_u8(bus, computed_addr))
                });
            }
            Opcode::Lh => {
                self.execute_load(instruction, |s, computed_addr| {
                    s.bus_read_u16(bus, computed_addr) as u32
                });
            }
            Opcode::Lhu => {
                self.execute_load(instruction, |s, computed_addr| {
                    Self::sign_extend_16(s.bus_read_u16(bus, computed_addr))
                });
            }
            Opcode::Lw => {
                self.execute_load(instruction, |s, computed_addr| {
                    s.bus_read_u32(bus, computed_addr)
                });
            }
            //Opcode::Lwl => {}
            //Opcode::Lwr => {}
            Opcode::Sb => {
                self.execute_store(instruction, |s, computed_addr, data| {
                    s.bus_write_u8(bus, computed_addr, data as u8)
                });
            }
            Opcode::Sh => {
                self.execute_store(instruction, |s, computed_addr, data| {
                    s.bus_write_u16(bus, computed_addr, data as u16)
                });
            }
            Opcode::Sw => {
                self.execute_store(instruction, |s, computed_addr, data| {
                    s.bus_write_u32(bus, computed_addr, data)
                });
            }
            //Opcode::Swl => {}
            //Opcode::Swr => {}
            Opcode::Slt => {
                let rs = self.regs.read_register(instruction.rs) as i32;
                let rt = self.regs.read_register(instruction.rt) as i32;

                self.regs.write_register(instruction.rd, (rs < rt) as u32);
            }
            Opcode::Sltu => {
                let rs = self.regs.read_register(instruction.rs);
                let rt = self.regs.read_register(instruction.rt);

                self.regs.write_register(instruction.rd, (rs < rt) as u32);
            }
            Opcode::Slti => {
                let rs = self.regs.read_register(instruction.rs) as i32;
                let imm = instruction.imm16 as i16 as i32;

                self.regs.write_register(instruction.rt, (rs < imm) as u32);
            }
            Opcode::Sltiu => {
                let rs = self.regs.read_register(instruction.rs);
                let imm = Self::sign_extend_16(instruction.imm16);

                self.regs.write_register(instruction.rt, (rs < imm) as u32);
            }
            Opcode::Addu => {
                self.execute_alu_reg(instruction, |rs, rt| rs.wrapping_add(rt));
            }
            Opcode::Add => {
                self.execute_alu_reg(instruction, |rs, rt| {
                    rs.checked_add(rt).expect("overflow trap")
                });
            }
            Opcode::Subu => {
                self.execute_alu_reg(instruction, |rs, rt| rs.wrapping_sub(rt));
            }
            Opcode::Sub => {
                self.execute_alu_reg(instruction, |rs, rt| {
                    rs.checked_sub(rt).expect("overflow trap")
                });
            }
            Opcode::Addiu => {
                self.execute_alu_imm(instruction, |rs, instr| {
                    rs.wrapping_add(Self::sign_extend_16(instr.imm16))
                });
            }
            Opcode::Addi => {
                self.execute_alu_imm(instruction, |rs, instr| {
                    // TODO: implement overflow trap
                    (rs as i32)
                        .checked_add(Self::sign_extend_16(instr.imm16) as i32)
                        .expect("overflow trap") as u32
                });
            }
            Opcode::And => {
                self.execute_alu_reg(instruction, |rs, rt| rs & rt);
            }
            Opcode::Or => {
                self.execute_alu_reg(instruction, |rs, rt| rs | rt);
            }
            Opcode::Xor => {
                self.execute_alu_reg(instruction, |rs, rt| rs ^ rt);
            }
            Opcode::Nor => {
                self.execute_alu_reg(instruction, |rs, rt| !(rs | rt));
            }
            Opcode::Andi => {
                self.execute_alu_imm(instruction, |rs, instr| rs & (instr.imm16 as u32));
            }
            Opcode::Ori => {
                self.execute_alu_imm(instruction, |rs, instr| rs | (instr.imm16 as u32));
            }
            Opcode::Xori => {
                self.execute_alu_imm(instruction, |rs, instr| rs ^ (instr.imm16 as u32));
            }
            Opcode::Sllv => {
                self.execute_alu_reg(instruction, |rs, rt| rt << (rs & 0x1F));
            }
            Opcode::Srlv => {
                self.execute_alu_reg(instruction, |rs, rt| rt >> (rs & 0x1F));
            }
            Opcode::Srav => {
                self.execute_alu_reg(instruction, |rs, rt| ((rs as i32) >> rt) as u32);
            }
            Opcode::Sll => {
                let rt = self.regs.read_register(instruction.rt);
                let result = rt << instruction.imm5;
                self.regs.write_register(instruction.rd, result);
            }
            Opcode::Srl => {
                let rt = self.regs.read_register(instruction.rt);
                let result = rt >> instruction.imm5;
                self.regs.write_register(instruction.rd, result);
            }
            Opcode::Sra => {
                let rt = self.regs.read_register(instruction.rt);
                let result = ((rt as i32) >> instruction.imm5) as u32;
                self.regs.write_register(instruction.rd, result);
            }
            Opcode::Lui => {
                let result = (instruction.imm16 as u32) << 16;
                self.regs.write_register(instruction.rt, result);
            }
            Opcode::Mult => {
                let rs = self.regs.read_register(instruction.rs) as i32 as i64;
                let rt = self.regs.read_register(instruction.rt) as i32 as i64;

                let result = (rs * rt) as u64;

                self.regs.hi = (result >> 32) as u32;
                self.regs.lo = result as u32;
            }
            Opcode::Multu => {
                let rs = self.regs.read_register(instruction.rs) as u64;
                let rt = self.regs.read_register(instruction.rt) as u64;

                let result = rs * rt;

                self.regs.hi = (result >> 32) as u32;
                self.regs.lo = result as u32;
            }
            Opcode::Div => {
                let rs = self.regs.read_register(instruction.rs) as i32 as i64;
                let rt = self.regs.read_register(instruction.rt) as i32 as i64;

                let div = (rs / rt) as u32;
                let remainder = (rs % rt) as u32;

                self.regs.hi = remainder;
                self.regs.lo = div;
            }
            Opcode::Divu => {
                let rs = self.regs.read_register(instruction.rs) as u64;
                let rt = self.regs.read_register(instruction.rt) as u64;

                let div = (rs / rt) as u32;
                let remainder = (rs % rt) as u32;

                self.regs.hi = remainder;
                self.regs.lo = div;
            }
            Opcode::Mfhi => {
                self.regs.write_register(instruction.rd, self.regs.hi);
            }
            Opcode::Mthi => {
                self.regs.hi = self.regs.read_register(instruction.rs);
            }
            Opcode::Mflo => {
                self.regs.write_register(instruction.rd, self.regs.lo);
            }
            Opcode::Mtlo => {
                self.regs.lo = self.regs.read_register(instruction.rs);
            }
            Opcode::J => {
                let base = self.regs.pc & 0xF0000000;
                let offset = instruction.imm26 * 4;

                self.jump_dest_next = Some(base + offset);
            }
            Opcode::Jal => {
                let base = self.regs.pc & 0xF0000000;
                let offset = instruction.imm26 * 4;

                self.jump_dest_next = Some(base + offset);

                self.regs.ra = self.regs.pc + 4;
            }
            Opcode::Jr => {
                self.jump_dest_next = Some(self.regs.read_register(instruction.rs));
            }
            Opcode::Jalr => {
                self.jump_dest_next = Some(self.regs.read_register(instruction.rs));

                self.regs.write_register(instruction.rd, self.regs.pc + 4);
            }
            Opcode::Beq => {
                self.execute_branch(instruction, true, |rs, rt| rs == rt);
            }
            Opcode::Bne => {
                self.execute_branch(instruction, true, |rs, rt| rs != rt);
            }
            Opcode::Bgtz => {
                self.execute_branch(instruction, false, |rs, _| rs > 0);
            }
            Opcode::Blez => {
                self.execute_branch(instruction, false, |rs, _| rs <= 0);
            }
            Opcode::Bltz => {
                self.execute_branch(instruction, false, |rs, _| rs < 0);
            }
            Opcode::Bgez => {
                self.execute_branch(instruction, false, |rs, _| rs >= 0);
            }
            Opcode::Bltzal => {
                if self.execute_branch(instruction, false, |rs, _| rs < 0) {
                    self.regs.ra = self.regs.pc + 4;
                }
            }
            Opcode::Bgezal => {
                if self.execute_branch(instruction, false, |rs, _| rs >= 0) {
                    self.regs.ra = self.regs.pc + 4;
                }
            }
            Opcode::Bcondz => unreachable!("bcondz should be converted"),
            Opcode::Syscall => {
                let syscall_cause = 0x8;
                let cause = syscall_cause << 2;
                self.cop0.write_cause(cause);

                let sr = self.cop0.read_sr();
                let bev = (sr >> 22) & 1 == 1;

                let jmp_vector = if bev { 0xBFC00180 } else { 0x80000080 };
                self.cop0.write_epc(self.regs.pc - 4);
                self.regs.pc = jmp_vector;
            }
            //Opcode::Break => {}
            //Opcode::Cop(_) => {}
            Opcode::Mfc(n) => {
                assert_eq!(n, 0);
                let result = self.cop0.read_data(instruction.rd_raw);

                self.regs.write_register(instruction.rt, result);
            }
            Opcode::Cfc(n) => {
                assert_eq!(n, 0);
                let result = self.cop0.read_ctrl(instruction.rd_raw);

                self.regs.write_register(instruction.rt, result);
            }
            Opcode::Mtc(n) => {
                assert_eq!(n, 0);
                let rt = self.regs.read_register(instruction.rt);

                self.cop0.write_data(instruction.rd_raw, rt);
            }
            Opcode::Ctc(n) => {
                assert_eq!(n, 0);
                let rt = self.regs.read_register(instruction.rt);

                self.cop0.write_ctrl(instruction.rd_raw, rt);
            }
            //Opcode::Bcf(_) => {}
            //Opcode::Bct(_) => {}
            Opcode::Rfe => {
                let mut sr = self.cop0.read_sr();
                // clear first two bits
                let second_two_bits = (sr >> 2) & 3;
                let third_two_bits = (sr >> 4) & 3;
                sr &= !0b1111;
                sr |= second_two_bits;
                sr |= third_two_bits << 2;

                self.cop0.write_sr(sr);
            }
            //Opcode::Lwc(_) => {}
            //Opcode::Swc(_) => {}
            Opcode::Special => unreachable!(),
            Opcode::Invalid => unreachable!(),
            _ => todo!("unimplemented_instruction {:?}", instruction.opcode),
        }
    }
}

impl Cpu {
    fn bus_read_u32<P: BusLine>(&self, bus: &mut P, addr: u32) -> u32 {
        match addr {
            0x00000000..=0x00001000 if self.cop0.is_cache_isolated() => 0,
            _ => bus.read_u32(addr),
        }
    }

    fn bus_write_u32<P: BusLine>(&self, bus: &mut P, addr: u32, data: u32) {
        match addr {
            0x00000000..=0x00001000 if self.cop0.is_cache_isolated() => {}
            _ => bus.write_u32(addr, data),
        }
    }

    fn bus_read_u16<P: BusLine>(&self, bus: &mut P, addr: u32) -> u16 {
        match addr {
            0x00000000..=0x00001000 if self.cop0.is_cache_isolated() => 0,
            _ => bus.read_u16(addr),
        }
    }

    fn bus_write_u16<P: BusLine>(&self, bus: &mut P, addr: u32, data: u16) {
        match addr {
            0x00000000..=0x00001000 if self.cop0.is_cache_isolated() => {}
            _ => bus.write_u16(addr, data),
        }
    }

    fn bus_read_u8<P: BusLine>(&self, bus: &mut P, addr: u32) -> u8 {
        match addr {
            0x00000000..=0x00001000 if self.cop0.is_cache_isolated() => 0,
            _ => bus.read_u8(addr),
        }
    }

    fn bus_write_u8<P: BusLine>(&self, bus: &mut P, addr: u32, data: u8) {
        match addr {
            0x00000000..=0x00001000 if self.cop0.is_cache_isolated() => {}
            _ => bus.write_u8(addr, data),
        }
    }
}
