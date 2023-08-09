# Building
WORD := 64
COLLECTION := gc
ISA := riscv$(WORD)
ISAC := $(ISA)$(COLLECTION)
BOARD := qemu
SBI := rustsbi
MODE := release
TARGET := $(ISAC)-unknown-none-elf
TARGET_DIR := ./target/$(TARGET)
BOOT_DIR := ./boot
LINKER_DIR := ./linker

BOOTLOADER := $(BOOT_DIR)/$(SBI)-$(BOARD).bin
LINKERLD := $(LINKER_DIR)/$(SBI)-$(BOARD).ld
KERNEL_ELF := $(TARGET_DIR)/$(MODE)/frontier_kernel
DISASM_TMP := $(TARGET_DIR)/$(MODE)/asm
KERNEL_BIN := $(KERNEL_ELF).bin

# Building mode argument
ifeq ($(MODE), release)
	MODE_ARG := --$(MODE)
endif

# kernel's entrypoint offset
# In qemu virtual board, The start-up phase of the machine is divided into three stages:
# stage 1. Load bootloader binary file as bios into memory (The point of start address is 0x80000000),
#	 load kernel binary file as virtual device into memory.
# stage 2. The PC(Program Counter) was init as 0x1000 and execute several instructions,
#    then jump to 0x80000000 and hand the control flow to bios (Which was loaded into memory by stage 1).
# stage 3. The bootloader will execute some instructions and jump to 0x80200000
KERNEL_ENTRY_OFFSET := 0x80200000

# Binutils
OBJDUMP := rust-objdump --arch-name=$(ISA)
OBJCOPY := rust-objcopy --binary-architecture=$(ISA)

# Install rust dependencies if not already installed
env:
	(rustup target list | grep "$(TARGET) (installed)") || rustup target add $(TARGET)
	cargo install cargo-binutils
	rustup component add rust-src
	rustup component add llvm-tools-preview

# Print out the build infos
version:
	@echo Platform: $(BOARD)
	@echo Word Length: $(WORD)
	@echo Instruction Set Architecture and Collection: $(ISAC)
	@echo Supervisor Binary Interface: $(SBI)
	@echo Target: $(TARGET)

# Show qemu version info
qemu-version:
	@qemu-riscv64 --version
	@qemu-system-riscv64 --version

show-kernel-elf-stat:
	@echo "####################### show kernel elf stat #######################"
	@stat $(KERNEL_ELF)
	@echo "\n\n\n"

show-kernel-elf-meta:
	@echo "####################### show kernel elf meta #######################"
	@rust-readobj -h $(KERNEL_ELF)
	@echo "\n\n\n"

show-kernel-bin-stat:
	@echo "####################### show kernel bin #######################"
	@stat $(KERNEL_BIN)
	@echo "\n\n\n"

show-kernel-elf-section:
	@$(OBJDUMP) -x $(KERNEL_ELF) | less

show-disassembly-code:
	@$(OBJDUMP) -d $(KERNEL_ELF) | less

# Build the kernel binary
$(KERNEL_ELF): version
	@cd ../user && make build
	@echo "####################### build kernel elf #######################"
	@echo Use $(LINKERLD) as linker.ld
	@cp $(LINKERLD) src/linker.ld
	@cargo build $(MODE_ARG)
	@rm src/linker.ld
	@echo "\n\n\n"


# Strip empty alignment and regenerated
$(KERNEL_BIN): $(KERNEL_ELF)
	@echo "####################### build kernel bin #######################"
	@$(OBJCOPY) $(KERNEL_ELF) --strip-all -O binary $@
	@echo "\n\n\n"

# Buld the kernel
build: $(KERNEL_BIN) show-kernel-elf-stat show-kernel-bin-stat

# Build the kernel and run it in qemu
run-qemu-with-riscv64: $(KERNEL_BIN)
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios $(BOOTLOADER) \
		-device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_OFFSET) \

# Run gdb client with riscv64
run-gdbclient-with-riscv64:
	@riscv64-unknown-elf-gdb-py \
		-ex 'file $(KERNEL_ELF)' \
		-ex 'set arch riscv:rv64' \
		-ex 'target remote localhost:1234'

# Run tmux and split two windows with gdbclient and qemu
debug-qemu-with-riscv64:
	@tmux new-session -d \
		"qemu-system-riscv64 -machine virt -nographic -bios $(BOOTLOADER) -device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_OFFSET) -s -S" && \
		tmux split-window -h "riscv64-unknown-elf-gdb-py -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

.PHONY: \
	env \
	version \
	show-kernel-elf-stat \
	show-kernel-elf-meta \
	show-kernel-elf-section \
	show-kernel-bin-meta \
	show-disassembly-code \
	build \
	run-qemu-with-riscv64-kernel \
	run-gdbclient-with-riscv64 \
	debug-qemu-with-riscv64

