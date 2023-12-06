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
    pub latest_cycle_count: u32,
    pub page_accessed: BTreeSet<u32>,
    pub latest_io_addrs: Vec<u32>,
    pub latest_accessed_new_pages: Vec<u32>,
    pub significant_cycles: Vec<(Vec<u32>, Vec<u32>, u32, u32, u32, u32)>,
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
            latest_cycle_count: 0,
            latest_io_addrs: Vec::new(),
            page_accessed: BTreeSet::new(),
            latest_accessed_new_pages: Vec::new(),
            significant_cycles: Vec::new(),
        }
    }
}

impl CycleTracer {
    pub fn handle_event(&mut self, event: TraceEvent) {
        match event {
            TraceEvent::InstructionStart { cycle, pc, insn } => {
                if cycle - self.latest_cycle_count >= 1094 {
                    self.significant_cycles.push((
                        self.latest_io_addrs.clone(),
                        self.latest_accessed_new_pages.clone(),
                        pc,
                        cycle,
                        insn,
                        self.latest_cycle_count,
                    ));
                }
                self.latest_io_addrs.clear();
                self.latest_accessed_new_pages.clear();
                self.latest_cycle_count = cycle;
                self.num_instructions += 1;

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
            TraceEvent::MemorySet { addr, value } => {
                let page_idx = addr >> 10;
                if !self.page_accessed.contains(&page_idx) {
                    self.page_accessed.insert(page_idx);
                    self.latest_accessed_new_pages.push(page_idx);
                }
                self.latest_io_addrs.push(addr);

                if addr >= self.trace_msg_channel && addr < self.trace_msg_channel + 512 {
                    self.msg_channel_buffer[(addr - self.trace_msg_channel) as usize] =
                        (value & 0xff) as u8;
                    self.msg_channel_buffer[(addr - self.trace_msg_channel + 1) as usize] =
                        ((value >> 8) & 0xff) as u8;
                    self.msg_channel_buffer[(addr - self.trace_msg_channel + 2) as usize] =
                        ((value >> 16) & 0xff) as u8;
                    self.msg_channel_buffer[(addr - self.trace_msg_channel + 3) as usize] =
                        ((value >> 24) & 0xff) as u8;
                }
                if addr == self.trace_msg_len_channel {
                    let str =
                        String::from_utf8(self.msg_channel_buffer[0..value as usize].to_vec())
                            .unwrap();
                    self.pending_records.push(PendingRecord {
                        name: str,
                        num_pending_records: self.pending_records.len(),
                        cur_num_instructions: self.num_instructions,
                        cur_num_cycles: self.latest_cycle_count,
                        start_significant_cycles: self.significant_cycles.len(),
                    });
                }
                if addr == self.trace_cycle_channel {
                    let elem = self.pending_records.pop().unwrap();
                    self.finished_records.push(FinishedRecord {
                        name: elem.name,
                        indents: elem.num_pending_records,
                        num_instructions: self.num_instructions - elem.cur_num_instructions,
                        num_cycles: self.latest_cycle_count - elem.cur_num_cycles,
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
                        let (addrs, pages, pc, num_cycles, insn, num_previous_cycles) =
                            &self.significant_cycles[i];

                        let addr_string = if addrs.is_empty() {
                            "".to_string()
                        } else {
                            if addrs.len() > 4 {
                                let str = addrs
                                    .iter()
                                    .take(4)
                                    .map(|x| format!("{:#08x}", x).to_string())
                                    .collect::<Vec<String>>();
                                format!(" that accesses {}, ...", str.join(", ").white())
                            } else {
                                let str = addrs
                                    .iter()
                                    .map(|x| format!("{:#08x}", x).to_string())
                                    .collect::<Vec<String>>();
                                format!(" that accesses {}", str.join(", ").white())
                            }
                        };

                        let page_string = if pages.is_empty() {
                            "".to_string()
                        } else {
                            if pages.len() > 4 {
                                let str = pages
                                    .iter()
                                    .take(4)
                                    .map(|x| format!("{:#08x}", x << 10).to_string())
                                    .collect::<Vec<String>>();
                                format!(" and pages-in {}, ...", str.join(", ").white())
                            } else {
                                let str = pages
                                    .iter()
                                    .map(|x| format!("{:#08x}", x << 10).to_string())
                                    .collect::<Vec<String>>();
                                format!(" and pages-in {}", str.join(", ").white())
                            }
                        };

                        use raki::decode::Decode;
                        use raki::Isa;

                        let decode = format!("{}", insn.decode(Isa::Rv32).unwrap());

                        cur_string += &format!(
                            "{}Cycle: {} => {}: {} at {}{}{} takes {} cycles\n",
                            compute_indent(cur_level + 1),
                            num_previous_cycles,
                            num_cycles,
                            decode.blue(),
                            format!("{:#08x}", pc).white(),
                            addr_string,
                            page_string,
                            format!("{}", num_cycles - num_previous_cycles).blue(),
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
