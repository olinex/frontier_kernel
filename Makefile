# Building
WORD := 64
COLLECTION := gc
ISA := riscv$(WORD)
ISAC := $(ISA)$(COLLECTION)
BOARD := qemu
SBI := rustsbi
MODE := release
CPU := unknown
TARGET := $(ISAC)-$(CPU)-none-elf
TARGET_DIR := ./target/$(TARGET)
RUNTIME_DIR := ./runtime
LINKER_DIR := ./linker
TEST_COMMAND := noneOfTest

RUNTIME := $(RUNTIME_DIR)/$(SBI)-$(BOARD).bin
SOURCE_MEMORY_LINKERLD := $(LINKER_DIR)/$(SBI)-$(BOARD)-memory.ld
TARGET_MEMORY_LINKERLD := $(LINKER_DIR)/memory.ld
LINKERLD := $(LINKER_DIR)/$(SBI).ld
DISASM_TMP := $(TARGET_DIR)/$(MODE)/asm
KERNEL_ELF := $(TARGET_DIR)/$(MODE)/frontier_kernel
KERNEL_BIN := $(KERNEL_ELF).bin
SOURCE_TEST_KERNEL_ELF := $(TARGET_DIR)/$(MODE)/deps/frontier_kernel-*[^\.d]
TEST_KERNEL_ELF := $(TARGET_DIR)/$(MODE)/frontier_kernel-unittest
TEST_KERNEL_BIN := $(TEST_KERNEL_ELF).bin

# Binutils
OBJDUMP := rust-objdump --arch-name=$(ISA)
OBJCOPY := rust-objcopy --binary-architecture=$(ISA)

ifeq ($(MODE), release)
	MODE_ARG := --release
endif

ifeq ($(TEST_COMMAND), noneOfTest)
	TEST_COMMAND_ARG := 
else
	TEST_COMMAND_ARG := --tests
endif

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

show-kernel-elf-header:
	@echo "####################### show kernel elf header #######################"
	@readelf -h $(KERNEL_ELF)
	@echo "\n\n\n"

show-kernel-elf-section:
	@$(OBJDUMP) -x $(KERNEL_ELF) | less

show-kernel-bin-stat:
	@echo "####################### show kernel bin #######################"
	@stat $(KERNEL_BIN)
	@echo "\n\n\n"

show-disassembly-code:
	@$(OBJDUMP) -d $(KERNEL_ELF) | less

# Build the kernel binary
$(KERNEL_ELF): version
	@cd ../frontier_user && make build
	@echo "####################### build kernel elf #######################"
	@echo Use $(LINKERLD) $(SOURCE_MEMORY_LINKERLD)
	@cp $(SOURCE_MEMORY_LINKERLD) $(TARGET_MEMORY_LINKERLD)
	cargo build $(MODE_ARG)
	@rm $(TARGET_MEMORY_LINKERLD)
	@echo "\n\n\n"

# Strip empty alignment and regenerated
$(KERNEL_BIN): $(KERNEL_ELF)
	@echo "####################### build kernel bin #######################"
	@$(OBJCOPY) $(KERNEL_ELF) --strip-all -O binary $@
	@echo "\n\n\n"

# Build the unittest kernel binary
$(TEST_KERNEL_ELF): version
	@cd ../frontier_user && make build
	@echo "####################### build kernel elf #######################"
	@echo Use $(LINKERLD) $(SOURCE_MEMORY_LINKERLD)
	@cp $(SOURCE_MEMORY_LINKERLD) $(TARGET_MEMORY_LINKERLD)
	cargo test $(TEST_COMMAND_ARG) $(MODE_ARG) --no-run
	mv -f $(SOURCE_TEST_KERNEL_ELF) $(TEST_KERNEL_ELF)
	@rm $(TARGET_MEMORY_LINKERLD)
	@echo "\n\n\n"

# Strip empty alignment and regenerated
$(TEST_KERNEL_BIN): $(TEST_KERNEL_ELF)
	@echo "####################### build kernel bin #######################"
	@$(OBJCOPY) $(TEST_KERNEL_ELF) --strip-all -O binary $@
	@echo "\n\n\n"

# Build the kernel
build: $(KERNEL_BIN) show-kernel-elf-stat show-kernel-bin-stat

# Build the test kernel
build-test: $(TEST_KERNEL_BIN)

# Build the kernel and run it in qemu
run-qemu-with-riscv64: build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios $(RUNTIME) \
		-device loader,file=$(KERNEL_ELF)

# Build the kernel and run it in qemu
test-qemu-with-riscv64: build-test
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios $(RUNTIME) \
		-device loader,file=$(TEST_KERNEL_ELF)

# Run tmux and split two windows with gdbclient and qemu
debug-qemu-with-riscv64: build
	@tmux new-session -d \
		"qemu-system-riscv64 -machine virt -nographic -bios $(RUNTIME) -device loader,file=$(KERNEL_ELF) -s -S" && \
		tmux split-window -h "riscv64-unknown-elf-gdb-py -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

.PHONY: \
	env \
	version \
	qemu-version \
	show-kernel-elf-stat \
	show-kernel-elf-header \
	show-kernel-elf-section \
	show-kernel-bin-stat \
	show-disassembly-code \
	build \
	build-test \
	run-qemu-with-riscv64 \
	test-qemu-with-riscv64 \
	debug-qemu-with-riscv64

