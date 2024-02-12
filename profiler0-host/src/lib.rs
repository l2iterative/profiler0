use risc0_zkvm::TraceEvent;
use std::collections::{BTreeSet, HashMap};

pub struct FinishedRecord {
    pub name: String,
    pub indents: usize,
    pub num_instructions: u32,
    pub num_cycles: u32,
    pub start_significant_cycles: usize,
    pub end_significant_cycles: usize,
}

pub struct PendingRecord {
    pub name: String,
    pub num_pending_records: usize,
    pub cur_num_instructions: u32,
    pub cur_num_cycles: u32,
    pub start_significant_cycles: usize,
}

pub struct SignificantCycleRecord {
    pub latest_io_addrs: Vec<u32>,
    pub latest_accessed_new_pages: Vec<u32>,
    pub pc: u32,
    pub current_cycle: u32,
    pub insn: u32,
    pub previous_cycle: u32,
    pub previous_instruction_is_jmp: (u32, u32),
    pub previous_instruction_is_branch: (u32, u32),
    pub first_instruction_new_segment: bool,
}

pub struct CycleTracer {
    pub init_state_machine: u32,
    pub trace_msg_channel: u32,
    pub trace_msg_len_channel: u32,
    pub trace_cycle_channel: u32,
    pub finished_records: Vec<FinishedRecord>,
    pub pending_records: Vec<PendingRecord>,
    pub msg_channel_buffer: [u8; 516],
    pub msg_len_channel_buffer: u32,
    pub num_instructions: u32,
    pub previous_cycle_count: u32,
    pub page_accessed: BTreeSet<u32>,
    pub latest_io_addrs: Vec<u32>,
    pub latest_accessed_new_pages: Vec<u32>,
    pub significant_cycles: Vec<SignificantCycleRecord>,
    pub previous_pc: u32,
    pub previous_insn: u32,
    pub previous_instruction_is_jmp: (u32, u32),
    pub previous_instruction_after_jmp: (u32, u32),
    pub previous_instruction_is_branch: (u32, u32),
    pub previous_instruction_after_branch: (u32, u32),
}

impl Default for CycleTracer {
    fn default() -> Self {
        Self {
            init_state_machine: 0,
            trace_msg_channel: 0,
            trace_msg_len_channel: 0,
            trace_cycle_channel: 0,
            finished_records: vec![],
            pending_records: vec![],
            msg_channel_buffer: [0u8; 516],
            msg_len_channel_buffer: 0,
            num_instructions: 0,
            previous_cycle_count: 0,
            latest_io_addrs: Vec::new(),
            latest_accessed_new_pages: Vec::new(),
            page_accessed: BTreeSet::new(),
            significant_cycles: Vec::new(),
            previous_pc: 0,
            previous_insn: 0,
            previous_instruction_is_jmp: (0, 0),
            previous_instruction_after_jmp: (0, 0),
            previous_instruction_is_branch: (0, 0),
            previous_instruction_after_branch: (0, 0),
        }
    }
}

