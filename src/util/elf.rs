// elf.rs -- ELF constants

pub const EI_CLASS: usize = 4;
pub const ELFCLASS32: u8 = 1;
pub const ELFCLASS64: u8 = 2;

pub const EI_DATA: usize = 5;
pub const ELFDATA2LSB: u8 = 1;
pub const ELFDATA2MSB: u8 = 2;

pub const E_TYPE: usize = 16;
pub const ET_REL: u16 = 1;
pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;
pub const ET_CORE: u16 = 4;

pub const E_MACHINE: usize = 18;
pub const EM_SPARC: u16 = 2;
pub const EM_386: u16 = 3;
pub const EM_68K: u16 = 4;
pub const EM_MIPS: u16 = 8;
pub const EM_PARISC: u16 = 15;
pub const EM_SPARC32PLUS: u16 = 18;
pub const EM_PPC: u16 = 20;
pub const EM_PPC64: u16 = 21;
pub const EM_S390: u16 = 22;
pub const EM_ARM: u16 = 40;
pub const EM_SH: u16 = 42;
pub const EM_SPARCV9: u16 = 43;
pub const EM_ARC: u16 = 45;
pub const EM_IA_64: u16 = 50;
pub const EM_X86_64: u16 = 62;
pub const EM_ARC_COMPACT: u16 = 93;
pub const EM_ALTERA_NIOS2: u16 = 113;
pub const EM_AARCH64: u16 = 183;
pub const EM_ARC_COMPACT2: u16 = 195;
pub const EM_AMDGPU: u16 = 224;
pub const EM_RISCV: u16 = 243;
pub const EM_BPF: u16 = 247;
pub const EM_ARC_COMPACT3_64: u16 = 253;
pub const EM_ARC_COMPACT3: u16 = 255;
pub const EM_LOONGARCH: u16 = 258;
pub const EM_ALPHA: u16 = 0x9026;

pub const E_ENTRY: usize = 24;
pub const EF_MIPS_ABI: u32 = 0x0000F000;
pub const EF_MIPS_ABI2: u32 = 0x00000020;
pub const E_MIPS_ABI_O32: u32 = 0x00001000;
pub const E_MIPS_ABI_O64: u32 = 0x00002000;
pub const E_MIPS_ABI_EABI32: u32 = 0x00003000;
pub const E_MIPS_ABI_EABI64: u32 = 0x00004000;

pub const EF_RISCV_RVC: u32 = 0x0001;
pub const EF_RISCV_FLOAT_ABI: u32 = 0x0006;
pub const EF_RISCV_FLOAT_ABI_SOFT: u32 = 0x0000;
pub const EF_RISCV_FLOAT_ABI_SINGLE: u32 = 0x0002;
pub const EF_RISCV_FLOAT_ABI_DOUBLE: u32 = 0x0004;
pub const EF_RISCV_FLOAT_ABI_QUAD: u32 = 0x0006;

pub const EF_LOONGARCH_ABI_LP64_SOFT_FLOAT: u32 = 0b001;
pub const EF_LOONGARCH_ABI_LP64_SINGLE_FLOAT: u32 = 0b010;
pub const EF_LOONGARCH_ABI_LP64_DOUBLE_FLOAT: u32 = 0b011;
pub const EF_LOONGARCH_ABI_ILP32_SOFT_FLOAT: u32 = 0b101;
pub const EF_LOONGARCH_ABI_ILP32_SINGLE_FLOAT: u32 = 0b110;
pub const EF_LOONGARCH_ABI_ILP32_DOUBLE_FLOAT: u32 = 0b111;
pub const EF_LOONGARCH_ABI_MASK: u32 = 0x07;