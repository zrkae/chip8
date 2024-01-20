use std::io::{self, Read};
use std::fs::File;
use std::env;
use std::cell::Cell;

mod gfx;

const LOAD_ADDR: u16 = 0x200;
const SCREEN_HEIGHT: u32 = 32;
const SCREEN_WIDTH: u32 = 64;

thread_local! {
    pub static VERBOSE_OUTPUT: Cell<bool> = Cell::new(false);
}

fn u16_from_nibbles_3(n1: u8, n2: u8, n3: u8) -> u16 {
    ((n1 as u16) << 8) + ((n2 as u16) << 4) + n3 as u16
}

fn u8_from_nibbles_2(n2: u8, n3: u8) -> u8 {
    (n2 << 4) + n3
}

fn binary_coded_decimal(value: u8) -> (u8, u8, u8) {
    (value / 100, value / 10 - value / 100 * 10, value - value / 10 * 10)
}

// Bit map font data, loaded at 0x00 in memory
const FONT_DATA: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0,
    0x20, 0x60, 0x20, 0x20, 0x70,
    0xF0, 0x10, 0xF0, 0x80, 0xF0,
    0xF0, 0x10, 0xF0, 0x10, 0xF0,
    0x90, 0x90, 0xF0, 0x10, 0x10,
    0xF0, 0x80, 0xF0, 0x10, 0xF0,
    0xF0, 0x80, 0xF0, 0x90, 0xF0,
    0xF0, 0x10, 0x20, 0x40, 0x40,
    0xF0, 0x90, 0xF0, 0x90, 0xF0,
    0xF0, 0x90, 0xF0, 0x10, 0xF0,
    0xF0, 0x90, 0xF0, 0x90, 0x90,
    0xE0, 0x90, 0xE0, 0x90, 0xE0,
    0xF0, 0x80, 0x80, 0x80, 0xF0,
    0xE0, 0x90, 0x90, 0x90, 0xE0,
    0xF0, 0x80, 0xF0, 0x80, 0xF0,
    0xF0, 0x80, 0xF0, 0x80, 0x80,
];

// Note: this is not part of the original specification
#[derive(Debug)]
pub enum ChipException {
    InvalidRegister,
    ReturnOutsideSubroutine,
    IllegalInstruction,
    InvalidFontCodePoint,
    DrawingOutOfBounds { offset: usize },
    WaitForKey { register: u8 },
    SkipIfPressed { register: u8 },
    SkipIfNotPressed { register: u8 },
}

struct Chip {
    memory: Box<[u8; 4096]>,
    ip: u16,
    video_memory: Box<[u8; 32*64]>,
    stack: Vec<u16>, 
    // registers V0 - VF
    // VF is a little special, being modified by some instructions
    data_regs: [u8; 16],
    // Actually 12-bits wide
    addr_reg: u16,

    delay_timer: u8,
    sound_timer: u8,
}

impl Default for Chip {
    fn default() -> Self {
        let mut memory = Box::new([0; 4096]);
        memory[..80].copy_from_slice(&FONT_DATA);

        Self {
            ip: LOAD_ADDR,
            memory,
            video_memory: Box::new([0; (SCREEN_WIDTH*SCREEN_HEIGHT) as usize]),
            stack: Vec::new(),
            data_regs: [0; 16],
            addr_reg: 0,
            delay_timer: 0,
            sound_timer: 0,
        }
    }
}

impl Chip {
    fn load_program(&mut self, path: &str) -> io::Result<usize> {
        let n_read = File::open(path)?
                        .read(&mut self.memory[(LOAD_ADDR as usize)..])?;

        if n_read > self.memory.len() {
            println!("ROM might be too large? {} > {}", n_read, self.memory.len()) 
        }

        Ok(n_read)
    }