impl CycleTracer {
    pub fn handle_event(&mut self, event: TraceEvent) {
        match event {
            TraceEvent::InstructionStart { cycle, pc, insn } => {
                let mut is_new_segment = false;
                if (cycle >> 20) != (self.previous_cycle_count >> 20) {
                    // a new segment has started
                    self.page_accessed.clear();
                    is_new_segment = true;
                }

                let mut page_idx = self.previous_pc >> 10;
                while !self.page_accessed.contains(&page_idx) {
                    self.page_accessed.insert(page_idx);
                    self.latest_accessed_new_pages.push(page_idx);
                    page_idx = (0x0D00_0000 + page_idx * 32) >> 10;
                }

                for addr in self.latest_io_addrs.iter() {
                    let mut page_idx = addr >> 10;
                    while !self.page_accessed.contains(&page_idx) {
                        self.page_accessed.insert(page_idx);
                        self.latest_accessed_new_pages.push(page_idx);
                        page_idx = (0x0D00_0000 + page_idx * 32) >> 10;
                    }
                }

                if cycle - self.previous_cycle_count >= 1094 {
                    self.significant_cycles.push(SignificantCycleRecord {
                        latest_io_addrs: self.latest_io_addrs.clone(),
                        latest_accessed_new_pages: self.latest_accessed_new_pages.clone(),
                        pc: self.previous_pc,
                        current_cycle: cycle,
                        insn: self.previous_insn,
                        previous_cycle: self.previous_cycle_count,
                        previous_instruction_is_jmp: self.previous_instruction_after_jmp,
                        previous_instruction_is_branch: self.previous_instruction_after_branch,
                        first_instruction_new_segment: is_new_segment,
                    });
                }

                self.latest_io_addrs.clear();
                self.latest_accessed_new_pages.clear();

                self.previous_pc = pc;
                self.previous_insn = insn;
                self.previous_cycle_count = cycle;
                self.previous_instruction_after_jmp = self.previous_instruction_is_jmp;
                self.previous_instruction_after_branch = self.previous_instruction_is_branch;
                self.num_instructions += 1;

                if (insn & 0x7f == 0x6f) || (insn & 0x7f == 0x67) {
                    self.previous_instruction_is_jmp = (pc, insn);
                } else {
                    self.previous_instruction_is_jmp = (0, 0);
                }

                if insn & 0x7f == 0b1100011 {
                    self.previous_instruction_is_branch = (pc, insn);
                } else {
                    self.previous_instruction_is_branch = (0, 0);
                }

                if self.init_state_machine == 999 {
                    return;
                }

                if insn == 0x00000013 {
                    // nop
                    if self.init_state_machine == 0 {
                        self.init_state_machine = 1;
                    } else {
                        self.init_state_machine = 0;
                    }
                } else if insn == 0xcdcdd037 {
                    // lui zero, 0xcdcdd
                    if self.init_state_machine == 1 {
                        self.init_state_machine = 2;
                    } else {
                        self.init_state_machine = 0;
                    }
                } else if insn == 0xdcd00013 {
                    // li zero, -0x233
                    if self.init_state_machine == 2 {
                        self.init_state_machine = 3;
                    } else {
                        self.init_state_machine = 0;
                    }
                } else if insn & 0x00000fff == 0x017 {
                    // auipc zero, ??? (12 bits)
                    if self.init_state_machine == 3 {
                        self.trace_msg_channel = pc + (insn & 0xfffff000);
                        self.init_state_machine = 4;
                    } else if self.init_state_machine == 5 {
                        self.trace_msg_len_channel = pc + (insn & 0xfffff000);
                        self.init_state_machine = 6;
                    } else if self.init_state_machine == 7 {
                        self.trace_cycle_channel = pc + (insn & 0xfffff000);
                        self.init_state_machine = 8;
                    } else {
                        self.init_state_machine = 0;
                    }
                } else if insn & 0x000fffff == 0x13 {
                    let abs = (insn >> 20) & 0xfff;
                    let neg = ((insn >> 31) & 1) == 1;

                    // li zero, ??? (12 bits, signed)
                    if self.init_state_machine == 4 {
                        if neg {
                            self.trace_msg_channel -= 4096 - abs;
                        } else {
                            self.trace_msg_channel += abs;
                        }
                        self.init_state_machine = 5;
                    } else if self.init_state_machine == 6 {
                        if neg {
                            self.trace_msg_len_channel -= 4096 - abs;
                        } else {
                            self.trace_msg_len_channel += abs;
                        }
                        self.init_state_machine = 7;
                    } else if self.init_state_machine == 8 {
                        if neg {
                            self.trace_cycle_channel -= 4096 - abs;
                        } else {
                            self.trace_cycle_channel += abs;
                        }

                        self.init_state_machine = 999
                    } else {
                        self.init_state_machine = 0;
                    }
                }
            }
            TraceEvent::RegisterSet { .. } => {}
            TraceEvent::MemorySet { addr, region } => {
                self.latest_io_addrs.push(addr);

                if addr >= self.trace_msg_channel && addr < self.trace_msg_channel + 512 {
                    let start = (addr - self.trace_msg_channel) as usize;
                    self.msg_channel_buffer[start..(start + region.len())].copy_from_slice(&region);
                }
                if addr == self.trace_msg_len_channel {
                    let value = (region[0] as u32)
                        + ((region[1] as u32) << 8)
                        + ((region[2] as u32) << 16)
                        + ((region[3] as u32) << 24);
                    let str =
                        String::from_utf8(self.msg_channel_buffer[0..value as usize].to_vec())
                            .unwrap();
                    self.pending_records.push(PendingRecord {
                        name: str,
                        num_pending_records: self.pending_records.len(),
                        cur_num_instructions: self.num_instructions,
                        cur_num_cycles: self.previous_cycle_count,
                        start_significant_cycles: self.significant_cycles.len(),
                    });
                }
                if addr == self.trace_cycle_channel {
                    let elem = self.pending_records.pop().unwrap();
                    self.finished_records.push(FinishedRecord {
                        name: elem.name,
                        indents: elem.num_pending_records,
                        num_instructions: self.num_instructions - elem.cur_num_instructions,
                        num_cycles: self.previous_cycle_count - elem.cur_num_cycles,
                        start_significant_cycles: elem.start_significant_cycles,
                        end_significant_cycles: self.significant_cycles.len(),
                    });
                }
            }
        }
    }

