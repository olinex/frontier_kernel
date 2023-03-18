/* set archtecture as riscv */
OUTPUT_ARCH(riscv)

/* set entry point as symbol '_addr_start' which was defined in src/assemble/entry.asm */
ENTRY(_addr_start)

/* set base address */
BASE_ADDRESS = 0x80200000;

SECTIONS
{
    /* rewrite current address to base address */
    . = BASE_ADDRESS;
    /* declare kernel start address */
    _addr_start_kernel = .;

    /*  the first section was code text */
    _addr_start_text = .;
    /* declare the .text section distribution */
    .text : {
        /* sub sction .text.entry will be the first part of .text section */
        *(.text.entry)
        *(.text .text.*)
    }

    /* declare inited read only data section */
    . = ALIGN(4K);
    _addr_end_text = .;
    _addr_start_rodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(._addr_start_rodata ._addr_start_rodata.*)
    }

    /* declare inited read write data section */
    . = ALIGN(4K);
    _addr_end_rodata = .;
    _addr_start_data = .;
    .data : {
        *(.data .data.*)
        *(._addr_start_data ._addr_start_data.*)
        *(.data.bootstack)
    }

    /* declare uninit data section */
    . = ALIGN(4K);
    _addr_end_data = .;
    _addr_start_bss = .;
    .bss : {
        *(.bss .bss.*)
        *(._addr_start_bss ._addr_start_bss.*)
    }

    . = ALIGN(4K);
    _addr_end_bss = .;
    _addr_end_kernel = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}