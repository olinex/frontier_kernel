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
USER_TARGET_DIR := ../frontier_user/target/$(TARGET)
RUNTIME_DIR := ./runtime
LINKER_DIR := ./linker
TEST_COMMAND := noneOfTest
USER_FS_IMG := user-fs.img

RUNTIME := $(RUNTIME_DIR)/$(SBI)-$(BOARD).bin
SOURCE_MEMORY_LINKERLD := $(LINKER_DIR)/$(ISA)/$(SBI)-$(BOARD)-memory.ld
TARGET_MEMORY_LINKERLD := $(LINKER_DIR)/$(ISA)/memory.ld
LINKERLD := $(LINKER_DIR)/$(ISA)/$(SBI).ld
DISASM_TMP := $(TARGET_DIR)/$(MODE)/asm
KERNEL_ELF := $(TARGET_DIR)/$(MODE)/frontier_kernel
KERNEL_BIN := $(KERNEL_ELF).bin
SOURCE_TEST_KERNEL_ELF := $(TARGET_DIR)/$(MODE)/deps/frontier_kernel-*
TEST_KERNEL_ELF := $(TARGET_DIR)/$(MODE)/frontier_kernel_unittest
TEST_KERNEL_BIN := $(TEST_KERNEL_ELF).bin
USER_FS_IMG_PATH := $(USER_TARGET_DIR)/$(MODE)/$(USER_FS_IMG)
QEMU_COMMAND_ARGS := -machine virt \
	-nographic \
	-bios $(RUNTIME) \
	-drive file=$(USER_FS_IMG_PATH),if=none,format=raw,id=x0 \
	-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0


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

# Output device tree specification
qemu-dtc:
	@qemu-system-$(ISA) -machine virt -machine dumpdtb=$(TARGET_DIR)/$(BOARD)-virt.dtb -bios default
	@dtc -I dtb -O dts -o $(TARGET_DIR)/$(BOARD)-virt.dts $(TARGET_DIR)/$(BOARD)-virt.dtb
	@rm -rf $(TARGET_DIR)/$(BOARD)-virt.dtb
	@less $(TARGET_DIR)/$(BOARD)-virt.dts
	@rm -rf $(TARGET_DIR)/$(BOARD)-virt.dts

# Show qemu version info
qemu-version:
	@qemu-$(ISA) --version
	@qemu-system-$(ISA) --version

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
	@rm -rf $(SOURCE_TEST_KERNEL_ELF)
	@cargo test $(TEST_COMMAND_ARG) $(MODE_ARG) --no-run
	@rm -rf $(SOURCE_TEST_KERNEL_ELF).d
	@mv -f $(SOURCE_TEST_KERNEL_ELF) $(TEST_KERNEL_ELF)
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
run-with-qemu: build
	@qemu-system-$(ISA) -device loader,file=$(KERNEL_ELF) $(QEMU_COMMAND_ARGS)

# Build the kernel and run it in qemu
test-with-qemu: build-test
	@qemu-system-$(ISA) -device loader,file=$(TEST_KERNEL_ELF) $(QEMU_COMMAND_ARGS)

# Run tmux and split two windows with gdbclient and qemu
debug-with-qemu: build
	@tmux new-session -d \
		"qemu-system-$(ISA) -device loader,file=$(KERNEL_ELF) $(QEMU_COMMAND_ARGS) -s -S" && \
		tmux split-window -h "$(ISA)-unknown-elf-gdb -nw -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

clear:
	@rm -rf $(TARGET_MEMORY_LINKERLD) \
		$(DISASM_TMP) \
		$(KERNEL_ELF) \
		$(KERNEL_BIN) \
		$(SOURCE_TEST_KERNEL_ELF) \
		$(TEST_KERNEL_ELF) \
		$(TEST_KERNEL_BIN)

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
	run-with-qemu \
	test-with-qemu \
	debug-with-qemu \
	clear