    pub fn print(&self) {
        use colored::Colorize;

        fn compute_indent(indent_amount: usize) -> String {
            let mut indent = String::new();
            for _ in 0..indent_amount {
                indent.push_str(&"····");
            }
            if indent_amount != 0 {
                indent.push_str(" ");
            }
            indent
        }

        let mut significant_cycles_shown = vec![false; self.significant_cycles.len()];

        let mut output: HashMap<usize, String> = HashMap::new();
        let mut cur_level = 0;

        for report in self.finished_records.iter() {
            let mut cur_string = if report.indents >= cur_level {
                cur_level = report.indents;
                let mut cur_string = output.get(&cur_level).cloned().unwrap_or_default();
                cur_string += &format!(
                    "{}{}: {} cycles, {} instructions\n",
                    compute_indent(cur_level),
                    report.name,
                    format!("{}", report.num_cycles).blue(),
                    format!("{}", report.num_instructions).blue(),
                );
                cur_string
            } else if report.indents < cur_level {
                let tmp_string = output.get(&cur_level).cloned().unwrap_or_default();
                output.insert(cur_level, "".to_string());

                cur_level = report.indents;

                let mut cur_string = output.get(&cur_level).cloned().unwrap_or_default();
                cur_string += &format!(
                    "{}{}: {} cycles, {} instructions\n{}",
                    compute_indent(cur_level),
                    report.name,
                    format!("{}", report.num_cycles).blue(),
                    format!("{}", report.num_instructions).blue(),
                    tmp_string
                );
                cur_string
            } else {
                unimplemented!()
            };

            if report.end_significant_cycles != report.start_significant_cycles {
                for i in report.start_significant_cycles..report.end_significant_cycles {
                    if significant_cycles_shown[i] == false {
                        let significant_cycle = &self.significant_cycles[i];

                        let addr_string = if significant_cycle.latest_io_addrs.is_empty() {
                            "".to_string()
                        } else {
                            if significant_cycle.latest_io_addrs.len() > 4 {
                                let mut sorted = significant_cycle.latest_io_addrs.clone();
                                sorted.sort();

                                let str = sorted
                                    .iter()
                                    .take(4)
                                    .map(|x| format!("{:#08x}", x).to_string())
                                    .collect::<Vec<String>>();
                                let last = format!("{:#08x}", sorted[sorted.len() - 1]);
                                format!(
                                    " writes to {}, ..., {}",
                                    str.join(", ").white(),
                                    last.white()
                                )
                            } else {
                                let mut sorted = significant_cycle.latest_io_addrs.clone();
                                sorted.sort();

                                let str = sorted
                                    .iter()
                                    .map(|x| format!("{:#08x}", x).to_string())
                                    .collect::<Vec<String>>();
                                format!(" writes to {}", str.join(", ").white())
                            }
                        };

                        let page_string = if significant_cycle.latest_accessed_new_pages.is_empty()
                        {
                            "".to_string()
                        } else {
                            if significant_cycle.latest_accessed_new_pages.len() > 4 {
                                let mut sorted =
                                    significant_cycle.latest_accessed_new_pages.clone();
                                sorted.sort();

                                let str = sorted
                                    .iter()
                                    .take(4)
                                    .map(|x| format!("{:#08x}", x << 10).to_string())
                                    .collect::<Vec<String>>();
                                let last = format!("{:#08x}", sorted[sorted.len() - 1] << 10,);
                                format!(
                                    " marks pages {}, ..., {} as dirty",
                                    str.join(", ").white(),
                                    last.white()
                                )
                            } else {
                                let mut sorted =
                                    significant_cycle.latest_accessed_new_pages.clone();
                                sorted.sort();

                                let str = sorted
                                    .iter()
                                    .map(|x| format!("{:#08x}", x << 10).to_string())
                                    .collect::<Vec<String>>();
                                format!(" marks pages {} as dirty", str.join(", ").white())
                            }
                        };

                        let prefix_word = if !significant_cycle.latest_accessed_new_pages.is_empty()
                            || !significant_cycle.latest_io_addrs.is_empty()
                        {
                            " that".to_string()
                        } else {
                            "".to_string()
                        };

                        let glue_word = if !significant_cycle.latest_accessed_new_pages.is_empty()
                            && !significant_cycle.latest_io_addrs.is_empty()
                        {
                            " and".to_string()
                        } else {
                            "".to_string()
                        };

                        let first_insn_word = if significant_cycle.first_instruction_new_segment {
                            " (first instruction in the new segment)".to_string()
                        } else {
                            "".to_string()
                        };

                        use raki::decode::Decode;
                        use raki::Isa;

                        let jump_string = if significant_cycle.previous_instruction_is_jmp.0 == 0 {
                            "".to_string()
                        } else {
                            format!(
                                ", due to {} at {},",
                                format!(
                                    "{}",
                                    significant_cycle
                                        .previous_instruction_is_jmp
                                        .1
                                        .decode(Isa::Rv32)
                                        .unwrap()
                                )
                                .blue(),
                                format!("{:#08x}", significant_cycle.previous_instruction_is_jmp.0)
                                    .white(),
                            )
                        };

                        let branch_string = if significant_cycle.previous_instruction_is_branch.0
                            == 0
                            || significant_cycle.previous_instruction_is_branch.0 + 4
                                == significant_cycle.pc
                        {
                            "".to_string()
                        } else {
                            format!(
                                ", due to {} at {},",
                                format!(
                                    "{}",
                                    significant_cycle
                                        .previous_instruction_is_branch
                                        .1
                                        .decode(Isa::Rv32)
                                        .unwrap()
                                )
                                .blue(),
                                format!(
                                    "{:#08x}",
                                    significant_cycle.previous_instruction_is_branch.0
                                )
                                .white(),
                            )
                        };

                        let decode =
                            format!("{}", significant_cycle.insn.decode(Isa::Rv32).unwrap());

                        cur_string += &format!(
                            "{}Cycle: {} => {}: {} at {}{}{}{}{}{}{}{} takes {} cycles\n",
                            compute_indent(cur_level + 1),
                            significant_cycle.previous_cycle,
                            significant_cycle.current_cycle,
                            decode.blue(),
                            format!("{:#08x}", significant_cycle.pc).white(),
                            first_insn_word,
                            jump_string,
                            branch_string,
                            prefix_word,
                            addr_string,
                            glue_word,
                            page_string,
                            format!(
                                "{}",
                                significant_cycle.current_cycle - significant_cycle.previous_cycle
                            )
                            .blue(),
                        );
                    }
                    significant_cycles_shown[i] = true;
                }
            }

            output.insert(cur_level, cur_string);
        }

        println!("{}", output.get(&0).cloned().unwrap_or_default().green());
    }
}
