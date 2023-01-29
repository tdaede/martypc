
use crate::cpu::*;
use crate::cpu::cpu_biu::*;
use super::CPU_CALL_STACK_LEN;

use crate::io::IoBusInterface;
use crate::util;

macro_rules! get_operand {
    ($target: expr, $pat: path) => {
        {
            if let $pat(a) = $target {
                a
            } else {
                panic!("Unexpected operand type.");
            }
        }
    };
}

impl<'a> Cpu<'a> {

    pub fn execute_instruction(&mut self, io_bus: &mut IoBusInterface) -> ExecutionResult {

        let mut unhandled: bool = false;
        let mut jump: bool = false;
        let mut exception: CpuException = CpuException::NoException;
        let mut cycles = 0;

        let mut handled_override = match self.i.segment_override {
            SegmentOverride::None => true,
            _ => false,
        };
        
        // Check for REPx prefixes
        if (self.i.prefixes & OPCODE_PREFIX_REP1 != 0) || (self.i.prefixes & OPCODE_PREFIX_REP2 != 0) {
            // A REPx prefix was set
            
            let mut invalid_rep = false;

            match self.i.mnemonic {
                Mnemonic::STOSB | Mnemonic::STOSW | Mnemonic::LODSB | Mnemonic::LODSW | Mnemonic::MOVSB | Mnemonic::MOVSW => {
                    self.rep_type = RepType::Rep;
                }
                Mnemonic::SCASB | Mnemonic::SCASW | Mnemonic::CMPSB | Mnemonic::CMPSW => {
                    // Valid string ops with REP prefix
                    if self.i.prefixes & OPCODE_PREFIX_REP1 != 0 {
                        self.rep_type = RepType::Repne;
                    }
                    else {
                        self.rep_type = RepType::Repe;
                    }
                }
                _=> {
                    invalid_rep = true;
                    //return ExecutionResult::ExecutionError(
                    //    format!("REP prefix on invalid opcode: {:?} at [{:04X}:{:04X}].", self.i.mnemonic, self.cs, self.ip)
                    //);
                    log::warn!("REP prefix on invalid opcode: {:?} at [{:04X}:{:04X}].", self.i.mnemonic, self.cs, self.ip);
                }
            }

            // Check if we have saved state from returning from an interrupt
            if let Some(rep_state) = self.rep_state.last() {
                // Is this state for the current instruction?
                if rep_state.0 == self.cs && rep_state.1 == self.ip {
                    // Restore the current state
                    //log::trace!("Restoring state for REP string instruction.");
                    match rep_state.2 {                        
                        RepState::StosbState(es, di, cx) => { // dst: [es:di], cx
                            self.es = es;
                            self.di = di;
                            self.set_register16(Register16::CX, cx);
                        }, 
                        RepState::StoswState(es, di, cx) => { // dst: [es:di], cx
                            self.es = es;
                            self.di = di;
                            self.set_register16(Register16::CX, cx);
                        }, 
                        RepState::LodsbState(seg, seg_val, si, cx) => { // src: [ds*:si], cx
                            self.set_register16(seg, seg_val);
                            self.si = si;
                            self.set_register16(Register16::CX, cx);
                        }, 
                        RepState::LodswState(seg, seg_val, si, cx) => {  // src: [ds*:si], cx
                            self.set_register16(seg, seg_val);
                            self.si = si;
                            self.set_register16(Register16::CX, cx);                            
                        },
                        RepState::MovsbState(seg, seg_val, si, es, di, cx) => {  // src: [ds*:si], dst: [es:di], cx
                            self.set_register16(seg, seg_val);
                            self.si = si;
                            self.es = es;
                            self.di = di;
                            self.set_register16(Register16::CX, cx);                            
                        },
                        RepState::MovswState(seg, seg_val, si, es, di, cx) => { // src: [ds*:si]. dst: [es:di], cx
                            self.set_register16(seg, seg_val);
                            self.si = si;
                            self.es = es;
                            self.di = di;
                            self.set_register16(Register16::CX, cx); 
                        }, 
                        RepState::ScasbState(es,di,cx) => { // src: [es:di], cx
                            self.es = es;
                            self.di = di;
                            self.set_register16(Register16::CX, cx);
                        }, 
                        RepState::ScaswState(es,di,cx) => { // src: [es:di], cx
                            self.es = es;
                            self.di = di;
                            self.set_register16(Register16::CX, cx);
                        }, 
                        RepState::CmpsbState(seg, seg_val, si, es, di, cx) => { // src: [ds*:si], dst: [es:di], cx
                            self.set_register16(seg, seg_val);
                            self.si = si;
                            self.es = es;
                            self.di = di;
                            self.set_register16(Register16::CX, cx);   
                        }, 
                        RepState::CmpswState(seg, seg_val, si, es, di, cx) => {  // src: [ds*:si], dst: [es:di], cx
                            self.set_register16(seg, seg_val);
                            self.si = si;
                            self.es = es;
                            self.di = di;
                            self.set_register16(Register16::CX, cx);   
                        },        
                        RepState::NoState => {
                            // Rep prefix on invalid opcode, restore nothing
                        }
                    }
                    self.rep_state.pop();
                }
            }
            if !invalid_rep {
                self.in_rep = true;
                self.rep_mnemonic = self.i.mnemonic;
            }
        }

        // Reset the wait cycle after STI
        self.interrupt_inhibit = false;

        // Keep a tally of how many Opcode 0x00's we've executed in a row. Too many likely means we've run 
        // off the rails into uninitialized memory, whereupon we halt so we can check things out.
        if self.i.opcode == 0x00 {
            self.opcode0_counter = self.opcode0_counter.wrapping_add(1);

            if self.opcode0_counter > 5 {
                // Halt permanently by clearing interrupt flag

                /*
                self.clear_flag(Flag::Interrupt);
                self.halted = true;
                */
            }
        }
        else {
            self.opcode0_counter = 0;
        }

        match self.i.opcode {
            0x00 | 0x02 | 0x04 |  // ADD r/m8, r8 | r8, r/m8 | al, imm8
            0x08 | 0x0A | 0x0C |  // OR r/m8, r8  | r8, r/m8 | al, imm8
            0x10 | 0x12 | 0x14 |  // ADC r/m8,r8  | r8, r/m8 | al, imm8 
            0x18 | 0x1A | 0x1C |  // SBB r/m8,r8  | r8, r/m8 | al, imm8 
            0x20 | 0x22 | 0x24 |  // AND r/m8,r8  | r8, r/m8 | al, imm8 
            0x28 | 0x2A | 0x2C |  // SUB r/m8,r8  | r8, r/m8 | al, imm8 
            0x30 | 0x32 | 0x34 => // XOR r/m8, r8 | r8, r/m8 | al, imm8
            { 
                // 8 bit ALU operations
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();            
                
                self.cycles_nx(2);
                
                if let OperandType::AddressingMode(_) = self.i.operand1_type {
                    self.cycles(3);
                }

                let result = self.math_op8(self.i.mnemonic, op1_value, op2_value);
                self.write_operand8(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);

                handled_override = true;
            }
            0x01 | 0x03 | 0x05 |  // ADD r/m16, r16 | r16, r/m16 | ax, imm16
            0x09 | 0x0B | 0x0D |  // OR r/m16, r16 | r16, r/m16 | ax, imm16
            0x11 | 0x13 | 0x15 |  // ADC r/m16,r16 | r16, r/m16 | ax,imm16 
            0x19 | 0x1B | 0x1D |  // SBB r/m16,r16 | r16, r/m16 | ax,imm16 
            0x21 | 0x23 | 0x25 |  // AND r/m16,r16 | r16, r/m16 | ax,imm16 
            0x29 | 0x2B | 0x2D |  // SUB r/m16,r16 | r16, r/m16 | ax,imm16 
            0x31 | 0x33 | 0x35 => // XOR r/m16, r16 |  XOR r16, r/m16
            {
                // 16 bit ADD variants
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();
                
                self.cycles_nx(2);
                
                if let OperandType::AddressingMode(_) = self.i.operand1_type {
                    self.cycles(3);
                }

                let result = self.math_op16(self.i.mnemonic, op1_value, op2_value);
                self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);       
                handled_override = true;         
            }
            0x06 => {
                // PUSH es
                // Flags: None
                self.push_register16(Register16::ES, ReadWriteFlag::RNI);
            }
            0x07 => {
                // POP es
                // Flags: None
                self.pop_register16(Register16::ES, ReadWriteFlag::RNI);
            }
            0x0E => {
                // PUSH cs
                // Flags: None
                self.push_register16( Register16::CS, ReadWriteFlag::RNI);
            }
            0x0F => {
                // POP cs
                // Flags: None
                self.pop_register16(Register16::CS, ReadWriteFlag::RNI);
            }
            0x16 => {
                // PUSH ss
                // Flags: None
                self.push_register16(Register16::SS, ReadWriteFlag::RNI);
            }
            0x17 => {
                // POP ss
                // Flags: None
                self.pop_register16(Register16::SS, ReadWriteFlag::RNI);
            }
            0x1E => {
                // PUSH ds
                // Flags: None
                self.push_register16(Register16::DS, ReadWriteFlag::RNI);
            }
            0x1F => {
                // POP ds
                // Flags: None
                self.pop_register16(Register16::DS, ReadWriteFlag::RNI);
            }
            0x26 => {
                // ES Segment Override Prefix
            }
            0x27 => {
                // DAA — Decimal Adjust AL after Addition
                self.daa();
            }
            0x2E => {
                // CS Override Prefix
            }
            0x2F => {
                // DAS
                self.das();
            }
            0x36 => {
                // SS Segment Override Prefix
            }
            0x37 => {
                // AAA
                self.aaa();
            }
            0x38 | 0x3A | 0x3C => {
                // CMP r/m8,r8 | r8, r/m8 | al,imm8 
                // CMP 8-bit variants
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();
                
                self.cycles_nx(2);
                let _result = self.math_op8(Mnemonic::CMP,  op1_value,  op2_value);
                //self.write_operand8(self.i.operand1_type, self.i.segment_override, result);
                handled_override = true;
            }
            0x39 | 0x3B | 0x3D => {
                // CMP r/m16,r16 | r16, r/m16 | ax,imm16 
                // CMP 16-bit variants
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();

                self.cycles_nx(2);
                let _result = self.math_op16(Mnemonic::CMP,  op1_value,  op2_value);
                //self.write_operand16(self.i.operand1_type, self.i.segment_override, result);
                handled_override = true;
            }
            0x3E => {
                // DS Segment Override Prefix
            }
            0x3F => {
                // AAS
                self.aas();
            }
            0x40..=0x47 => {
                // INC r16 register-encoded operands
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                // math_op16 handles flags
                let result = self.math_op16(Mnemonic::INC, op1_value, 0);
                self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);

                self.cycles(1);
                handled_override = true;
            }
            0x48..=0x4F => {
                // DEC r16 register-encoded operands
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                // math_op16 handles flags
                let result = self.math_op16(Mnemonic::DEC, op1_value, 0);
                self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);

