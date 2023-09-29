/* 
    kernel's entrypoint offset
    In qemu virtual board, The start-up phase of the machine is divided into three stages:
    stage 1. Load bootloader binary file as bios into memory (The point of start address is 0x80000000),
    	load kernel binary file as virtual device into memory.
    stage 2. The PC(Program Counter) was init as 0x1000 and execute several instructions,
        then jump to 0x80000000 and hand the control flow to bios (Which was loaded into memory by stage 1).
    stage 3. The bootloader will execute some instructions and jump to 0x80200000
    KERNEL_ENTRY_OFFSET := 0x80200000
*/
MEMORY
{
    /*
        1. the physical memory address was start with 0x80000000
        so bios and sbi will use 2MiB(to 0x80200000)
        and the physical memory size will be 128MiB as default.

        2. the origin physical memory address must be divisible by 4k
    */
    RAM : ORIGIN = 0x80200000, LENGTH = 126M
}