    // interpret and execute an instruction
    fn exec(&mut self, instr: u16) -> Result<(), ChipException> {
        use ChipException::*;

        let nibbles = [((instr & 0xF000) >> 12) as u8, 
                       ((instr & 0x0F00) >> 8) as u8, 
                       ((instr & 0x00F0) >> 4) as u8, 
                       (instr & 0x000F) as u8];

        if VERBOSE_OUTPUT.get() {
            println!("[ip: {:X}]: {nibbles:X?}", self.ip);
        }

        match nibbles {
            // clear the screen
            [0, 0, 0xE, 0] => {
                self.video_memory.fill(0);
            }
            // return from subroutine
            [0, 0, 0xE, 0xE] => {
                if let Some(addr) = self.stack.pop() {
                    self.ip = addr; 
                } else {
                    return Err(ReturnOutsideSubroutine)
                }
            }
            // call (machine language?) subroutine at addr n1n2n3
            // does the same thing as normal call for now
            [0, n1, n2, n3] => {
                println!("hic sunt dracones: the weird instruction has been encountered. this program might be a bit too 70s");
                // save return address
                self.stack.push(self.ip);
                // jump to subroutine
                self.ip = u16_from_nibbles_3(n1, n2, n3);
            }
            // jmp to n1n2n3
            [1, n1, n2, n3] => {
                self.ip =  u16_from_nibbles_3(n1, n2, n3);
            }
            // call subroutine at addr n1n2n3
            [2, n1, n2, n3] => {
                // save return address
                self.stack.push(self.ip);
                // jump to subroutine
                self.ip = u16_from_nibbles_3(n1, n2, n3);
            }
            // skip the next instruction if n1n2 == regs[x]
            [3, x, n1, n2] => {
                if x > 0xF {
                    return Err(InvalidRegister)
                }
                if self.data_regs[x as usize] == u8_from_nibbles_2(n1, n2) {
                    self.ip += 2; 
                }
            }
            // skip the next instruction if n1n2 != regs[x]
            [4, x, n1, n2] => {
                if x > 0xF {
                    return Err(InvalidRegister)
                }
                if self.data_regs[x as usize] != u8_from_nibbles_2(n1, n2) {
                    self.ip += 2; 
                }
            }
            // skip next instruction if regs[x] == regs[y]
            [5, x, y, 0] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }

                if self.data_regs[x as usize] == self.data_regs[y as usize] {
                    self.ip += 2; 
                }
            }
            // set value of regs[x] to n1n2
            [6, x, n1, n2] => {
                if x > 0xF {
                    return Err(InvalidRegister)
                }
                self.data_regs[x as usize] = u8_from_nibbles_2(n1, n2);
            }
            // add n1n2 to regs[x]
            [7, x, n1, n2] => {
                if x > 0xF {
                    return Err(InvalidRegister)
                }
                let value = u8_from_nibbles_2(n1, n2);
                self.data_regs[x as usize] = self.data_regs[x as usize].wrapping_add(value);
            }
            // set regs[x] = regs[y]
            [8, x, y, 0] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                self.data_regs[x as usize] = self.data_regs[y as usize];
            }
            // set regs[x] = regs[x] | regs[y]
            [8, x, y, 1] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                self.data_regs[x as usize] |= self.data_regs[y as usize];
            }
            // set regs[x] = regs[x] & regs[y]
            [8, x, y, 2] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                self.data_regs[x as usize] &= self.data_regs[y as usize];
            }
            // set regs[x] = regs[x] ^ regs[y]
            [8, x, y, 3] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                self.data_regs[x as usize] ^= self.data_regs[y as usize];
            }
            // add regs[y] to regs[x], set regs[0xF] to 1 if carry, set to 0 if otherwise
            [8, x, y, 4] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                let (new_rx, carry) = self.data_regs[x as usize].overflowing_add(self.data_regs[y as usize]);
                self.data_regs[0xF] = carry as u8;
                self.data_regs[x as usize] = new_rx;
            }
            // subtract regs[y] from regs[x], set regs[0xF] to 1 if borrow, set to 0 otherwise
            [8, x, y, 5] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                let (new_rx, borrow) = self.data_regs[x as usize].overflowing_sub(self.data_regs[y as usize]);
                self.data_regs[0xF] = borrow as u8;
                self.data_regs[x as usize] = new_rx;
            }
            // set regs[x] to regs[y] >> 1, set regs[0xF] to LSb of regs[y] prior to shift
            [8, x, y, 6] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                self.data_regs[x as usize] = self.data_regs[y as usize] >> 1;
                self.data_regs[0xF] = self.data_regs[y as usize] & 1;
            }
            // set regs[x] to regs[y] - regs[x], store if borrow occured in regs[0xF]
            [8, x, y, 7] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                let (new_rx, borrow) = self.data_regs[y as usize].overflowing_sub(self.data_regs[x as usize]);
                self.data_regs[0xF] = borrow as u8;
                self.data_regs[x as usize] = new_rx;
            }
            // store regs[y] << 1 in regs[x], set regs[0xF] to MSb prior to shift
            [8, x, y, 0xE] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                self.data_regs[x as usize] = self.data_regs[y as usize] << 1;
                self.data_regs[0xF] = self.data_regs[y as usize] >> 7;
            }
            // skip the next instruction if regs[x] != regs[y]
            [9, x, y, 0] => {
                if x > 0xF || y > 0xF {
                    return Err(InvalidRegister)
                }
                if self.data_regs[x as usize] != self.data_regs[y as usize] {
                    self.ip += 2;
                }
            }
            // set the address register to n1n2n3
            [0xA, n1, n2, n3] => {
                self.addr_reg = u16_from_nibbles_3(n1, n2, n3);
            }
            // jump to regs[0x0] + n1n2n3
            [0xB, n1, n2, n3] => {
                self.ip = self.data_regs[0] as u16 + u16_from_nibbles_3(n1, n2, n3);
            }
            // Generate a random u8 and apply a n1n2 mask to it 
            [0xC, x, n1, n2] => {
                if x > 0xF {
                    return Err(InvalidRegister)
                }
                self.data_regs[x as usize] = rand::random::<u8>() & u8_from_nibbles_2(n1, n2);
            }
            // draw sprite at (reg[x],reg[y]) with n bytes of data from memory at addr_register
            // every sprite is eight pixels wide (because 8 bits in a byte)
            [0xD, x, y, n] => {
                if VERBOSE_OUTPUT.get() {
                    println!("DRAW CALL: ({},{}), h: {n}", self.data_regs[x as usize], self.data_regs[y as usize]);
                }
                let mut set_flag = false;

                let start_row = self.data_regs[y as usize];
                let start_col = self.data_regs[x as usize];
                // let start_offset = self.data_regs[y as usize] as u32 * SCREEN_WIDTH + self.data_regs[x as usize] as u32;

                for row in 0..n {
                    let row_data = self.memory[(self.addr_reg + row as u16) as usize];
                    for col in 0..8 {
                        let set = 0 < ((row_data >> (7 - col)) & 1);

                        if set {
                            // let pixel_offset = (start_offset + (row * 8 + col) as u32) as usize;
                            let pixel_row = start_row + row;
                            let pixel_col = start_col + col;
                            let pixel_offset = (pixel_row as u32 * SCREEN_WIDTH + pixel_col as u32) as usize;
                            
                            if pixel_offset > self.video_memory.len() {
                                return Err(DrawingOutOfBounds { offset: pixel_offset });
                            } else {
                                self.video_memory[pixel_offset] ^= 1;
                                if self.video_memory[pixel_offset] == 0 {
                                    set_flag = true;
                                }
                            }
                        }
                    }
                }

                self.data_regs[0xF] = set_flag as u8;
            }
            // skip the next instruction if the key stored in regs[x] is pressed
            [0xE, x, 9, 0xE] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                return Err(SkipIfPressed { register: x });
            }
            // skip the next instruction if the key stored in regs[x] is _not_ pressed
            [0xE, x, 0xA, 1] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                return Err(SkipIfNotPressed { register: x });
            }
            // store the current value of delay_timer in regs[x]
            [0xF, x, 0, 0x7] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                self.data_regs[x as usize] = self.delay_timer;
            }
            // wait for the next keypress and store the result in regs[x]
            [0xF, x, 0, 0xA] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                return Err(WaitForKey { register: x });
            }
            // set delay_timer to value of regs[x]
            [0xF, x, 1, 5] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                self.delay_timer = self.data_regs[x as usize];
            }
            // set sound_timer to value of regs[x]
            [0xF, x, 1, 8] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                self.sound_timer = self.data_regs[x as usize];
            }
            // increment add_reg by regs[x]
            [0xF, x, 1, 0xE] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                self.addr_reg = self.addr_reg.wrapping_add(self.data_regs[x as usize] as u16);
            }
            // set addr_reg to point to the font sprite data of value regs[x]
            [0xF, x, 2, 9] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                if self.data_regs[x as usize] > 0xF {
                    return Err(InvalidFontCodePoint)
                }
                self.addr_reg = self.data_regs[x as usize] as u16 * 5; 
            }
            // store the binary coded decimal of regs[x] at add_reg (offset 0,1,2)
            [0xF, x, 3, 3] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                let (d0, d1, d2) = binary_coded_decimal(self.data_regs[x as usize]);
                self.memory[self.addr_reg as usize] = d0;
                self.memory[(self.addr_reg + 1) as usize] = d1;
                self.memory[(self.addr_reg + 2) as usize] = d2;
            }
            // store the values of regs from regs[0] to regs[x] _inclusive_, at addr_reg
            [0xF, x, 5, 5] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                for i in 0..=x {
                    self.memory[(self.addr_reg + i as u16) as usize] = self.data_regs[i as usize];
                }
            }
            // fill regs from regs[0] to regs[x] _inclusive_, from memory starting at addr_reg
            [0xF, x, 6, 5] => {
                if x > 0xF {
                    return Err(InvalidRegister);
                }
                for i in 0..=x {
                    self.data_regs[i as usize] = self.memory[(self.addr_reg + i as u16) as usize];
                }
            }
            _ => return Err(IllegalInstruction),
        };

        Ok(())
    }

    fn cycle(&mut self) -> Result<(), ChipException> {
        // fetch next instruction
        let next = u16::from_be_bytes([self.memory[self.ip as usize], self.memory[(self.ip + 1) as usize]]);
        self.ip += 2; // increment instruction pointer, this might get overriden by a jmp
        self.exec(next)
    }
}

fn die_usage(path: &String) -> ! {
    eprintln!("\
usage: ./{path} [OPTIONS..] [PATH]
Options:
    --help          Show this message
    --verbose | -v  Verbose mode");
    std::process::exit(1);
}

fn handle_args(chip: &mut Chip) {
    let args: Vec<_> = env::args().collect();
    let path = args.first().unwrap();

    if args.len() == 1 {
        die_usage(path);
    }

    // handle intermediate options
    for arg in args.iter()
        .skip(1)
        .take(args.len() - 2) 
    {
        match arg.as_str() {
            "--verbose" | "-v" => {
                VERBOSE_OUTPUT.set(true);
                println!("Verbose mode set.");
            }
            _ => {
                die_usage(path);
            } 
        }
    }

    // last arg should be the path of the binary
    if let Some(arg) = args.last() {
        match chip.load_program(arg) {
            Ok(n) => {
                println!("Loaded {n} Bytes from file '{arg}'.");
            },
            Err(e) => {
                eprintln!("Couldn't load '{arg}' - {e}");
                std::process::exit(1);
            }
        }
    } else {
        die_usage(path);
    }
}

fn main() {
    let mut chip = Chip::default();
    handle_args(&mut chip);
    gfx::spawn_window(chip);
}