                self.cycles(1);
                handled_override = true;           
            }
            0x50..=0x57 => {
                // PUSH reg16
                // Flags: None
                let reg = REGISTER16_LUT[(self.i.opcode & 0x07) as usize];
                self.cycles(4);
                self.push_register16(reg, ReadWriteFlag::RNI);
            }
            0x58..=0x5F => {
                // POP reg16
                // Flags: None
                let reg = REGISTER16_LUT[(self.i.opcode & 0x07) as usize];
                self.pop_register16(reg, ReadWriteFlag::RNI);
            }
            0x60..=0x7F => {
                // JMP rel8 variants
                // Note that 0x60-6F maps to 0x70-7F on 8088
                jump = match self.i.opcode & 0x0F {
                    0x00 => self.get_flag(Flag::Overflow),  // JO - Jump if overflow set
                    0x01 => !self.get_flag(Flag::Overflow), // JNO - Jump it overflow not set
                    0x02 => self.get_flag(Flag::Carry), // JB -> Jump if carry set
                    0x03 => !self.get_flag(Flag::Carry), // JNB -> Jump if carry not set
                    0x04 => self.get_flag(Flag::Zero), // JZ -> Jump if Zero set
                    0x05 => !self.get_flag(Flag::Zero), // JNZ -> Jump if Zero not set
                    0x06 => self.get_flag(Flag::Carry) || self.get_flag(Flag::Zero), // JBE -> Jump if Carry OR Zero
                    0x07 => !self.get_flag(Flag::Carry) && !self.get_flag(Flag::Zero), // JNBE -> Jump if Carry not set AND Zero not set
                    0x08 => self.get_flag(Flag::Sign), // JS -> Jump if Sign set
                    0x09 => !self.get_flag(Flag::Sign), // JNS -> Jump if Sign not set
                    0x0A => self.get_flag(Flag::Parity), // JP -> Jump if Parity set
                    0x0B => !self.get_flag(Flag::Parity), // JNP -> Jump if Parity not set
                    0x0C => self.get_flag(Flag::Sign) != self.get_flag(Flag::Overflow), // JL -> Jump if Sign flag != Overflow flag
                    0x0D => self.get_flag(Flag::Sign) == self.get_flag(Flag::Overflow), // JNL -> Jump if Sign flag == Overflow flag
                    0x0E => self.get_flag(Flag::Zero) || (self.get_flag(Flag::Sign) != self.get_flag(Flag::Overflow)),  // JLE ((ZF=1) OR (SF!=OF))
                    0x0F => !self.get_flag(Flag::Zero) && (self.get_flag(Flag::Sign) == self.get_flag(Flag::Overflow)), // JNLE ((ZF=0) AND (SF=OF))
                    _ => false
                };
                if jump {

                    let rel8 = get_operand!(self.i.operand1_type, OperandType::Relative8);
                    //log::trace!(">>> Calculating jump to new IP: {:04X} + size:{} + rel8:{}", self.ip, self.i.size, rel8);
                    self.ip = util::relative_offset_u16(self.ip, rel8 as i16 + self.i.size as i16 );
                    
                    self.cycles(2); // Suspend is not immediate and occurs sometime during 9 cycle post-fetch. May need to adjust. 
                    self.biu_suspend_fetch();
                    self.cycles(4);
                    self.biu_queue_flush();
                }
                else {
                    self.cycles(1);
                }
            }
            0x80 | 0x82 => {
                // ADD/OR/ADC/SBB/AND/SUB/XOR/CMP r/m8, imm8
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();
                
                let result = self.math_op8(self.i.mnemonic, op1_value, op2_value);

                if self.i.mnemonic != Mnemonic::CMP {
                    self.write_operand8(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                }
                handled_override = true;
            }
            0x81 | 0x83 => {
                // ADD/OR/ADC/SBB/AND/SUB/XOR/CMP r/m16, imm16
                // ADD/ADC/SBB/SUB/CMP r/m16, imm8 (sign-extended) (Decode extends operand for us)
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();
                // math_op16 handles flags
                let result = self.math_op16(self.i.mnemonic, op1_value, op2_value);

                if self.i.mnemonic != Mnemonic::CMP {
                    self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                }
                handled_override = true;
            }
            0x84 => {
                // TEST r/m8, r8
                // Flags: o..sz.pc
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();
                
                self.math_op8(Mnemonic::TEST, op1_value, op2_value);
                self.cycles_nx(2);
                handled_override = true;
            }
            0x85 => {
                // TEST r/m16, r16
                // Flags: o..sz.pc                
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();
                // math_op16 handles flags
                self.math_op16(Mnemonic::TEST, op1_value, op2_value);
                self.cycles_nx(2);
                handled_override = true;
            }
            0x86 => {
                // XCHG r8, r/m8
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();

                self.cycles(4);
                
                if let OperandType::AddressingMode(_) = self.i.operand2_type {
                    // Memory operand takes 2 more cycles
                    self.cycles(2);
                }

                // Exchange values. Write operand2 first so we don't affect EA calculation if EA includes register being swapped.
                self.write_operand8(self.i.operand2_type, self.i.segment_override, op1_value, ReadWriteFlag::RNI);
                self.write_operand8(self.i.operand1_type, self.i.segment_override, op2_value, ReadWriteFlag::Normal);
                handled_override = true;
            }
            0x87 => {
                // XCHG r16, r/m16
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();

                self.cycles(4);

                if let OperandType::AddressingMode(_) = self.i.operand2_type {
                    // Memory operand takes 2 more cycles
                    self.cycles(2);
                }

                // Exchange values. Write operand2 first so we don't affect EA calculation if EA includes register being swapped.
                self.write_operand16(self.i.operand2_type, self.i.segment_override, op1_value, ReadWriteFlag::RNI);
                self.write_operand16(self.i.operand1_type, self.i.segment_override, op2_value, ReadWriteFlag::Normal);
                handled_override = true;
            }
            0x88 | 0x8A => {
                // MOV r/m8, r8  |  MOV r8, r/m8
                self.cycle();
                let op_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();

                if let OperandType::AddressingMode(_) = self.i.operand1_type {
                    self.cycles(2);
                }
                self.write_operand8(self.i.operand1_type, self.i.segment_override, op_value, ReadWriteFlag::RNI);
                handled_override = true;
            }
            0x89 | 0x8B => {
                // MOV r/m16, r16  |  MOV r16, r/m16
                self.cycle();
                let op_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();

                if let OperandType::AddressingMode(_) = self.i.operand1_type {
                    self.cycles(2);
                }                
                self.write_operand16(self.i.operand1_type, self.i.segment_override, op_value, ReadWriteFlag::RNI);
                handled_override = true;
            }
            0x8C | 0x8E => {
                // MOV r/m16, SReg | MOV SReg, r/m16

                if let OperandType::Register16(foo) = self.i.operand2_type {
                    if let Register16::InvalidRegister = foo {
                        println!("Whoops!")
                    }
                }                
                let op_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();
                self.write_operand16(self.i.operand1_type, self.i.segment_override, op_value, ReadWriteFlag::RNI);
                handled_override = true;
            }
            0x8D => {
                // LEA - Load Effective Address
                let ea = self.load_effective_address(self.i.operand2_type);
                match ea {
                    Some(value) => {
                        self.write_operand16(self.i.operand1_type, SegmentOverride::None, value, ReadWriteFlag::Normal);
                    }
                    None => {
                        // In the event of an invalid (Register) operand2, operand1 is set to the last EA calculated by an instruction.
                        self.write_operand16(self.i.operand1_type, SegmentOverride::None, self.last_ea, ReadWriteFlag::Normal);
                        self.cycles(3);
                    }
                }
                
            }
            0x8F => {
                // POP r/m16
                let value = self.pop_u16();
                self.write_operand16(self.i.operand1_type, self.i.segment_override, value, ReadWriteFlag::RNI);
                handled_override = true;
            }
            0x90..=0x97 => {
                // XCHG AX, r
                // Cycles: 3 (1 Fetch + 2 EU)
                // Flags: None
                let op_reg = REGISTER16_LUT[(self.i.opcode & 0x07) as usize];
                let ax_value = self.ax;
                let op_reg_value = self.get_register16(op_reg);
                self.set_register16(Register16::AX, op_reg_value);
                self.set_register16(op_reg, ax_value);
                self.cycles(2);
            }
            0x98 => {
                // CBW - Convert Byte to Word
                // Flags: None
                self.sign_extend_al();
            }
            0x99 => {
                // CWD - Convert Word to Doubleword
                // Flags: None
                self.sign_extend_ax();
            }
            0x9A => {
                // CALLF - Call Far

                self.cycle();
                self.biu_suspend_fetch();
                self.cycles(6);

                // Push return address of next instruction
                self.push_register16(Register16::CS, ReadWriteFlag::Normal);
                
                self.cycles(3);
                let next_i = self.ip + (self.i.size as u16);

                if let OperandType::FarAddress(segment, offset) = self.i.operand1_type {        
                    
                    // Add to call stack
                    if self.call_stack.len() == CPU_CALL_STACK_LEN {
                        self.call_stack.pop_front();
                    }                    
                    self.call_stack.push_back(CallStackEntry::CallF(self.cs, self.ip, segment, offset));
                    
                    self.cs = segment;
                    self.ip = offset;
                }

                self.biu_queue_flush();
                self.cycles(4); 
                
                self.push_u16(next_i, ReadWriteFlag::RNI);

                jump = true;
            }
            0x9B => {
                // WAIT
                self.cycles(3);
            }
            0x9C => {
                // PUSHF - Push Flags
                self.cycles(4);
                self.push_flags(ReadWriteFlag::RNI);
            }
            0x9D => {
                // POPF - Pop Flags
                self.pop_flags();
            }
            0x9E => {
                // SAHF - Store AH into Flags
                self.store_flags(self.ah as u16);
                cycles = 4;
            }
            0x9F => {
                // LAHF - Load Status Flags into AH Register
                let flags = self.load_flags() as u8;
                self.set_register8(Register8::AH, flags);
                cycles = 4;
            }
            0xA0 => {
                // MOV al, offset8
                // These MOV variants are unique in that they take a direct offset with no modr/m byte
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();
                self.set_register8(Register8::AL, op2_value);
                handled_override = true;
            }
            0xA1 => {
                // MOV AX, offset16
                // These MOV variants are unique in that they take a direct offset with no modr/m byte
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();
                self.set_register16(Register16::AX, op2_value);                
                handled_override = true;
            }
            0xA2 => {
                // MOV offset8, Al
                // These MOV variants are unique in that they take a direct offset with no modr/m byte
                let op2_value = self.al;
                self.write_operand8(self.i.operand1_type, self.i.segment_override, op2_value, ReadWriteFlag::RNI);
                handled_override = true;
            }
            0xA3 => {
                // MOV offset16, AX
                // These MOV variants are unique in that they take a direct offset with no modr/m byte
                let op2_value = self.ax;
                self.write_operand16(self.i.operand1_type, self.i.segment_override, op2_value, ReadWriteFlag::RNI);
                handled_override = true;        
            }
            0xA4 => {
                // MOVSB
                if !self.in_rep || (self.in_rep && self.cx > 0) {
                    self.string_op(Mnemonic::MOVSB, self.i.segment_override);
                }

                // Check for end condition (CX==0)
                if self.in_rep {
                    if self.cx > 0 {
                        self.decrement_register16(Register16::CX);
                    }
                    if self.cx == 0 {
                        self.in_rep = false;
                        self.rep_type = RepType::NoRep;
                    }
                }
                handled_override = true;
            }
            0xA5 => {
                // MOVSW
                if !self.in_rep || (self.in_rep && self.cx > 0) {
                    self.string_op(Mnemonic::MOVSW, self.i.segment_override);
                }

                // Check for end condition (CX==0)
                if self.in_rep {
                    if self.cx > 0 {
                        self.decrement_register16(Register16::CX);
                    }
                    if self.cx == 0 {
                        self.in_rep = false;
                        self.rep_type = RepType::NoRep;
                    }
                }
                handled_override = true;
            }
            0xA6 | 0xA7 => {
                // CMPSB & CMPSw
                // Segment override: DS overridable
                // Flags: All
                if !self.in_rep || (self.in_rep && self.cx > 0) {
                    self.string_op(self.i.mnemonic, self.i.segment_override);       
                }

                // Check for REP end condition #1 (CX==0)
                if self.in_rep {
                    if self.cx > 0 {
                        self.decrement_register16(Register16::CX);
                    }
                    if self.cx == 0 {
                        self.in_rep = false;
                        self.rep_type = RepType::NoRep;
                    }
                }
                // Check for REP end condition #2 (Z/NZ)
                match self.rep_type {
                    RepType::Repne => {
                        // Repeat while NOT zero. If Zero flag is set, end REP.
                        if self.get_flag(Flag::Zero) {
                            self.in_rep = false;
                            self.rep_type = RepType::NoRep;
                        }
                    }
                    RepType::Repe => {
                        // Repeat while zero. If zero flag is NOT set, end REP.
                        if !self.get_flag(Flag::Zero) {
                            self.in_rep = false;
                            self.rep_type = RepType::NoRep;
                        }
                    }
                    _=> {}
                };
                handled_override = true;
            }
            0xA8 => {
                // TEST al, imm8
                // Flags: o..sz.pc
                let op1_value = self.al;
                let op2_value = self.read_operand8(self.i.operand2_type, SegmentOverride::None).unwrap();
                
                self.math_op8(Mnemonic::TEST,  op1_value, op2_value);
            }
            0xA9 => {
                // TEST ax, imm16
                // Flags: o..sz.pc
                let op1_value = self.ax;
                let op2_value = self.read_operand16(self.i.operand2_type, SegmentOverride::None).unwrap();
                
                self.math_op16(Mnemonic::TEST,  op1_value, op2_value);
            }
            0xAA | 0xAB => {
                // STOSB & STOSW
                if !self.in_rep || (self.in_rep && self.cx > 0) {
                    self.string_op(self.i.mnemonic, SegmentOverride::None);
                }

                // Check for end condition (CX==0)
                if self.in_rep {
                    if self.cx > 0 {
                        self.decrement_register16(Register16::CX);
                    }
                    if self.cx == 0 {
                        self.in_rep = false;
                        self.rep_type = RepType::NoRep;
                    }
                }
            }
            0xAC | 0xAD => {
                // LODSB & LODSW
                // Flags: None

                // Although LODSx is not typically used with a REP prefix, it can be
                if !self.in_rep {
                    self.cycles(2);
                    self.string_op(self.i.mnemonic, self.i.segment_override);
                    self.cycles(2);
                } 
                else {
                    if self.cx > 0 {
                        self.string_op(self.i.mnemonic, self.i.segment_override);
                    }
                }
                
                // Check for REP end condition #1 (CX==0)
                if self.in_rep {
                    if self.cx > 0 {
                        self.decrement_register16(Register16::CX);
                    }
                    if self.cx == 0 {
                        self.in_rep = false;
                        self.rep_type = RepType::NoRep;
                    }
                }
                handled_override = true;
            }
            0xAE | 0xAF => {
                // SCASB & SCASW
                // Flags: ALL
                if !self.in_rep {
                    self.cycles(4);
                    self.string_op(self.i.mnemonic, SegmentOverride::None);
                    self.cycles(3);
                }
                else {
                    if self.cx > 0 {
                        self.string_op(self.i.mnemonic, SegmentOverride::None);
                    }
                }

                // Check for REP end condition #1 (CX==0)
                if self.in_rep {
                    if self.cx > 0 {
                        self.decrement_register16(Register16::CX);
                    }
                    if self.cx == 0 {
                        self.in_rep = false;
                        self.rep_type = RepType::NoRep;
                    }
                }
                // Check for REP end condition #2 (Z/NZ)
                match self.rep_type {
                    RepType::Repne => {
                        // Repeat while NOT zero. If Zero flag is set, end REP.
                        if self.get_flag(Flag::Zero) {
                            self.in_rep = false;
                            self.rep_type = RepType::NoRep;
                        }
                    }
                    RepType::Repe => {
                        // Repeat while zero. If zero flag is NOT set, end REP.
                        if !self.get_flag(Flag::Zero) {
                            self.in_rep = false;
                            self.rep_type = RepType::NoRep;
                        }
                    }
                    _=> {}
                };
            }
            0xB0..=0xB7 => {
                // MOV r8, imm8
                if let OperandType::Immediate8(imm8) = self.i.operand2_type {
                    if let OperandType::Register8(reg) = self.i.operand1_type { 
                        self.set_register8(reg, imm8);
                    }
                }
                cycles = 4;
            }
            0xB8..=0xBF => {
                // MOV r16, imm16
                if let OperandType::Immediate16(imm16) = self.i.operand2_type {
                    if let OperandType::Register16(reg) = self.i.operand1_type {
                        self.set_register16(reg, imm16);
                    }
                }
            }
            0xC0 | 0xC2 => {
                // RETN imm16 - Return from call w/ release
                // 0xC0 undocumented alias for 0xC2
                // Flags: None
                let new_ip = self.pop_u16();
                // temporary timings
                self.biu_suspend_fetch();
                self.cycle();
                self.biu_queue_flush();
                self.cycles(2);    
                self.ip = new_ip;
                
                let stack_disp = self.read_operand16(self.i.operand1_type, SegmentOverride::None).unwrap();
                self.release(stack_disp);                

                // Pop call stack
                self.call_stack.pop_back();

                jump = true
            }
            0xC1 | 0xC3 => {
                // RETN - Return from call
                // 0xC1 undocumented alias for 0xC3
                // Flags: None
                // Effectively, this instruction is pop ip
                let new_ip = self.pop_u16();
                self.biu_suspend_fetch();
                self.cycle();
                self.biu_queue_flush();
                self.cycles(2);                
                self.ip = new_ip;
                
                // Pop call stack
                self.call_stack.pop_back();

                jump = true
            }
            0xC4 => {
                // LES - Load ES from Pointer
                // Operand 2 is far pointer
                let (les_segment, les_offset) = 
                    self.read_operand_farptr(
                        self.i.operand2_type, 
                        self.i.segment_override,
                        ReadWriteFlag::Normal).unwrap();

                //log::trace!("LES instruction: Loaded {:04X}:{:04X}", les_segment, les_offset);
                self.write_operand16(
                    self.i.operand1_type, 
                    self.i.segment_override, 
                    les_offset, 
                    ReadWriteFlag::Normal);
                self.es = les_segment;
                handled_override = true;
            }
            0xC5 => {
                // LDS - Load DS from Pointer
                // Operand 2 is far pointer
                let (lds_segment, lds_offset) = 
                    self.read_operand_farptr(
                        self.i.operand2_type, 
                        self.i.segment_override,
                        ReadWriteFlag::RNI
                    ).unwrap();

                //log::trace!("LDS instruction: Loaded {:04X}:{:04X}", lds_segment, lds_offset);
                self.write_operand16(self.i.operand1_type, self.i.segment_override, lds_offset, ReadWriteFlag::Normal);
                self.ds = lds_segment;
                handled_override = true;
            }
            0xC6 => {
                // MOV r/m8, imm8
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();
                self.cycles(2);
                self.write_operand8(self.i.operand1_type, self.i.segment_override, op2_value, ReadWriteFlag::RNI);
                
                handled_override = true;
            }
            0xC7 => {
                // MOV r/m16, imm16
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();
                self.cycles(2);
                self.write_operand16(self.i.operand1_type, self.i.segment_override, op2_value, ReadWriteFlag::RNI);
                
                handled_override = true;
            }
            0xC8 | 0xCA => {
                // RETF imm16 - Far Return w/ release 
                // 0xC8 undocumented alias for 0xCA
                
                self.pop_register16(Register16::IP, ReadWriteFlag::Normal);
                self.biu_suspend_fetch();    
                self.cycles(3);
                self.pop_register16(Register16::CS, ReadWriteFlag::Normal);
                
                let stack_disp = self.read_operand16(self.i.operand1_type, SegmentOverride::None).unwrap();
                self.release(stack_disp);

                self.biu_queue_flush();
                self.cycle();
                
                // Pop call stack
                self.call_stack.pop_back();
                jump = true;
            }
            0xC9 | 0xCB => {
                // RETF - Far Return
                // 0xC9 undocumented alias for 0xCB
                self.pop_register16(Register16::IP, ReadWriteFlag::Normal);
                self.pop_register16(Register16::CS, ReadWriteFlag::Normal);

                // temporary timings
                self.biu_suspend_fetch();
                self.cycle();
                self.biu_queue_flush();
                self.cycles(2); 

                // Pop call stack
                self.call_stack.pop_back();                
                jump = true;
            }
            0xCC => {
                // INT 3 - Software Interrupt 3
                // This is a special form of INT which assumes IRQ 3 always. Most assemblers will not generate this form
                self.ip = self.ip.wrapping_add(1);
                self.sw_interrupt(3);

                jump = true;    
            }
            0xCD => {
                // INT imm8 - Software Interrupt
                // The Interrupt flag does not affect the handling of non-maskable interrupts (NMIs) or software interrupts
                // generated by the INT instruction. 

                // Get IRQ number
                let irq = self.read_operand8(self.i.operand1_type, SegmentOverride::None).unwrap();
                self.ip = self.ip.wrapping_add(2);
                self.sw_interrupt(irq);
                jump = true;
            }
            0xCE => {
                // INTO - Call Overflow Interrupt Handler
                self.ip = self.ip.wrapping_add(1);
                self.sw_interrupt(4);
            
                jump = true;
            }
            0xCF => {
                // IRET instruction
                self.end_interrupt();
                jump = true;
            }
            0xD0 => {
                // ROL, ROR, RCL, RCR, SHL, SHR, SAR:  r/m8, 0x01
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let result = self.bitshift_op8(self.i.mnemonic, op1_value, 1);
                self.write_operand8(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                handled_override = true;
            }
            0xD1 => {
                // ROL, ROR, RCL, RCR, SHL, SHR, SAR:  r/m16, 0x01
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let result = self.bitshift_op16(self.i.mnemonic, op1_value, 1);
                self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                handled_override = true;
            }
            0xD2 => {
                // ROL, ROR, RCL, RCR, SHL, SHR, SAR:  r/m8, cl
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();

                self.cycles(6);
                self.trace_comment("START LOOPING");
                if self.cl > 0 {
                    self.cycles(4 * self.cl as u32 - 1);
                }
                
                self.trace_comment("DONE LOOPING");
                let result = self.bitshift_op8(self.i.mnemonic, op1_value, op2_value);

                if let OperandType::AddressingMode(_) = self.i.operand1_type {
                    self.cycles(5); // Is there a prefetch abort in here?
                }
                else {
                    //self.cycle();
                }

                self.write_operand8(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                // TODO: Cost
                handled_override = true;
            }
            0xD3 => {
                // ROL, ROR, RCL, RCR, SHL, SHR, SAR:  r/m16, cl
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();

                self.cycles(6);
                self.trace_comment("START LOOPING");
                if self.cl > 0 {
                    self.cycles(4 * self.cl as u32 - 1);
                }
             
                self.trace_comment("DONE LOOPING");
                let result = self.bitshift_op16(self.i.mnemonic, op1_value, op2_value);

                if let OperandType::AddressingMode(_) = self.i.operand1_type {
                    self.cycles(5); // Is there a prefetch abort in here?
                }
                else {
                    //self.cycle();
                }                

                self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                // TODO: Cost
                handled_override = true;
            }
            0xD4 => {
                // AAM - Ascii adjust AX after Multiply
                // Get imm8 value
                let op1_value = self.read_operand8(self.i.operand1_type, SegmentOverride::None).unwrap();
                
                if op1_value == 0 {
                    exception = CpuException::DivideError;
                }
                else {
                    self.aam(op1_value);
                }
            }
            0xD5 => {
                // AAD - Ascii Adjust before Division
                let op1_value = self.read_operand8(self.i.operand1_type, SegmentOverride::None).unwrap();
                self.aad(op1_value);
            }
            0xD6 => {
                // SALC - Undocumented Opcode - Set Carry flag in AL
                // http://www.rcollins.org/secrets/opcodes/SALC.html

                self.set_register8(Register8::AL,
                    match self.get_flag(Flag::Carry) {
                        true => 0xFF,
                        false => 0
                    }
                );
            }
            0xD7 => {
                // XLAT
                
                // Handle segment override, default DS
                let segment = Cpu::segment_override(self.i.segment_override, Segment::DS);

                let disp16: u16 = self.bx.wrapping_add(self.al as u16);

                let addr = self.calc_linear_address_seg(segment, disp16);
                
                let value = self.biu_read_u8(segment, addr);
                
                self.set_register8(Register8::AL, value as u8);
                handled_override = true;
            }
            0xD8..=0xDF => {
                // ESC - FPU instructions. 
                
                // Perform dummy read if memory operand
                let _op1_value = self.read_operand16(self.i.operand1_type, SegmentOverride::None);
            }
            0xE0 => {
                // LOOPNE - Decrement CX, Jump short if count!=0 and ZF=0
                // loop does not modify flags

                // Cycles spent decrementing CX were accounted for in decode(). This instruction doesn't have a clean 
                // separation between fetch/execute.                
                self.decrement_register16(Register16::CX);
                self.cycle();

                if self.cx != 0 {
                    if !self.get_flag(Flag::Zero) {
                        if let OperandType::Relative8(rel8) = self.i.operand1_type {
                            self.ip = util::relative_offset_u16(self.ip, rel8 as i16 + self.i.size as i16 );
                        }
                        self.cycle();
                        self.biu_suspend_fetch();
                        self.cycles(4);
                        self.biu_queue_flush();
                        jump = true;                     
                    }
                }
                if !jump {
                    self.cycle();
                }                
            }
            0xE1 => {
                // LOOPE - Jump short if count!=0 and ZF=1
                // loop does not modify flags
                
                // Cycles spent decrementing CX were accounted for in decode(). This instruction doesn't have a clean 
                // separation between fetch/execute.
                self.decrement_register16(Register16::CX);
                self.cycle();

                if self.cx != 0 {
                    if self.get_flag(Flag::Zero) {                        
                        if let OperandType::Relative8(rel8) = self.i.operand1_type {
                            self.ip = util::relative_offset_u16(self.ip, rel8 as i16 + self.i.size as i16 );
                        }
                        self.cycle();
                        self.biu_suspend_fetch();
                        self.cycles(4);
                        self.biu_queue_flush();
                        jump = true;
                    }
                }
                if !jump {
                    self.cycle();
                }
            }
            0xE2 => {
                // LOOP - Jump short if count != 0 
                // loop does not modify flags

                // Cycles spent decrementing CX were accounted for in decode(). This instruction doesn't have a clean 
                // separation between fetch/execute.                
                self.decrement_register16(Register16::CX);
                self.cycle();

                if self.cx != 0 {
                    if let OperandType::Relative8(rel8) = self.i.operand1_type {
                        self.ip = util::relative_offset_u16(self.ip, rel8 as i16 + self.i.size as i16 );

                        self.cycle();
                        self.biu_suspend_fetch();
                        self.cycles(4);
                        self.biu_queue_flush();
                        jump = true;
                    }
                }
                if !jump {
                    self.cycle();
                }
            }
            0xE3 => {
                // JCXZ - Jump if CX == 0
                // Flags: None
                                
                if self.cx == 0 {
                    if let OperandType::Relative8(rel8) = self.i.operand1_type {
                        self.ip = util::relative_offset_u16(self.ip, rel8 as i16 + self.i.size as i16 );
                    }
                    self.cycle();
                    self.biu_suspend_fetch();
                    self.cycles(4);
                    self.biu_queue_flush();                    
                    jump = true;
                }
                if !jump {
                    self.cycle();
                }
            }
            0xE4 => {
                // IN al, imm8
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap(); 
                
                let in_byte = io_bus.read_u8(op2_value as u16);
                self.set_register8(Register8::AL, in_byte);
                //println!("IN: Would input value from port {:#02X}", op2_value);  
                
                #[cfg(feature = "cpu_validator")]
                self.validator.as_mut().unwrap().discard_op();
            }
            0xE5 => {
                // IN ax, imm8
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap(); 
                let in_byte = io_bus.read_u8(op2_value as u16);
                self.set_register16(Register16::AX, in_byte as u16);

                #[cfg(feature = "cpu_validator")]
                self.validator.as_mut().unwrap().discard_op();
            }
            0xE6 => {
                // OUT imm8, al
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();                

                // Write to port
                io_bus.write_u8(op1_value as u16, op2_value);
                //println!("OUT: Would output {:02X} to Port {:#02X}", op2_value, op1_value);

                #[cfg(feature = "cpu_validator")]
                self.validator.as_mut().unwrap().discard_op();
            }
            0xE7 => {
                // OUT imm8, ax
                let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();                

                // Write first 8 bits to first port
                io_bus.write_u8(op1_value as u16, (op2_value & 0xFF) as u8);
                // Write next 8 bits to port + 1
                io_bus.write_u8((op1_value + 1) as u16, (op2_value >> 8 & 0xFF) as u8);

                #[cfg(feature = "cpu_validator")]
                self.validator.as_mut().unwrap().discard_op();
            }
            0xE8 => {
                // CALL rel16
                // Push offset of next instruction
                let cs = self.get_register16(Register16::CS);
                let ip = self.get_register16(Register16::IP);
                let next_i = ip.wrapping_add(self.i.size as u16);
                self.push_u16(next_i, ReadWriteFlag::Normal);

                // Add rel16 to ip
                let rel16 = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                self.ip = util::relative_offset_u16(self.ip, rel16 as i16 + self.i.size as i16 );

                // Add to call stack
                if self.call_stack.len() == CPU_CALL_STACK_LEN {
                    self.call_stack.pop_front();
                }                

                // temporary timings
                self.biu_suspend_fetch();
                self.cycles(4);
                self.biu_queue_flush();
                jump = true;

                self.call_stack.push_back(CallStackEntry::Call(cs, ip, rel16));
            }
            0xE9 => {
                // JMP rel16
                // Cycles: 10 (4 fetch + 6 EU)
                
                let rel16 = get_operand!(self.i.operand1_type, OperandType::Relative16);
                self.ip = Cpu::relative_offset_u16(self.ip, rel16 as i16 + self.i.size as i16 );
                /*
                if let OperandType::Relative16(rel16) = self.i.operand1_type {
                    self.ip = util::relative_offset_u16(self.ip, rel16 as i16 + self.i.size as i16 );
                }
                */
                self.biu_suspend_fetch(); // Immediately suspend.
                self.cycles(4);
                self.biu_queue_flush();
                self.cycles(2);
                jump = true;
            }
            0xEA => {
                // JMP FAR [ptr16:16]
                // This instruction is longer than the 8088 instruction queue.
                // Cycles (8088): 14 (8 Fetch + 6 EU)
                if let OperandType::FarAddress(segment, offset) = self.i.operand1_type {                
                    self.cs = segment;
                    self.ip = offset;
                }
                self.biu_suspend_fetch();
                self.cycles(4);
                self.biu_queue_flush();
                self.cycles(2);
                jump = true;
            }
            0xEB => {
                // JMP rel8
                // Cycles: 10 (3 fetch + 7 EU)

                let rel8 = get_operand!(self.i.operand1_type, OperandType::Relative8);
                self.ip = Cpu::relative_offset_u16(self.ip, rel8 as i16 + self.i.size as i16 );
                /*
                if let OperandType::Relative8(rel8) = self.i.operand1_type {
                    self.ip = util::relative_offset_u16(self.ip, rel8 as i16 + self.i.size as i16 );
                }*/

                self.biu_suspend_fetch(); // Immediately suspend
                self.cycles(5);
                self.biu_queue_flush();
                self.cycles(2);
                jump = true
            }
            0xEC => {
                // IN al, dx
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap(); 
                let in_byte = io_bus.read_u8(op2_value);
                self.set_register8(Register8::AL, in_byte);

                #[cfg(feature = "cpu_validator")]
                self.validator.as_mut().unwrap().discard_op();
            }
            0xED => {
                // IN ax, dx
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap(); 
                let in_byte = io_bus.read_u8(op2_value);
                self.set_register16(Register16::AX, in_byte as u16);

                #[cfg(feature = "cpu_validator")]
                self.validator.as_mut().unwrap().discard_op();
            }
            0xEE => {
                // OUT dx, al
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();                

                io_bus.write_u8(op1_value as u16, op2_value);

                #[cfg(feature = "cpu_validator")]
                self.validator.as_mut().unwrap().discard_op();          
            }
            0xEF => {
                // OUT dx, ax
                // On the 8088, this does two writes to successive port #'s 
                let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();

                // Write first 8 bits to first port
                io_bus.write_u8(op1_value, (op2_value & 0xFF) as u8);
                // Write next 8 bits to port + 1
                io_bus.write_u8(op1_value + 1, (op2_value >> 8 & 0xFF) as u8);

                #[cfg(feature = "cpu_validator")]
                self.validator.as_mut().unwrap().discard_op();
            }
            0xF0 => {
                unhandled = true;
            }
            0xF1 => {
                // Does nothing?
                self.cycle();
            }
            0xF2 => {
                unhandled = true;
            }
            0xF3 => {
                unhandled = true;
            }
            0xF4 => {
                // HLT - Halt
                self.halted = true;
                log::trace!("Halted at [{:05X}]", Cpu::calc_linear_address(self.cs, self.ip));
                self.cycles(2);
            }
            0xF5 => {
                // CMC - Complement (invert) Carry Flag
                let carry_flag = self.get_flag(Flag::Carry);
                self.set_flag_state(Flag::Carry, !carry_flag);
            }
            0xF6 => {
                // Miscellaneous Opcode Extensions, r/m8, imm8
                match self.i.mnemonic {

                    Mnemonic::TEST => {
                        let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        let op2_value = self.read_operand8(self.i.operand2_type, self.i.segment_override).unwrap();
                        // Don't use result, just set flags
                        let _result = self.math_op8(self.i.mnemonic, op1_value, op2_value);
                    }
                    Mnemonic::NOT => {
                        let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        let result = self.math_op8(self.i.mnemonic, op1_value, 0);
                        self.write_operand8(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                    }
                    Mnemonic::NEG => {
                        let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        let result = self.math_op8(self.i.mnemonic, op1_value, 0);
                        self.write_operand8(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                    }
                    Mnemonic::MUL => {
                        let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Multiply handles writing to ax
                        self.multiply_u8(op1_value);
                    }
                    Mnemonic::IMUL => {
                        let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Multiply handles writing to ax
                        self.multiply_i8(op1_value as i8);
                    }                    
                    Mnemonic::DIV => {
                        let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Divide handles writing to dx:ax
                        let success = self.divide_u8(op1_value);
                        if !success {
                            exception = CpuException::DivideError;
                        }
                        // TODO: Handle DIV exceptions
                    }          
                    Mnemonic::IDIV => {
                        let op1_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Divide handles writing to dx:ax
                        let success = self.divide_i8(op1_value);
                        if !success {
                            exception = CpuException::DivideError;
                        }
                        // TODO: Handle DIV exceptions
                    }                                 
                    _=> unhandled = true
                }
                handled_override = true;
            }
            0xF7 => {
                // Miscellaneous Opcode Extensions, r/m16, imm16
                match self.i.mnemonic {

                    Mnemonic::TEST => {
                        let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        let op2_value = self.read_operand16(self.i.operand2_type, self.i.segment_override).unwrap();
                        // Don't use result, just set flags
                        let _result = self.math_op16(self.i.mnemonic, op1_value, op2_value);
                    }
                    Mnemonic::NOT => {
                        let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        let result = self.math_op16(self.i.mnemonic, op1_value, 0);
                        self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                    }
                    Mnemonic::NEG => {
                        let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        let result = self.math_op16(self.i.mnemonic, op1_value, 0);
                        self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                    }
                    Mnemonic::MUL => {
                        let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Multiply handles writing to ax
                        self.multiply_u16(op1_value);
                    }
                    Mnemonic::IMUL => {
                        let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Multiply handles writing to dx:ax
                        self.multiply_i16(op1_value as i16);
                    }
                    Mnemonic::DIV => {
                        let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Divide handles writing to dx:ax
                        let success = self.divide_u16(op1_value);
                        if !success {
                            exception = CpuException::DivideError;
                        }
                        // TODO: Handle DIV exceptions
                    }
                    Mnemonic::IDIV => {
                        let op1_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Divide handles writing to dx:ax
                        let success = self.divide_i16(op1_value);
                        if !success {
                            exception = CpuException::DivideError;
                        }
                    }
                    _=> unhandled = true
                }
                handled_override = true;
            }
            0xF8 => {
                // CLC - Clear Carry Flag
                self.clear_flag(Flag::Carry);
                self.cycle()
            }
            0xF9 => {
                // STC - Set Carry Flag
                self.set_flag(Flag::Carry);
                self.cycle()
            }
            0xFA => {
                // CLI - Clear Interrupt Flag
                self.clear_flag(Flag::Interrupt);
                self.cycle()
            }
            0xFB => {
                // STI - Set Interrupt Flag
                self.set_flag(Flag::Interrupt);
                self.cycle()
            }
            0xFC => {
                // CLD - Clear Direction Flag
                self.clear_flag(Flag::Direction);
                self.cycle()
            }
            0xFD => {
                // STD = Set Direction Flag
                self.set_flag(Flag::Direction);
                self.cycle()
            }
            0xFE => {
                // INC/DEC r/m8
                // Technically only the INC and DEC froms of this group are valid. However, the other operands do 8 bit sorta-broken versions
                // of CALL, JMP and PUSH. The behavior implemented here was derived from experimentation with a real 8088 CPU.
                match self.i.mnemonic {
                    // INC/DEC r/m16
                    Mnemonic::INC | Mnemonic::DEC => {
                        let op_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        let result = self.math_op8(self.i.mnemonic, op_value, 0);
                        self.write_operand8(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                    },
                    // Call Near
                    Mnemonic::CALL => {

                        if let OperandType::AddressingMode(mode) = self.i.operand1_type {
                            // Reads only 8 bit operand from modrm.
                            let ptr8 = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                            
                            // Push only 8 bits of next IP onto stack
                            let next_i = self.ip + (self.i.size as u16);
                            self.push_u8((next_i & 0xFF) as u8, ReadWriteFlag::Normal);

                            // temporary timings
                            self.biu_suspend_fetch();
                            self.cycles(4);
                            self.biu_queue_flush();

                            // Set only lower 8 bits of IP, upper bits FF
                            self.ip = 0xFF00 | ptr8 as u16;
                        }
                        else if let OperandType::Register8(reg) = self.i.operand1_type {
                            
                            // Push only 8 bits of next IP onto stack
                            let next_i = self.ip + (self.i.size as u16);
                            self.push_u8((next_i & 0xFF) as u8, ReadWriteFlag::Normal);

                            // temporary timings
                            self.biu_suspend_fetch();
                            self.cycles(4);
                            self.biu_queue_flush();
                            
                            // If this form uses a register operand, the full 16 bits are copied to IP.
                            self.ip = self.get_register16(Cpu::reg8to16(reg));
                        }
                        jump = true;
                    }
                    // Call Far
                    Mnemonic::CALLF => {
                        if let OperandType::AddressingMode(mode) = self.i.operand1_type {
                            let (ea_segment_value, ea_segment, ea_offset) = self.calc_effective_address(mode, SegmentOverride::None);

                            // Read one byte of offset and one byte of segment
                            let offset_addr = Cpu::calc_linear_address(ea_segment_value, ea_offset);
                            let segment_addr = Cpu::calc_linear_address(ea_segment_value, ea_offset + 2);
                            let offset = self.biu_read_u8(ea_segment, offset_addr);
                            let segment = self.biu_read_u8(ea_segment, segment_addr);

                            self.cycles(4);
                            self.biu_suspend_fetch();
                            self.cycles(3);
    
                            // Push low byte of CS
                            self.push_u8((self.cs & 0x00FF) as u8, ReadWriteFlag::Normal);
                            let next_i = self.ip.wrapping_add(self.i.size as u16);
                            // Push low byte of next IP
                            self.push_u8((next_i & 0x00FF) as u8, ReadWriteFlag::Normal);
    
                            self.cycles(4);
                            self.biu_queue_flush();
                            self.cycles(3);
    
                            self.cs = 0xFF00 | segment as u16;
                            self.ip = 0xFF00 | offset as u16;
                            jump = true;
                        }
                        else if let OperandType::Register8(reg) = self.i.operand1_type {

                            // Read one byte from DS:0004 (weird?) and don't do anything with it.
                            let _ = self.biu_read_u8(Segment::DS, 0x0004);

                            // Push low byte of CS
                            self.push_u8((self.cs & 0x00FF) as u8, ReadWriteFlag::Normal);
                            let next_i = self.ip.wrapping_add(self.i.size as u16);
                            // Push low byte of next IP
                            self.push_u8((next_i & 0x00FF) as u8, ReadWriteFlag::Normal);

                            // temporary timings
                            self.biu_suspend_fetch();
                            self.cycles(4);
                            self.biu_queue_flush();
                            
                            // If this form uses a register operand, the full 16 bits are copied to IP.
                            self.ip = self.get_register16(Cpu::reg8to16(reg));
                        }
                    }
                    // Jump to memory r/m16
                    Mnemonic::JMP => {
                        // Reads only 8 bit operand from modrm.
                        let ptr8 = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();

                        // Set only lower 8 bits of IP, upper bits FF
                        self.ip = 0xFF00 | ptr8 as u16;

                        self.biu_suspend_fetch();
                        self.cycles(4);
                        self.biu_queue_flush();
                        jump = true;
                    }
                    // Jump Far
                    Mnemonic::JMPF => {
                        if let OperandType::AddressingMode(mode) = self.i.operand1_type {
                            let (ea_segment_value, ea_segment, ea_offset) = self.calc_effective_address(mode, SegmentOverride::None);

                            // Read one byte of offset and one byte of segment
                            let offset_addr = Cpu::calc_linear_address(ea_segment_value, ea_offset);
                            let segment_addr = Cpu::calc_linear_address(ea_segment_value, ea_offset + 2);
                            let offset = self.biu_read_u8(ea_segment, offset_addr);
                            let segment = self.biu_read_u8(ea_segment, segment_addr);

                            self.biu_suspend_fetch();
                            self.cycles(4);
                            self.biu_queue_flush();

                            self.cs = 0xFF00 | segment as u16;
                            self.ip = 0xFF00 | offset as u16;
                            jump = true;                     
                        }
                        else if let OperandType::Register8(reg) = self.i.operand1_type {

                            // Read one byte from DS:0004 (weird?) and don't do anything with it.
                            let _ = self.biu_read_u8(Segment::DS, 0x0004);

                            // temporary timings
                            self.biu_suspend_fetch();
                            self.cycles(4);
                            self.biu_queue_flush();
                            
                            // If this form uses a register operand, the full 16 bits are copied to IP.
                            self.ip = self.get_register16(Cpu::reg8to16(reg));
                        }
                    }
                    // Push Byte onto stack
                    Mnemonic::PUSH => {
                        // Read one byte from rm
                        let op_value = self.read_operand8(self.i.operand1_type, self.i.segment_override).unwrap();
                        // Write one byte to stack
                        self.push_u8(op_value, ReadWriteFlag::RNI);
                    }                                                           
                    _ => {
                        unhandled = true;
                    }
                }

                // cycles ?
                handled_override = true;
            }
            0xFF => {
                // Several opcode extensions here
                match self.i.mnemonic {
                    // INC/DEC r/m16
                    Mnemonic::INC | Mnemonic::DEC => {
                        let op_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        let result = self.math_op16(self.i.mnemonic, op_value, 0);
                        self.write_operand16(self.i.operand1_type, self.i.segment_override, result, ReadWriteFlag::RNI);
                    },
                    // Call Near
                    Mnemonic::CALL => {
                        let ptr16 = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        //log::trace!("CALL: Destination [{:04X}]", ptr16);
                        
                        // Push return address (next instruction offset) onto stack
                        let next_i = self.ip + (self.i.size as u16);
                        self.push_u16(next_i, ReadWriteFlag::Normal);

                        // Add to call stack
                        if self.call_stack.len() == CPU_CALL_STACK_LEN {
                            self.call_stack.pop_front();
                        }                
                        self.call_stack.push_back(CallStackEntry::Call(self.cs, self.ip, ptr16));
                        
                        // temporary timings
                        self.biu_suspend_fetch();
                        self.cycles(4);
                        self.biu_queue_flush();

                        self.ip = ptr16;
                        jump = true;
                    }
                    // Call Far
                    Mnemonic::CALLF => {
                        let (segment, offset) = 
                            self.read_operand_farptr(
                                self.i.operand1_type, 
                                self.i.segment_override, 
                                ReadWriteFlag::Normal).unwrap();

                        // Push return address of next instruction

                        self.cycles(4);
                        self.biu_suspend_fetch();
                        self.cycles(3);

                        self.push_register16(Register16::CS, ReadWriteFlag::Normal);
                        let next_i = self.ip + (self.i.size as u16);
                        self.push_u16(next_i, ReadWriteFlag::Normal);

                        self.cycles(4);
                        self.biu_queue_flush();
                        self.cycles(3);

                        self.call_stack.push_back(CallStackEntry::CallF(self.cs, self.ip, segment, offset));

                        self.cs = segment;
                        self.ip = offset;

                        // Add to call stack
                        if self.call_stack.len() == CPU_CALL_STACK_LEN {
                            self.call_stack.pop_front();
                        }

                        // log geoworks crap
                        if self.i.segment_override == SegmentOverride::SS {

                            /*
                            let addr = Cpu::calc_linear_address_seg(&self, Segment::SS, 0x000c);

                            let (offset, _) = self.bus.read_u16(addr as usize).unwrap();
                            let (segment, _) = self.bus.read_u16((addr + 2) as usize).unwrap();
                            */

                            log::trace!("ptr ss:[0x00c]: {:04X}:{:04X}", segment, offset);
                        }

                        //log::trace!("CALLF: Destination [{:04X}:{:04X}]", segment, offset);
                        jump = true;
                    }
                    // Jump to memory r/m16
                    Mnemonic::JMP => {
                        let ptr16 = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();

                        self.ip = ptr16;

                        self.biu_suspend_fetch();
                        self.cycles(4);
                        self.biu_queue_flush();
                        jump = true;
                    }
                    // Jump Far
                    Mnemonic::JMPF => {

                        let (segment, offset) = self.read_operand_farptr(self.i.operand1_type, self.i.segment_override, ReadWriteFlag::Normal).unwrap();

                        self.cs = segment;
                        self.ip = offset;

                        // temporary timings
                        self.biu_suspend_fetch();
                        self.cycles(4);
                        self.biu_queue_flush();
                        jump = true;

                        log::trace!("JMPF: Destination [{:04X}:{:04X}]", segment, offset);
                    }                    
                    // Push Word onto stack
                    Mnemonic::PUSH => {
                        let op_value = self.read_operand16(self.i.operand1_type, self.i.segment_override).unwrap();
                        self.push_u16(op_value, ReadWriteFlag::RNI);
                    }                    
                    _=> {
                        unhandled = true;
                    }
                }
                handled_override = true;
                // cycles ?
            }
            _ => {
                return ExecutionResult::UnsupportedOpcode(self.i.opcode);
            }

        }

        match self.i.segment_override {
            SegmentOverride::None => {},
            _ => {
                //Check that we properly handled override. No longer panics as IBM DOS 1.0 has a stray 'cs' override
                if !handled_override {
                    log::warn!("Unhandled segment override at [{:04X}:{:04X}]: {:02X}", self.cs, self.ip, self.i.opcode);
                }
            }

        }

        if unhandled {
            ExecutionResult::UnsupportedOpcode(self.i.opcode)
        }
        else {
            if self.halted && !self.get_flag(Flag::Interrupt) {
                // CPU was halted with interrupts disabled - will not continue
                ExecutionResult::Halt
            }
            else if jump {
                ExecutionResult::OkayJump
            }
            else if self.in_rep {
                ExecutionResult::OkayRep
            }
            else {
                match exception {
                    CpuException::DivideError => ExecutionResult::ExceptionError(exception),
                    CpuException::NoException => ExecutionResult::Okay
                }                
            }
        }
    }
}